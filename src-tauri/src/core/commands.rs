//! Tauri commands and events for the UI. Copied from CoolerScreen's `core/commands.rs` (the shell part). Thin async wrappers:
//! every command runs its `App` method on a blocking thread so the UI thread never waits on disk or a device.
use super::app::{App, Events};
use super::error::{CoreError, CoreResult};
use super::scheduler::Job;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{Emitter, State};

/// Managed state: the core app.
pub struct Core(pub Arc<App>);

/// Events → `uc:job`, `uc:app`, `uc:device`.
pub struct TauriEvents(pub tauri::AppHandle);

impl Events for TauriEvents {
    fn job(&self, job: &Job) {
        let _ = self.0.emit("uc:job", job);
    }
    fn app_changed(&self) {
        let _ = self.0.emit("uc:app", json!({}));
    }
    fn device_busy(&self, busy: bool) {
        let _ = self.0.emit("uc:device", json!({ "busy": busy }));
    }
}

/// Late-bound events: the App is created before the Tauri handle exists.
#[derive(Default)]
pub struct EventRelay(pub std::sync::OnceLock<Arc<dyn Events>>);

impl Events for EventRelay {
    fn job(&self, j: &Job) {
        if let Some(e) = self.0.get() {
            e.job(j)
        }
    }
    fn app_changed(&self) {
        if let Some(e) = self.0.get() {
            e.app_changed()
        }
    }
    fn device_busy(&self, b: bool) {
        if let Some(e) = self.0.get() {
            e.device_busy(b)
        }
    }
}

async fn run<T: Send + 'static>(app: &State<'_, Core>, f: impl FnOnce(&App) -> CoreResult<T> + Send + 'static) -> Result<T, CoreError> {
    let a = app.0.clone();
    tauri::async_runtime::spawn_blocking(move || f(&a)).await.map_err(|e| CoreError::new(e.to_string()).status(500))?
}

// ---------- state, jobs ----------

#[tauri::command]
pub async fn app_state(core: State<'_, Core>) -> Result<Value, CoreError> {
    run(&core, |a| Ok(a.app_state())).await
}

/// The JSON args of a call as one object (so a key set to null can be told from a missing one).
fn json_args(request: &tauri::ipc::Request<'_>) -> Value {
    match request.body() {
        tauri::ipc::InvokeBody::Json(v) => v.clone(),
        _ => json!({}),
    }
}

/// `{ page? }`, or the same wrapped in `patch`.
#[tauri::command]
pub async fn settings_update(core: State<'_, Core>, request: tauri::ipc::Request<'_>) -> Result<Value, CoreError> {
    let a = json_args(&request);
    let patch = a.get("patch").filter(|p| p.is_object()).cloned().unwrap_or(a);
    run(&core, move |app| app.settings_update(&patch)).await
}

#[tauri::command]
pub async fn activity(core: State<'_, Core>) -> Result<Value, CoreError> {
    run(&core, |a| Ok(a.activity())).await
}

#[tauri::command]
pub async fn job_get(core: State<'_, Core>, job_id: String) -> Result<Value, CoreError> {
    run(&core, move |a| a.job_get(&job_id)).await
}

#[tauri::command]
pub async fn job_cancel(core: State<'_, Core>, job_id: String) -> Result<Value, CoreError> {
    run(&core, move |a| Ok(json!({ "cancelled": a.sched.cancel(&job_id) }))).await
}
