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
    pub settings: Arc<SettingsStore>,
    pub profiles: Arc<super::profiles::ProfileStore>,
    pub runtime: OnceLock<Arc<super::runtime_resources::RuntimeResources>>,
    pub dock: OnceLock<Arc<super::dock_service::Controller>>,
    pub cooler: OnceLock<Arc<super::cooler::Cooler>>,
    cooler_config_lock: Mutex<()>,
    closing: AtomicBool,
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
        let app = Arc::new(App {
            settings: settings.clone(),
            profiles,
            runtime: OnceLock::new(),
            dock: OnceLock::new(),
            cooler: OnceLock::new(),
            cooler_config_lock: Mutex::new(()),
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
        if settings.0["cooler"]["enabled"] == true
            && self.base.join("devices/cooler/migration.json").is_file()
        {
            let _ = cooler.configure(true);
        }
        Ok(())
    }
    pub fn cooler_state(&self) -> Value {
        self.cooler.get().map(|cooler|cooler.status()).unwrap_or_else(||json!({"enabled":false,"state":"disabled","message":"Cooler screen is disabled","busy":false}))
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
        if enabled && !self.base.join("devices/cooler/migration.json").is_file() {
            if !super::buildinfo::fake_device().is_some() && super::cooler::other_app_running() {
                return Err(CoreError::coded("cooler_owned","Close CoolerScreen from its tray before enabling its engine in Unified Control."));
            }
            let source = if super::buildinfo::fake_device().is_some() {
                self.base.join("test-empty-cooler-source")
            } else {
                self.base
                    .parent()
                    .unwrap_or(&self.base)
                    .join("CoolerScreen")
            };
            let target = self.base.join("devices/cooler");
            let gallery = if super::buildinfo::fake_device().is_some() {
                self.base.join("test-empty-gallery")
            } else {
                dirs::document_dir()
                    .unwrap_or_else(|| self.base.clone())
                    .join("CoolerScreen/Gallery")
            };
            self.run_task(
                TaskSpec::new(
                    "cooler-import",
                    "Preparing cooler settings and library",
                    super::scheduler::Resources::one(super::scheduler::Res::Io),
                )
                .visible(),
                move |_| {
                    super::cooler_migration::prepare(&source, &target, &gallery)?;
                    Ok(json!({"ok":true}))
                },
            )?;
        }
        let old = self.settings.load().0["cooler"]["enabled"]
            .as_bool()
            .unwrap_or(false);
        self.settings
            .update(|s| s["cooler"]["enabled"] = json!(enabled))
            .map_err(CoreError::from)?;
        let state = match cooler.configure(enabled) {
            Ok(state) => state,
            Err(error) => {
                self.settings
                    .update(|s| s["cooler"]["enabled"] = json!(old))
                    .map_err(CoreError::from)?;
                return Err(error);
            }
        };
        Ok(state)
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
            "autostart": super::autostart::is_on(),
            "dock": self.dock_state(),
            "cooler": self.cooler_state(),
        })
    }

    /// `{ page? }`: saved at once (no Save button).
    pub fn settings_update(&self, body: &Value) -> CoreResult<Value> {
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
                if let Some(follow) = follow {
                    s["ui"]["followPhysicalPage"] = follow.clone();
                }
                if let Some(p) = page {
                    s["ui"]["page"] = json!(p);
                }
            })
            .map_err(|e| CoreError::new(e.to_string()))?;
        Ok(self.app_state())
    }

    pub fn activity(&self) -> Value {
        json!({ "deviceBusy": self.sched.device_active(), "jobs": self.sched.activity(120_000) })
    }

    pub fn profiles_state(&self) -> CoreResult<Value> {
        let state = self
            .profiles
            .state()
            .map_err(|e| CoreError::coded("profiles", e.to_string()))?;
        Ok(json!({ "state": state, "notice": self.profiles.recovery_notice() }))
    }

    pub fn save_profiles(&self, snapshot: super::profiles::ProfileState) -> CoreResult<Value> {
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
                let state = store
                    .save(&snapshot)
                    .map_err(|e| CoreError::coded("profiles", e.to_string()))?;
                if let Some(dock) = &dock {
                    dock.notify_profiles_changed();
                }
                Ok(json!({ "state": state, "notice": store.recovery_notice() }))
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
        self.sched.any_visible_job()
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
