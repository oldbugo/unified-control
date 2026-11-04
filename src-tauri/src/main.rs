// Unified Control shell: Tauri 2 + system WebView2, seamless frameless window, tray icon (Open, Quit).
// Copied from CoolerScreen's main.rs (commit in README.md): the app shell only. The Rust core (src/core) serves the UI through
// Tauri commands/events; the tray lifecycle is in live_app.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod instance_id;
mod agent_transport;
mod live_app;
mod quit_cmd;
mod relaunch;
mod window;

use std::sync::Arc;
use tauri::{Manager, RunEvent};
use uc_core::core::commands::{self as cc, Core, EventRelay, TauriEvents};
use uc_core::core::App;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The supervised child has no window, tray, MCP server or durable profile
    // writer. Dispatch before every GUI/single-instance path.
    if args.iter().any(|a| a == "--dock-worker") {
        std::process::exit(if uc_core::core::dock_worker::worker_main().is_ok() {
            0
        } else {
            1
        });
    }
    // MCP's request-only bridge must never create or focus an application.
    if args.iter().any(|a| a == "--agent-request") {
        std::process::exit(agent_transport::run_request());
    }
    // `--quit`: ask the running app (same app folder) to quit like the tray's Quit; never shows a window or starts an app.
    if args.iter().any(|a| a == "--quit") {
        std::process::exit(quit_cmd::run_request(quit_cmd::Request::Quit));
    }
    // `--restart`: the running app quits like the tray's Quit and starts itself again quietly (`--background`).
    if args.iter().any(|a| a == "--restart") {
        std::process::exit(quit_cmd::run_request(quit_cmd::Request::Restart));
    }
    // `--update <UnifiedControl_x.y.z_x64-setup.exe>`: only when idle; the app quits, the installer runs silently, the new app starts quietly.
    if let Some(i) = args.iter().position(|a| a == "--update") {
        let p = args
            .get(i + 1)
            .map(std::path::PathBuf::from)
            .and_then(|p| std::fs::canonicalize(&p).ok())
            .map(|p| {
                let s = p.to_string_lossy().to_string();
                s.strip_prefix(r"\\?\").map(String::from).unwrap_or(s)
            });
        match p.filter(|p| relaunch::valid_installer(std::path::Path::new(p))) {
            Some(p) => std::process::exit(quit_cmd::run_request(quit_cmd::Request::Update(p))),
            None => std::process::exit(quit_cmd::EXIT_BAD),
        }
    }
    // the detached helper a restart or update leaves behind (waits for the app to exit, installs, starts it again)
    if args.iter().any(|a| a == relaunch::HELPER_FLAG) {
        std::process::exit(relaunch::run_helper(&args));
    }
    // `--tray` (the Windows start-up entry, opt-in): no window, icon in the tray.
    let tray_only = args
        .iter()
        .any(|a| a == uc_core::core::autostart::TRAY_FLAG);
    // `--background`: start without showing or activating anything (a quiet restart after an install): tray only.
    let background = args.iter().any(|a| a == "--background");
    let relay = Arc::new(EventRelay::default());
    let core = App::new(
        uc_core::core::paths::base(),
        relay.clone(),
        env!("CARGO_PKG_VERSION"),
    );
    let mut builder = tauri::Builder::default();
    // One Unified Control at a time, unless UC_MULTI is set (UI tests next to the installed app, with a temp UC_HOME).
    // (The override only works in debug builds: an installed build always focuses the running one.)
    if !(cfg!(debug_assertions) && std::env::var_os("UC_MULTI").is_some()) {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // a second launch brings the window up, except a quiet `--background` restart
            if !args.iter().any(|a| a == "--background") {
                live_app::show_main(app);
            }
        }));
    }
    // The single-instance lock is named after the app identifier: debug builds and instances with their own app folder get their
    // own identifier, so they can never forward a launch to (or take one from) the installed app. See instance_id.rs.
    let mut context = tauri::generate_context!();
    let id = instance_id::identifier(
        &context.config().identifier,
        cfg!(debug_assertions),
        instance_id::env_home().as_deref(),
    );
    context.config_mut().identifier = id;
    let app = builder
        .manage(Core(core.clone()))
        .invoke_handler(tauri::generate_handler![
            window::win_minimize,
            window::win_toggle_maximize,
            window::win_state,
            window::win_close,
            window::win_force_close,
            live_app::app_restart,
            live_app::editor_session_ready,
            live_app::editor_quit_ready,
            cc::app_state,
            cc::settings_update,
            cc::activity,
            cc::job_get,
            cc::job_cancel,
            cc::profiles_state,
            cc::profiles_save,
            cc::key_previews,
            cc::dock_state,
            cc::cooler_state,
            cc::cooler_migration_verify,
            cc::cooler_configure,
            cc::cooler_call,
            cc::dock_configure,
            cc::dock_test_button,
            cc::dock_test_studio,
            cc::asset_import,
            cc::monitor_list,
            cc::agent_editor_present,
            cc::agent_editor_ready,
            cc::autostart_set
        ])
        .setup(move |app| {
            uc_core::device::log(&uc_core::core::buildinfo::startup_line(env!(
                "CARGO_PKG_VERSION"
            )));
            uc_core::device::log(&format!("single-instance id: {}", app.config().identifier));
            let _ = relay.0.set(Arc::new(TauriEvents(app.handle().clone())));
            app.state::<Core>()
                .0
                .start_runtime(app.path().resource_dir()?)
                .map_err(|error| std::io::Error::other(error.message))?;
            let agent_core = app.state::<Core>().0.clone();
            match agent_transport::start(Arc::new(move |request| {
                match agent_core.agent_call(
                    request["method"].as_str().unwrap_or(""),
                    request["params"].clone(),
                ) {
                    Ok(result) => serde_json::json!({"ok":true,"result":result}),
                    Err(error) => agent_error(error),
                }
            })) {
                Ok(server) => { app.manage(std::sync::Mutex::new(Some(server))); }
                Err(_) => uc_core::device::log("local agent connection unavailable"),
            }
            // the tray is always there (Open, Quit): closing the window leaves the app in the tray
            live_app::ensure_tray(app.handle(), false);
            if !tray_only && !background {
                live_app::create_window(app.handle(), true)?;
            }
            // `UnifiedControl.exe --quit` from an installer or script reaches this app through a named event
            {
                let (h1, h2) = (app.handle().clone(), app.handle().clone());
                quit_cmd::listen(
                    move || h1.state::<Core>().0.busy_for_quit(),
                    move |req| live_app::restart_app(&h2, req),
                );
            }
            // Debug builds only: the tray's Quit path on a timer, for the quiet tests (a tray menu cannot be clicked from a script).
            #[cfg(debug_assertions)]
            if let Some(ms) = std::env::var("UC_TEST_TRAY_QUIT_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
            {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                    eprintln!("UC_TEST tray quit, tray_on={}", live_app::tray_on());
                    live_app::quit_app(&h);
                });
            }
            // Debug builds only: "name@ms,..." (open, quit; plus hide and close of the window) calls the tray menu handler on a timer,
            // and prints the state after it.
            #[cfg(debug_assertions)]
            if let Ok(plan) = std::env::var("UC_TEST_TRAY") {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    let t0 = std::time::Instant::now();
                    for step in plan.split(',') {
                        let Some((name, ms)) = step.split_once('@') else {
                            continue;
                        };
                        let at = std::time::Duration::from_millis(ms.parse().unwrap_or(0));
                        std::thread::sleep(at.saturating_sub(t0.elapsed()));
                        match name {
                            "hide" => {
                                if let Some(w) = h.get_webview_window("main") {
                                    let _ = w.hide();
                                }
                            }
                            "close" => {
                                if let Some(w) = h.get_webview_window("main") {
                                    let _ = w.close();
                                }
                            }
                            _ => live_app::tray_menu_click(&h, name),
                        }
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        eprintln!(
                            "UC_TEST after {name}: tray_on={} window_visible={:?} menu: {}",
                            live_app::tray_on(),
                            h.get_webview_window("main")
                                .map(|w| w.is_visible().unwrap_or(false)),
                            live_app::tray_state()
                        );
                    }
                });
            }
            Ok(())
        })
        .build(context)
        .expect("error while building Unified Control");

    app.run(|app, event| match event {
        // The last window closed: with the tray the app stays (it ends only with the tray's Quit).
        RunEvent::ExitRequested { code, api, .. } => {
            if code.is_none() && live_app::keep_running() {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            live_app::arm_exit_watchdog("exit");
            if let Some(server) = app.try_state::<std::sync::Mutex<Option<agent_transport::Server>>>() {
                if let Some(mut server) = server.lock().unwrap().take() { server.shutdown(); }
            }
            app.state::<Core>().0.shutdown();
            uc_core::device::log("exit");
        }
        _ => {}
    });
}

fn agent_error(error: uc_core::core::error::CoreError) -> serde_json::Value {
    let (code, message) = match error.code.as_deref() {
        Some("profiles") => ("profiles", "Dock layout could not be saved or read."),
        Some(code @ ("revision_conflict" | "invalid-arguments" | "busy" | "agent-disabled"
            | "method-not-found" | "runtime-unavailable" | "monitor-unavailable"
            | "render-unavailable" | "undo-unavailable" | "stale-lease"))
            if error.message.len() <= 512 && !error.message.chars().any(char::is_control) => (code, error.message.as_str()),
        _ => ("agent_error", "The agent operation could not be completed."),
    };
    serde_json::json!({"ok":false,"error":{"code":code,"message":message}})
}

#[cfg(test)]
mod agent_error_tests {
    use super::*;
    use uc_core::core::error::CoreError;
    #[test]
    fn agent_errors_keep_conflicts_but_hide_private_storage_and_unknown_details() {
        let private = "private fixture path or operating system detail";
        for error in [CoreError::coded("profiles",private),CoreError::new(private),CoreError::coded("future-code",private)] {
            assert!(!agent_error(error).to_string().contains(private));
        }
        assert_eq!(agent_error(CoreError::coded("revision_conflict","Dock layout changed"))["error"]["code"],"revision_conflict");
    }
}
