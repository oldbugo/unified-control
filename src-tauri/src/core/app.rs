//! The core application object: settings, scheduler and events. Copied from CoolerScreen's `core/app.rs` (the shell parts:
//! `Events`, `Debounce`, the settings change hook and the scheduler); the library, media, sources, cooler device and live host
//! are not here. Tauri commands (core/commands.rs) are thin wrappers over `App` methods so everything can be tested without a
//! window.
use super::error::{CoreError, CoreResult};
use super::scheduler::{Job, JobSink, Limits, Policy, Scheduler, TaskCtx, TaskSpec};
use super::settings::SettingsStore;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

/// What the UI is told (Tauri events in the app, a recorder in tests).
pub trait Events: Send + Sync + 'static {
    fn agent(&self, _event: &str, _payload: Value) {}
    fn cooler(&self, _event: &str, _payload: Value) {}
    fn job(&self, _job: &Job) {}
    /// Settings changed: the UI reloads `app_state` (debounced).
    fn app_changed(&self) {}
    fn device_busy(&self, _busy: bool) {}
}

pub struct NoEvents;
impl Events for NoEvents {}

struct SinkAdapter(Arc<dyn Events>);
impl JobSink for SinkAdapter {
    fn job(&self, job: &Job) {
        self.0.job(job)
    }
    fn device_busy(&self, busy: bool) {
        self.0.device_busy(busy)
    }
}

/// Calls `f` at most once per `delay` after a burst of `poke`s.
pub struct Debounce {
    pending: AtomicBool,
    delay: Duration,
    f: Box<dyn Fn() + Send + Sync>,
}

impl Debounce {
    pub fn new(delay: Duration, f: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Debounce {
            pending: AtomicBool::new(false),
            delay,
            f: Box::new(f),
        })
    }
    pub fn poke(self: &Arc<Self>) {
        if self.pending.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(me.delay);
            me.pending.store(false, Ordering::SeqCst);
            (me.f)();
        });
    }
}

pub struct App {
    pub agent: super::agent::AgentState,
    pub settings: Arc<SettingsStore>,
    pub profiles: Arc<super::profiles::ProfileStore>,
    pub runtime: OnceLock<Arc<super::runtime_resources::RuntimeResources>>,
    pub dock: OnceLock<Arc<super::dock_service::Controller>>,
    pub cooler: OnceLock<Arc<super::cooler::Cooler>>,
    cooler_config_lock: Mutex<()>,
    cooler_migration_report: Mutex<Value>,
    cooler_migration_job: Mutex<Option<String>>,
    pub(crate) closing: AtomicBool,
    dock_config_lock: Mutex<()>,
    pub sched: Scheduler,
    pub events: Arc<dyn Events>,
    pub app_version: String,
    /// The app folder (%APPDATA%\UnifiedControl or UC_HOME).
    pub base: PathBuf,
    pub(crate) me: Mutex<Weak<App>>,
}

impl App {
    /// `base` = app folder (%APPDATA%\UnifiedControl or UC_HOME).
    pub fn new(base: impl Into<PathBuf>, events: Arc<dyn Events>, app_version: &str) -> Arc<App> {
        let base = base.into();
        let settings = Arc::new(SettingsStore::new(base.join("settings.json")));
        let profiles = Arc::new(super::profiles::ProfileStore::new(&base));
        let sched = Scheduler::new(
            Limits::default(),
            Policy::default(),
            Arc::new(SinkAdapter(events.clone())),
        );
        let migration_report = super::cooler_migration_audit::read_receipt(
            &base.join("cooler-migration-verification.json"),
        )
        .ok()
        .flatten()
        .unwrap_or_else(|| json!({"state":"not_checked","verified":false}));
        let app = Arc::new(App {
            agent: super::agent::AgentState::default(),
            settings: settings.clone(),
            profiles,
            runtime: OnceLock::new(),
            dock: OnceLock::new(),
            cooler: OnceLock::new(),
            cooler_config_lock: Mutex::new(()),
            cooler_migration_report: Mutex::new(migration_report),
            cooler_migration_job: Mutex::new(None),
            closing: AtomicBool::new(false),
            dock_config_lock: Mutex::new(()),
            sched,
            events: events.clone(),
            app_version: app_version.to_string(),
            base,
            me: Mutex::new(Weak::new()),
        });
        *app.me.lock().unwrap() = Arc::downgrade(&app);
        let ev = events.clone();
        let app_deb = Debounce::new(Duration::from_millis(100), move || ev.app_changed());
        settings.set_on_change(move || app_deb.poke());
        app
    }

    pub fn arc(&self) -> Arc<App> {
        self.me.lock().unwrap().upgrade().expect("App alive")
    }
    /// Called once by the host. It starts disabled unless the user previously
    /// enabled the dock; fake mode never opens native devices or account readers.
    pub fn start_runtime(&self, resource_dir: PathBuf) -> CoreResult<()> {
        if self.dock.get().is_some() {
            return Ok(());
        }
        let fake = super::buildinfo::fake_device().is_some();
        let cooler_events = self.events.clone();
        let cooler = super::cooler::Cooler::new(
            self.base.join("devices/cooler"),
            resource_dir.clone(),
            fake,
            Arc::new(move |event, payload| cooler_events.cooler(event, payload)),
        );
        let _ = self.cooler.set(cooler.clone());
        let resources =
            super::runtime_resources::RuntimeResources::new(&self.base, resource_dir, fake);
        let controller = Arc::new(super::dock_service::Controller::new(
            self.profiles.clone(),
            resources.clone(),
            fake,
        ));
        let settings = self.settings.load();
        cooler.set_diagnostics(
            settings.0["cooler"]["diagnostics"]
                .as_bool()
                .unwrap_or(true),
        );
        controller
            .set_brightness(settings.0["dock"]["brightness"].as_u64().unwrap_or(50) as u8)
            .map_err(CoreError::new)?;
        self.runtime
            .set(resources)
            .map_err(|_| CoreError::new("Runtime is already initialized"))?;
        self.dock
            .set(controller.clone())
            .map_err(|_| CoreError::new("Dock is already initialized"))?;
        controller
            .set_enabled(settings.0["dock"]["enabled"].as_bool().unwrap_or(false))
            .map_err(CoreError::new)?;
        if settings.0["cooler"]["enabled"] == true {
            match self.prepare_cooler_storage() {
                Ok(()) => {
                    let _ = cooler.configure(true);
                    if !self.migration_verified_before() {
                        let _ = self.verify_cooler_migration();
                    }
                }
                Err(_) => {
                    *self.cooler_migration_report.lock().unwrap() = json!({"state":"attention","verified":false,"code":"migration_setup_failed"});
                }
            }
        }
        Ok(())
    }
    pub fn cooler_state(&self) -> Value {
        let mut state = self.cooler.get().map(|cooler|cooler.status()).unwrap_or_else(||json!({"enabled":false,"state":"disabled","message":"Cooler screen is disabled","busy":false}));
        // Scheduler state only: status never reopens or scans migration files.
        let current = self.cooler_migration_job.lock().unwrap();
        let mut cached = self.cooler_migration_report.lock().unwrap();
        if cached["state"] == "checking"
            && current.as_ref().is_some_and(|id| {
                self.sched
                    .job(id)
                    .map(|job| job.state.finished())
                    .unwrap_or(true)
            })
        {
            *cached = json!({"state":"attention","verified":false,"code":"verification_cancelled","checkedAt":crate::device::log::iso_now()});
        }
        let migration = cached.clone();
        drop(cached);
        drop(current);
        if migration["code"] == "migration_setup_failed"
            && self.settings.load().0["cooler"]["enabled"] == true
            && state["enabled"] == false
        {
            state["enabled"] = json!(true);
            state["state"] = json!("blocked");
            state["message"] =
                json!("The cooler import needs attention. Existing files were preserved.");
        }
        state["migration"] = migration;
        state
    }

    fn cooler_storage_paths(&self) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        if super::buildinfo::fake_device().is_some() {
            return (
                self.base.join("test-empty-cooler-source"),
                self.base.join("devices/cooler"),
                self.base.join("test-empty-gallery"),
                self.base.join("test-empty-sources"),
            );
        }
        let documents = dirs::document_dir().unwrap_or_else(|| self.base.clone());
        (
            self.base
                .parent()
                .unwrap_or(&self.base)
                .join("CoolerScreen"),
            self.base.join("devices/cooler"),
            documents.join("CoolerScreen/Gallery"),
            documents.join("CoolerScreen/Sources"),
        )
    }

    fn prepare_cooler_storage(&self) -> CoreResult<()> {
        let (source, target, gallery, plugins) = self.cooler_storage_paths();
        if !super::buildinfo::fake_device().is_some() && super::cooler::other_app_running() {
            return Err(CoreError::coded(
                "cooler_owned",
                "Close CoolerScreen from its tray before importing its data.",
            ));
        }
        self.run_task(
            TaskSpec::new(
                "cooler-import",
                "Preparing cooler settings and library",
                super::scheduler::Resources::one(super::scheduler::Res::Io),
            )
            .visible(),
            move |_| {
                super::cooler_migration::prepare(&source, &target, &gallery)?;
                super::cooler_migration::upgrade_sources(&plugins, &target)?;
                Ok(json!({"ok":true}))
            },
        )?;
        Ok(())
    }

    fn migration_verified_before(&self) -> bool {
        let report = self.cooler_migration_report.lock().unwrap();
        report["reportVersion"] == 1
            && report["migrationVersion"] == 2
            && ((report["state"] == "complete" && report["verified"] == true)
                || report["state"] == "not_required")
    }

    /// Compare in the host's own data namespace. Status reads only expose the
    /// cached receipt; this explicit job never opens a physical device.
    pub fn verify_cooler_migration(&self) -> CoreResult<Value> {
        let (source, target, gallery, _) = self.cooler_storage_paths();
        self.verify_cooler_migration_paths(source, target, gallery)
    }
    fn verify_cooler_migration_paths(
        &self,
        source: PathBuf,
        target: PathBuf,
        gallery: PathBuf,
    ) -> CoreResult<Value> {
        let _admission = self.cooler_config_lock.lock().unwrap();
        if self.closing.load(Ordering::SeqCst) {
            return Err(CoreError::new("The app is closing"));
        }
        let mut current = self.cooler_migration_job.lock().unwrap();
        if let Some(id) = current.as_ref() {
            if self.sched.job(id).is_some_and(|job| !job.state.finished()) {
                return Ok(json!({"jobId":id}));
            }
        }
        let receipt = self.base.join("cooler-migration-verification.json");
        // Invalidate any previous success before admission. Pending survives
        // cancellation or a failed receipt write, including across restarts.
        super::cooler_migration_audit::begin_receipt(&receipt)?;
        *self.cooler_migration_report.lock().unwrap() =
            json!({"state":"checking","verified":false});
        let app = self.arc();
        let job = self.sched.submit(
            TaskSpec::new(
                "cooler-migration-check",
                "Verifying cooler migration",
                super::scheduler::Resources::one(super::scheduler::Res::Io),
            )
            .visible(),
            move |ctx| {
                for _ in 0..300 {
                    if !app.cooler.get().is_some_and(|cooler| cooler.busy()) {
                        break;
                    }
                    ctx.sleep(Duration::from_millis(100))?;
                }
                ctx.check()?;
                let import = super::cooler_migration::migration_summary(&target).ok();
                let ready = import.as_ref().is_some_and(|v| {
                    v["version"] == 2
                        && v["sourcePlugins"]["imported"] == true
                        && v["sourcePlugins"]["folderPresent"] == true
                });
                let mut report = if !ready {
                    json!({"verified":false,"code":"sources_pending"})
                } else if import
                    .as_ref()
                    .is_some_and(|v| v["copiedExisting"] == false)
                {
                    json!({"verified":false,"code":"new_setup","state":"not_required"})
                } else if app.cooler.get().is_some_and(|cooler| cooler.busy()) {
                    json!({"verified":false,"code":"audit_failed"})
                } else {
                    match super::cooler_migration_audit::audit_with_cancel(
                        &source,
                        &target,
                        &gallery,
                        || ctx.check(),
                    ) {
                        Ok(report) => report,
                        Err(error) if error.is_cancelled() => return Err(error),
                        Err(_) => json!({"verified":false,"code":"audit_failed"}),
                    }
                };
                ctx.check()?;
                report["reportVersion"] = json!(1);
                if report["state"] != "not_required" {
                    report["state"] = json!(if report["verified"] == true {
                        "complete"
                    } else {
                        "attention"
                    });
                }
                report["checkedAt"] = json!(crate::device::log::iso_now());
                report["migrationVersion"] = json!(import
                    .as_ref()
                    .and_then(|v| v["version"].as_u64())
                    .unwrap_or(0));
                report["originalKept"] =
                    json!(import.as_ref().is_some_and(|v| v["originalKept"] == true));
                if let Some(import) = import {
                    report["sourcePlugins"] = import["sourcePlugins"].clone();
                }
                if super::cooler_migration_audit::write_receipt(&receipt, &report).is_err() {
                    report["verified"] = json!(false);
                    report["state"] = json!("attention");
                    report["code"] = json!("receipt_write_failed");
                }
                *app.cooler_migration_report.lock().unwrap() = report.clone();
                app.events.app_changed();
                Ok(report)
            },
        );
        *current = Some(job.id.clone());
        Ok(json!({"jobId":job.id}))
    }
    pub fn cooler_configure(&self, enabled: bool) -> CoreResult<Value> {
        if self.closing.load(Ordering::SeqCst) {
            return Err(CoreError::new("The app is closing"));
        }
        let _guard = self.cooler_config_lock.lock().unwrap();
        if self.closing.load(Ordering::SeqCst) {
            return Err(CoreError::new("The app is closing"));
        }
        let cooler = self
            .cooler
            .get()
            .ok_or_else(|| CoreError::new("Cooler engine is unavailable"))?
            .clone();
        if enabled {
            self.prepare_cooler_storage()?;
        }
        let old = self.settings.load().0["cooler"]["enabled"]
            .as_bool()
            .unwrap_or(false);
        self.settings
            .update(|s| s["cooler"]["enabled"] = json!(enabled))
            .map_err(CoreError::from)?;
        match cooler.configure(enabled) {
            Ok(_) => {}
            Err(error) => {
                self.settings
                    .update(|s| s["cooler"]["enabled"] = json!(old))
                    .map_err(CoreError::from)?;
                return Err(error);
            }
        };
        drop(_guard);
        if enabled && !self.migration_verified_before() {
            let _ = self.verify_cooler_migration();
        }
        Ok(self.cooler_state())
    }
    pub fn cooler_call(&self, command: &str, args: Value) -> CoreResult<Value> {
        if self.closing.load(Ordering::SeqCst) {
            return Err(CoreError::new("The app is closing"));
        }
        self.cooler
            .get()
            .ok_or_else(|| CoreError::new("Cooler engine is unavailable"))?
            .call(command, args)
    }
    pub fn dock_state(&self) -> Value {
        let mut value=self.dock.get().map(|dock|serde_json::to_value(dock.status()).unwrap_or_default()).unwrap_or_else(||json!({"state":"disabled","message":"Dock is disabled","enabled":false,"fake":super::buildinfo::fake_device().is_some()}));
        value["brightness"] = self.settings.load().0["dock"]["brightness"].clone();
        value["usageReaderRunning"] = json!(self
            .runtime
            .get()
            .is_some_and(|runtime| runtime.usage.is_running()));
        if let Some(runtime) = self.runtime.get() {
            value["usageStatus"] = runtime.usage_status();
        }
        if let Some(id) = value["currentProfile"].as_str().map(String::from) {
            if let Ok(profiles) = self.profiles.state() {
                if let Some(profile) = profiles.profiles.iter().find(|p| p.id == id) {
                    value["currentProfileName"] = json!(profile.name);
                    if let Some(page) = profile
                        .pages
                        .iter()
                        .find(|p| Some(p.id.as_str()) == value["currentPage"].as_str())
                    {
                        value["currentPageName"] = json!(page.name);
                    }
                }
            }
        }
        value
    }
    pub fn dock_configure(&self, patch: &Value) -> CoreResult<Value> {
        let _guard = self.dock_config_lock.lock().unwrap();
        let controller = self
            .dock
            .get()
            .ok_or_else(|| CoreError::new("Dock controller is unavailable"))?;
        if patch.get("enabled").is_some_and(|v| !v.is_boolean())
            || patch
                .get("brightness")
                .is_some_and(|v| v.as_u64().is_none_or(|n| n > 100))
        {
            return Err(CoreError::new("Invalid dock setting"));
        }
        let saved = self
            .settings
            .update(|state| {
                if let Some(v) = patch.get("enabled") {
                    state["dock"]["enabled"] = v.clone()
                }
                if let Some(v) = patch.get("brightness") {
                    state["dock"]["brightness"] = v.clone()
                }
            })
            .map_err(CoreError::from)?;
        controller
            .set_brightness(saved.0["dock"]["brightness"].as_u64().unwrap_or(50) as u8)
            .map_err(CoreError::new)?;
        controller
            .set_enabled(saved.0["dock"]["enabled"].as_bool().unwrap_or(false))
            .map_err(CoreError::new)?;
        if !saved.0["dock"]["enabled"].as_bool().unwrap_or(false) {
            if let Some(runtime) = self.runtime.get() {
                runtime.shutdown();
            }
        }
        Ok(self.dock_state())
    }

    /// Run a task through the scheduler and wait for it (on the calling thread).
    pub fn run_task(
        &self,
        spec: TaskSpec,
        f: impl Fn(&TaskCtx) -> CoreResult<Value> + Send + Sync + 'static,
    ) -> CoreResult<Value> {
        self.sched.submit(spec, f).wait()
    }

    pub fn settings_changed(&self) {
        self.events.app_changed();
    }

    /// What the window shows: version, build, fake-device mode and the settings.
    pub fn app_state(&self) -> Value {
        let s = self.settings.load();
        json!({
            "appVersion": self.app_version,
            "build": { "hash": super::buildinfo::HASH, "commitDate": super::buildinfo::COMMIT_DATE, "builtAt": super::buildinfo::built_at(), "note": super::buildinfo::screen_note() },
            "fake": super::buildinfo::fake_device(),
            "page": s.page(),
            "followPhysicalPage": s.0["ui"]["followPhysicalPage"].as_bool().unwrap_or(true),
            "agentAccess": s.0["ui"]["agentAccess"].as_bool().unwrap_or(false),
            "agentBusy": self.agent_busy(),
            "autostart": super::autostart::is_on(),
            "dock": self.dock_state(),
            "cooler": self.cooler_state(),
        })
    }

    /// `{ page? }`: saved at once (no Save button).
    pub fn settings_update(&self, body: &Value) -> CoreResult<Value> {
        let agent_access = body.get("agentAccess");
        if agent_access.is_some_and(|value| !value.is_boolean()) {
            return Err(CoreError::new("Invalid agent access setting"));
        }
        let diagnostics = body.get("coolerDiagnostics");
        if diagnostics.is_some_and(|value| !value.is_boolean()) {
            return Err(CoreError::new("Invalid cooler diagnostics setting"));
        }
        let follow = body.get("followPhysicalPage");
        if follow.is_some_and(|value| !value.is_boolean()) {
            return Err(CoreError::new("Invalid follow-physical-page setting"));
        }
        let page = body.get("page").and_then(Value::as_str).map(String::from);
        if let Some(p) = &page {
            if !super::settings::PAGES.contains(&p.as_str()) {
                return Err(CoreError::new(format!("Unknown page: {p}")));
            }
        }
        self.settings
            .update(|s| {
                if let Some(enabled) = agent_access {
                    s["ui"]["agentAccess"] = enabled.clone();
                }
                if let Some(diagnostics) = diagnostics {
                    s["cooler"]["diagnostics"] = diagnostics.clone();
                }
                if let Some(follow) = follow {
                    s["ui"]["followPhysicalPage"] = follow.clone();
                }
                if let Some(p) = page {
                    s["ui"]["page"] = json!(p);
                }
            })
            .map_err(|e| CoreError::new(e.to_string()))?;
        if let Some(diagnostics) = diagnostics {
            if let Some(cooler) = self.cooler.get() {
                cooler.set_diagnostics(diagnostics.as_bool().unwrap());
            }
        }
        Ok(self.app_state())
    }

    pub fn activity(&self) -> Value {
        json!({ "deviceBusy": self.sched.device_active(), "jobs": self.sched.activity(120_000) })
    }

    pub fn profiles_state(&self) -> CoreResult<Value> {
        let (state, revision) = self
            .profiles
            .state_with_revision()
            .map_err(|e| CoreError::coded("profiles", e.to_string()))?;
        Ok(
            json!({ "state": state, "revision":revision, "notice": self.profiles.recovery_notice() }),
        )
    }

    pub fn save_profiles(&self, snapshot: super::profiles::ProfileState) -> CoreResult<Value> {
        self.save_profiles_versioned(snapshot, None)
    }
    pub fn save_profiles_if_revision(
        &self,
        snapshot: super::profiles::ProfileState,
        expected_revision: String,
    ) -> CoreResult<Value> {
        self.save_profiles_versioned(snapshot, Some(expected_revision))
    }
    fn save_profiles_versioned(
        &self,
        snapshot: super::profiles::ProfileState,
        expected_revision: Option<String>,
    ) -> CoreResult<Value> {
        let store = self.profiles.clone();
        let dock = self.dock.get().cloned();
        // Saves count as visible work: tray Quit and idle-safe installation wait.
        self.run_task(
            TaskSpec::new(
                "profiles-save",
                "Saving dock layout",
                super::scheduler::Resources::one(super::scheduler::Res::Io),
            )
            .visible(),
            move |_| {
                let (state,revision) = if let Some(expected)=&expected_revision {
                    store.save_if_revision(&snapshot,expected)
                } else {
                    store.save_with_revision(&snapshot)
                }.map_err(|e| {if e.is_conflict(){CoreError::coded("revision_conflict","Dock layout changed; read the latest revision and retry").status(409)}else{CoreError::coded("profiles", e.to_string())}})?;
                if let Some(dock) = &dock {
                    dock.notify_profiles_changed();
                }
                Ok(json!({ "state": state, "revision":revision, "notice": store.recovery_notice() }))
            },
        )
    }

    pub fn job_get(&self, id: &str) -> CoreResult<Value> {
        let j = self
            .sched
            .job(id)
            .ok_or_else(|| CoreError::not_found("No such job"))?;
        Ok(serde_json::to_value(j).unwrap_or(Value::Null))
    }

    pub fn shutdown(&self) {
        self.stop_runtime();
        self.sched.cancel_where(|_| true);
        // Give cancelled tasks a moment to stop. Bounded: a running device write cannot be cancelled and is never waited for
        // longer than this.
        self.sched.wait_idle(Duration::from_secs(1), true);
    }
    /// Stop cooler admission before preparing a relaunch helper. Refusal leaves
    /// the host and dock usable; a failed helper can restore the cooler engine.
    pub fn prepare_cooler_exit(&self) -> CoreResult<()> {
        let _guard = self.cooler_config_lock.lock().unwrap();
        if self.sched.any_visible_job() {
            return Err(CoreError::new("Wait for active jobs before quitting."));
        }
        self.agent_begin_close()?;
        self.closing.store(true, Ordering::SeqCst);
        if let Some(cooler) = self.cooler.get() {
            if let Err(error) = cooler.configure(false) {
                self.closing.store(false, Ordering::SeqCst);
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn cancel_cooler_exit(&self) {
        let _guard = self.cooler_config_lock.lock().unwrap();
        self.closing.store(false, Ordering::SeqCst);
        if self.settings.load().0["cooler"]["enabled"] == true {
            if let Some(cooler) = self.cooler.get() {
                let _ = cooler.configure(true);
            }
        }
    }
    /// Must run off the UI thread; active monitor recovery is allowed to finish.
    pub fn stop_runtime(&self) {
        if let Some(cooler) = self.cooler.get() {
            if cooler.shutdown().is_err() {
                return;
            }
        }
        let _guard = self.dock_config_lock.lock().unwrap();
        if let Some(dock) = self.dock.get() {
            dock.shutdown();
        }
        if let Some(resources) = self.runtime.get() {
            resources.shutdown();
        }
    }

    /// `UnifiedControl.exe --quit` refuses while a job is running.
    pub fn busy_for_quit(&self) -> bool {
        self.agent_busy()
            || self.sched.any_visible_job()
            || self.dock.get().is_some_and(|dock| dock.busy())
            || self.cooler.get().is_some_and(|cooler| cooler.busy())
    }

    /// True while something is being written to a device (close guard).
    pub fn uploading(&self) -> bool {
        self.sched.running_kind(&["send", "setup"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scheduler::{Res, Resources};

    fn app() -> (tempfile::TempDir, Arc<App>) {
        let dir = tempfile::tempdir().unwrap();
        let a = App::new(dir.path(), Arc::new(NoEvents), "0.0.0");
        (dir, a)
    }

    #[test]
    fn migration_verification_job_preserves_data_and_caches_its_receipt() {
        let (dir, app) = app();
        let old = dir.path().join("old");
        let gallery = dir.path().join("old-gallery");
        let target = app.base.join("devices/cooler");
        std::fs::create_dir_all(old.join("library/item")).unwrap();
        std::fs::create_dir_all(&gallery).unwrap();
        std::fs::write(gallery.join("clip.bin"), b"original media").unwrap();
        std::fs::write(
            old.join("settings.json"),
            serde_json::to_vec(&json!({"galleryDir":gallery})).unwrap(),
        )
        .unwrap();
        std::fs::write(
            old.join("library/item/original.json"),
            serde_json::to_vec(&json!({"file":gallery.join("clip.bin")})).unwrap(),
        )
        .unwrap();
        std::fs::write(old.join("library/item/theme.json"), b"{\"widgets\":[]}").unwrap();
        super::super::cooler_migration::prepare(&old, &target, &gallery).unwrap();
        super::super::cooler_migration::upgrade_sources(&dir.path().join("no-plugins"), &target)
            .unwrap();
        let edited = b"{\"widgets\":[{\"kind\":\"clock\"}]}";
        std::fs::write(target.join("library/item/theme.json"), edited).unwrap();
        let source_before = std::fs::read(old.join("settings.json")).unwrap();
        let job = app
            .verify_cooler_migration_paths(old.clone(), target.clone(), gallery)
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while app
            .sched
            .job(job["jobId"].as_str().unwrap())
            .is_some_and(|job| !job.state.finished())
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let report = app.cooler_state()["migration"].clone();
        assert_eq!(report["verified"], true, "{report}");
        assert_eq!(report["library"]["changedRecords"], 1);
        assert_eq!(
            std::fs::read(target.join("library/item/theme.json")).unwrap(),
            edited
        );
        assert_eq!(
            std::fs::read(old.join("settings.json")).unwrap(),
            source_before
        );
        let reopened = App::new(&app.base, Arc::new(NoEvents), "test");
        assert_eq!(reopened.cooler_state()["migration"], report);
        assert!(
            !reopened.sched.any_visible_job(),
            "reading the receipt never launches verification"
        );
    }

    #[test]
    fn migration_requires_owned_sources_and_persists_failed_recheck() {
        let (dir, app) = app();
        let old = dir.path().join("old");
        let gallery = dir.path().join("gallery");
        let target = app.base.join("devices/cooler");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("settings.json"), b"{}").unwrap();
        super::super::cooler_migration::prepare(&old, &target, &gallery).unwrap();
        let wait = |value: Value| {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !app
                .sched
                .job(value["jobId"].as_str().unwrap())
                .unwrap()
                .state
                .finished()
            {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        wait(
            app.verify_cooler_migration_paths(old.clone(), target.clone(), gallery.clone())
                .unwrap(),
        );
        assert_eq!(app.cooler_state()["migration"]["code"], "sources_pending");
        super::super::cooler_migration::upgrade_sources(&dir.path().join("no-sources"), &target)
            .unwrap();
        wait(
            app.verify_cooler_migration_paths(old.clone(), target.clone(), gallery.clone())
                .unwrap(),
        );
        assert_eq!(app.cooler_state()["migration"]["verified"], true);
        std::fs::remove_file(target.join("settings.json")).unwrap();
        wait(
            app.verify_cooler_migration_paths(old, target, gallery)
                .unwrap(),
        );
        assert_eq!(app.cooler_state()["migration"]["verified"], false);
        let reopened = App::new(&app.base, Arc::new(NoEvents), "test");
        assert_eq!(reopened.cooler_state()["migration"]["verified"], false);
        assert!(!reopened.migration_verified_before());
    }

    #[test]
    fn migration_cancelled_while_queued_is_retryable_and_blocks_exit_admission() {
        let (dir, app) = app();
        let gate = app.sched.submit(
            TaskSpec::new("io-gate", "fixture", Resources::none().with(Res::Io, 8)),
            |ctx| loop {
                ctx.sleep(Duration::from_millis(10))?;
            },
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while app.sched.job(&gate.id).unwrap().state != super::super::scheduler::JobState::Running {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let target = app.base.join("devices/cooler");
        let old = dir.path().join("no-old");
        let gallery = dir.path().join("no-gallery");
        super::super::cooler_migration::prepare(&old, &target, &gallery).unwrap();
        super::super::cooler_migration::upgrade_sources(&dir.path().join("no-sources"), &target)
            .unwrap();
        let value = app
            .verify_cooler_migration_paths(old.clone(), target.clone(), gallery.clone())
            .unwrap();
        let id = value["jobId"].as_str().unwrap();
        assert_eq!(
            app.sched.job(id).unwrap().state,
            super::super::scheduler::JobState::Queued
        );
        assert!(app.prepare_cooler_exit().is_err());
        assert!(!app.closing.load(Ordering::SeqCst));
        assert!(app.sched.cancel(id));
        assert_eq!(
            app.cooler_state()["migration"]["code"],
            "verification_cancelled"
        );
        let reopened = App::new(&app.base, Arc::new(NoEvents), "test");
        assert_eq!(reopened.cooler_state()["migration"]["state"], "not_checked");
        assert!(app.sched.cancel(&gate.id));
        gate.wait().unwrap_err();
        let retry = app
            .verify_cooler_migration_paths(old, target, gallery)
            .unwrap();
        while !app
            .sched
            .job(retry["jobId"].as_str().unwrap())
            .unwrap()
            .state
            .finished()
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.cooler_state()["migration"]["state"], "not_required");
        app.prepare_cooler_exit().unwrap();
        assert!(app.verify_cooler_migration().is_err());
    }

    #[test]
    fn settings_update_saves_the_page_at_once() {
        let (dir, a) = app();
        assert_eq!(a.app_state()["page"], "home");
        a.settings_update(&json!({ "page": "dock" })).unwrap();
        assert_eq!(a.app_state()["page"], "dock");
        let on_disk: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk["ui"]["page"], "dock");
        for page in ["cooler", "dial"] {
            a.settings_update(&json!({ "page": page })).unwrap();
            assert_eq!(a.app_state()["page"], page);
        }
        assert!(a.settings_update(&json!({ "page": "unknown" })).is_err());
    }

    #[test]
    fn cooler_diagnostics_can_be_disabled_and_survive_restart() {
        let (dir, app) = app();
        app.settings_update(&json!({"coolerDiagnostics":false}))
            .unwrap();
        let reopened = App::new(dir.path(), Arc::new(NoEvents), "test");
        assert_eq!(reopened.settings.load().0["cooler"]["diagnostics"], false);
        assert!(app
            .settings_update(&json!({"coolerDiagnostics":"yes"}))
            .is_err());
    }

    #[test]
    fn physical_page_follow_is_saved_without_changing_profiles() {
        let (dir, app) = app();
        let profiles = app.profiles.state().unwrap();
        assert_eq!(app.app_state()["followPhysicalPage"], true);
        app.settings_update(&json!({"followPhysicalPage":false}))
            .unwrap();
        let reopened = App::new(dir.path(), Arc::new(NoEvents), "test");
        assert_eq!(reopened.app_state()["followPhysicalPage"], false);
        assert_eq!(reopened.profiles.state().unwrap(), profiles);
        assert!(app
            .settings_update(&json!({"followPhysicalPage":"yes"}))
            .is_err());
        assert_eq!(app.app_state()["followPhysicalPage"], false);
    }
    #[test]
    fn profile_commands_persist_and_reject_invalid_snapshots() {
        let (dir, app) = app();
        let value = app.profiles_state().unwrap();
        let mut state: super::super::profiles::ProfileState =
            serde_json::from_value(value["state"].clone()).unwrap();
        state.profiles[0].pages[0].keys[0].a.text = "Focus".into();
        app.save_profiles(state.clone()).unwrap();
        assert_eq!(
            super::super::profiles::ProfileStore::new(dir.path())
                .load()
                .unwrap(),
            state
        );
        let mut invalid = state.clone();
        invalid.profiles[0].id = "../outside".into();
        assert!(app.save_profiles(invalid).is_err());
        assert_eq!(app.profiles.state().unwrap(), state);
    }

    #[test]
    fn busy_while_a_job_runs_and_idle_after() {
        let (_dir, a) = app();
        assert!(!a.busy_for_quit());
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let rx = Mutex::new(rx);
        let h = a.sched.submit(
            TaskSpec::new("send", "Sending a page", Resources::one(Res::Device)).visible(),
            move |_| {
                let _ = rx.lock().unwrap().recv_timeout(Duration::from_secs(5));
                Ok(Value::Null)
            },
        );
        let t0 = std::time::Instant::now();
        while !a.busy_for_quit() && t0.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            a.busy_for_quit(),
            "a running job makes the app busy (quit and installs wait)"
        );
        assert!(a.uploading());
        tx.send(()).unwrap();
        let _ = h.wait();
        assert!(!a.busy_for_quit());
    }
}
