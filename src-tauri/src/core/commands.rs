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
    fn cooler(&self, event: &str, payload: Value) {
        let _ = self
            .0
            .emit("uc:cooler-event", json!({"event":event,"payload":payload}));
    }
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
    fn cooler(&self, event: &str, payload: Value) {
        if let Some(e) = self.0.get() {
            e.cooler(event, payload);
        }
    }
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

async fn run<T: Send + 'static>(
    app: &State<'_, Core>,
    f: impl FnOnce(&App) -> CoreResult<T> + Send + 'static,
) -> Result<T, CoreError> {
    let a = app.0.clone();
    tauri::async_runtime::spawn_blocking(move || f(&a))
        .await
        .map_err(|e| CoreError::new(e.to_string()).status(500))?
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
pub async fn settings_update(
    core: State<'_, Core>,
    request: tauri::ipc::Request<'_>,
) -> Result<Value, CoreError> {
    let a = json_args(&request);
    let patch = a
        .get("patch")
        .filter(|p| p.is_object())
        .cloned()
        .unwrap_or(a);
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
    run(&core, move |a| {
        Ok(json!({ "cancelled": a.sched.cancel(&job_id) }))
    })
    .await
}

#[tauri::command]
pub async fn profiles_state(core: State<'_, Core>) -> Result<Value, CoreError> {
    run(&core, |app| app.profiles_state()).await
}

#[tauri::command]
pub async fn profiles_save(
    core: State<'_, Core>,
    snapshot: super::profiles::ProfileState,
) -> Result<Value, CoreError> {
    run(&core, move |app| app.save_profiles(snapshot)).await
}

#[tauri::command]
pub async fn key_previews(
    core: State<'_, Core>,
    faces: Vec<super::profiles::Face>,
    clock: String,
) -> Result<Value, CoreError> {
    if faces.len() != 14 {
        return Err(CoreError::new("A dock page has exactly 14 displays"));
    }
    run(&core, move |app| {
        use super::dock_service::DockHooks;
        let runtime=app.runtime.get().ok_or_else(||CoreError::new("The app runtime is unavailable"))?;
        let mut inputs=runtime.inputs();inputs.clock=Some(clock);
        let mut rendered = Vec::with_capacity(14);
        for (index, face) in faces.iter().enumerate() {
            let frame = runtime.render_one(face,index,&inputs).map_err(CoreError::new)?;
            rendered.push(frame);
        }
        let mut frames=Vec::with_capacity(14);let now=super::scheduler::now_ms();
        for(index,mut frame) in rendered.into_iter().enumerate(){frame.guard_delivery(now).map_err(CoreError::new)?;frames.push(json!({"index":index,"width":frame.width,"height":frame.height,"png":frame.png,"unknown":frame.unknown}));}
        Ok(Value::Array(frames))
    }).await
}

#[tauri::command]
pub async fn dock_state(core: State<'_, Core>) -> Result<Value, CoreError> {
    run(&core, |app| Ok(app.dock_state())).await
}
#[tauri::command]
pub async fn cooler_state(core: State<'_, Core>) -> Result<Value, CoreError> {
    run(&core, |app| Ok(app.cooler_state())).await
}
#[tauri::command]
pub async fn cooler_configure(core: State<'_, Core>, enabled: bool) -> Result<Value, CoreError> {
    run(&core, move |app| app.cooler_configure(enabled)).await
}
#[tauri::command]
pub async fn cooler_call(
    core: State<'_, Core>,
    command: String,
    args: Value,
) -> Result<Value, CoreError> {
    run(&core, move |app| app.cooler_call(&command, args)).await
}
#[tauri::command]
pub async fn dock_configure(core: State<'_, Core>, patch: Value) -> Result<Value, CoreError> {
    run(&core, move |app| app.dock_configure(&patch)).await
}
#[tauri::command]
pub async fn dock_test_button(
    core: State<'_, Core>,
    index: usize,
    pressed: bool,
) -> Result<Value, CoreError> {
    run(&core, move |app| {
        app.dock
            .get()
            .ok_or_else(|| CoreError::new("Dock unavailable"))?
            .simulate_button(index, pressed)
            .map_err(CoreError::new)?;
        Ok(app.dock_state())
    })
    .await
}
#[tauri::command]
pub async fn dock_test_studio(core: State<'_, Core>, running: bool) -> Result<Value, CoreError> {
    run(&core, move |app| {
        app.dock
            .get()
            .ok_or_else(|| CoreError::new("Dock unavailable"))?
            .set_fake_studio(running)
            .map_err(CoreError::new)?;
        Ok(app.dock_state())
    })
    .await
}
#[tauri::command]
pub async fn asset_import(core: State<'_, Core>, bytes: Vec<u8>) -> Result<Value, CoreError> {
    if bytes.len() > super::key_render::MAX_PICTURE_BYTES {
        return Err(CoreError::new("Choose a picture of at most 4 MiB"));
    }
    run(&core, move |app| {
        let runtime = app
            .runtime
            .get()
            .ok_or_else(|| CoreError::new("The app runtime is unavailable"))?;
        Ok(json!(runtime
            .assets
            .import(&bytes)
            .map_err(CoreError::new)?))
    })
    .await
}
#[tauri::command]
pub async fn monitor_list(core: State<'_, Core>) -> Result<Value, CoreError> {
    run(&core, |app| {
        let runtime = app
            .runtime
            .get()
            .ok_or_else(|| CoreError::new("The app runtime is unavailable"))?;
        if runtime.fake {
            return Ok(json!([{"key":"fake:display-1","label":"Simulated display"}]));
        }
        serde_json::to_value(
            super::actions::list_monitors(&runtime.resource_dir).map_err(CoreError::new)?,
        )
        .map_err(|_| CoreError::new("Monitor information unavailable"))
    })
    .await
}
#[tauri::command]
pub async fn autostart_set(core: State<'_, Core>, enabled: bool) -> Result<Value, CoreError> {
    run(&core, move |app| {
        if super::buildinfo::fake_device().is_some() {
            return Err(CoreError::new("Startup settings are disabled in test mode"));
        }
        if !super::autostart::set(enabled) {
            return Err(CoreError::new(
                "The Windows startup setting could not be changed",
            ));
        }
        Ok(app.app_state())
    })
    .await
}
