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
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

/// What the UI is told (Tauri events in the app, a recorder in tests).
pub trait Events: Send + Sync + 'static {
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
        Arc::new(Debounce { pending: AtomicBool::new(false), delay, f: Box::new(f) })
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
        let sched = Scheduler::new(Limits::default(), Policy::default(), Arc::new(SinkAdapter(events.clone())));
        let app = Arc::new(App { settings: settings.clone(), sched, events: events.clone(), app_version: app_version.to_string(), base, me: Mutex::new(Weak::new()) });
        *app.me.lock().unwrap() = Arc::downgrade(&app);
        let ev = events.clone();
        let app_deb = Debounce::new(Duration::from_millis(100), move || ev.app_changed());
        settings.set_on_change(move || app_deb.poke());
        app
    }

    pub fn arc(&self) -> Arc<App> {
        self.me.lock().unwrap().upgrade().expect("App alive")
    }

    /// Run a task through the scheduler and wait for it (on the calling thread).
    pub fn run_task(&self, spec: TaskSpec, f: impl Fn(&TaskCtx) -> CoreResult<Value> + Send + Sync + 'static) -> CoreResult<Value> {
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
            "autostart": super::autostart::is_on(),
        })
    }

    /// `{ page? }`: saved at once (no Save button).
    pub fn settings_update(&self, body: &Value) -> CoreResult<Value> {
        let page = body.get("page").and_then(Value::as_str).map(String::from);
        if let Some(p) = &page {
            if !super::settings::PAGES.contains(&p.as_str()) {
                return Err(CoreError::new(format!("Unknown page: {p}")));
            }
        }
        self.settings
            .update(|s| {
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

    pub fn job_get(&self, id: &str) -> CoreResult<Value> {
        let j = self.sched.job(id).ok_or_else(|| CoreError::not_found("No such job"))?;
        Ok(serde_json::to_value(j).unwrap_or(Value::Null))
    }

    pub fn shutdown(&self) {
        self.sched.cancel_where(|_| true);
        // Give cancelled tasks a moment to stop. Bounded: a running device write cannot be cancelled and is never waited for
        // longer than this.
        self.sched.wait_idle(Duration::from_secs(1), true);
    }

    /// `UnifiedControl.exe --quit` refuses while a job is running.
    pub fn busy_for_quit(&self) -> bool {
        self.sched.any_visible_job()
    }

    /// True while something is being written to a device (close guard).
    pub fn uploading(&self) -> bool {
        self.sched.running_kind(&["send", "setup"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scheduler::{Resources, Res};

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
        let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(dir.path().join("settings.json")).unwrap()).unwrap();
        assert_eq!(on_disk["ui"]["page"], "dock");
        assert!(a.settings_update(&json!({ "page": "cooler" })).is_err(), "no cooler page");
    }

    #[test]
    fn busy_while_a_job_runs_and_idle_after() {
        let (_dir, a) = app();
        assert!(!a.busy_for_quit());
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let rx = Mutex::new(rx);
        let h = a.sched.submit(TaskSpec::new("send", "Sending a page", Resources::one(Res::Device)).visible(), move |_| {
            let _ = rx.lock().unwrap().recv_timeout(Duration::from_secs(5));
            Ok(Value::Null)
        });
        let t0 = std::time::Instant::now();
        while !a.busy_for_quit() && t0.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(a.busy_for_quit(), "a running job makes the app busy (quit and installs wait)");
        assert!(a.uploading());
        tx.send(()).unwrap();
        let _ = h.wait();
        assert!(!a.busy_for_quit());
    }
}
