//! The tray lifecycle (copied from CoolerScreen's `live_app.rs`; the cooler's live engine, pause/fps items and the
//! sleep/lock window are not here). Unified Control always has its tray icon: Open and Quit. Closing the window hides it to
//! the tray (or destroys it to free the WebView memory, see `DESTROY_ON_CLOSE`); only Quit ends the app. Nothing starts with
//! Windows unless the user turns that on (opt-in, off by default).
//!
//! Quiet test instances (`UC_TEST_QUIET=1`) keep the tray logic but do not put an icon in the notification area, so a test
//! never adds anything to the user's taskbar (`UC_TEST_TRAY_ICON=1` shows it anyway, for a test that needs the real icon).
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry, WebviewWindow, WindowEvent};
use uc_core::core::commands::Core;
use uc_core::core::error::CoreError;

/// True while the tray exists (closing the window then hides it, and the app does not exit with its last window).
static TRAY_ON: AtomicBool = AtomicBool::new(false);

const TRAY_ID: &str = "uc";
const TOOLTIP: &str = "Unified Control";
/// Closing the window to the tray destroys it (and its WebView2), as CoolerScreen's default "free the window memory".
const DESTROY_ON_CLOSE: bool = true;

/// The menu items (kept so later phases can change their labels from any thread).
struct Tray {
    open: MenuItem<Wry>,
}

static TRAY: Mutex<Option<Tray>> = Mutex::new(None);

pub fn tray_on() -> bool {
    TRAY_ON.load(Ordering::SeqCst)
}

/// The tray icon is left out of the notification area in quiet test runs.
fn tray_icon_hidden() -> bool {
    crate::window::quiet() && !std::env::var("UC_TEST_TRAY_ICON").is_ok_and(|v| !v.is_empty() && v != "0")
}

// ---------------------------------------------------------------- window

/// Close handling: with the tray on, the window hides (or is destroyed to free the WebView) instead of quitting.
pub fn attach_close_handler(win: &WebviewWindow) {
    let w2 = win.clone();
    win.on_window_event(move |e| {
        if let WindowEvent::CloseRequested { api, .. } = e {
            if tray_on() {
                crate::window::clear_force_close();
                if !DESTROY_ON_CLOSE {
                    api.prevent_close();
                    let _ = w2.hide();
                }
                return; // destroy: let the close happen; the app stays up (see `keep_running`)
            }
            if crate::window::force_close_requested() {
                return;
            }
            // Something is being written to a device: keep the window and let the page ask in its own dialog.
            if w2.app_handle().state::<Core>().0.uploading() {
                api.prevent_close();
                let _ = tauri::Emitter::emit(&w2, "close-guard", ());
            }
        }
    });
}

/// Build the main window (hidden), add the close handler and show it.
pub fn create_window(app: &AppHandle, show: bool) -> tauri::Result<WebviewWindow> {
    create_window_as(app, show.then_some(crate::window::Present::Normal))
}

/// As `create_window`, with how it is shown (None: built hidden).
pub fn create_window_as(app: &AppHandle, how: Option<crate::window::Present>) -> tauri::Result<WebviewWindow> {
    let win = crate::window::create(app)?;
    attach_close_handler(&win);
    if let Some(h) = how {
        crate::window::present(&win, h);
    }
    Ok(win)
}

/// Bring the window up (build it again if it was destroyed). Safe from any thread.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        crate::window::present(&w, crate::window::Present::Normal);
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = create_window(&app, true);
    });
}

/// True when the app must stay up although its last window is gone.
pub fn keep_running() -> bool {
    tray_on()
}

// ---------------------------------------------------------------- tray

fn icon(app: &AppHandle) -> Image<'static> {
    if let Some(i) = app.default_window_icon() {
        return Image::new_owned(i.rgba().to_vec(), i.width(), i.height());
    }
    let n = 32usize;
    let mut px = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let key = (5..27).contains(&x) && (5..27).contains(&y);
            let c: [u8; 4] = if key { [64, 200, 160, 255] } else { [28, 32, 48, 255] };
            px[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
        }
    }
    Image::new_owned(px, n as u32, n as u32)
}

fn build_tray(app: &AppHandle) {
    let items = (|| -> tauri::Result<(Menu<Wry>, Tray)> {
        let open = MenuItem::with_id(app, "open", "Open Unified Control", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
        let sep = PredefinedMenuItem::separator(app)?;
        let menu = Menu::with_items(app, &[&open, &sep, &quit])?;
        Ok((menu, Tray { open }))
    })();
    let Ok((menu, tray)) = items else {
        eprintln!("could not create the tray menu");
        return;
    };
    if tray_icon_hidden() {
        // quiet test run: the same lifecycle, no icon on the user's taskbar
        uc_core::device::log("tray: quiet test run, icon not shown");
        *TRAY.lock().unwrap() = Some(tray);
        TRAY_ON.store(true, Ordering::SeqCst);
        return;
    }
    // Tauri's own tray: its menu and click events are routed by Tauri to these two closures (the `tray-icon` crate's global
    // handlers are a set-once slot that Tauri fills first, so a handler set here never receives anything).
    let built = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(TOOLTIP)
        .icon(icon(app))
        .on_menu_event(|a: &AppHandle, ev: MenuEvent| tray_menu_click(a, ev.id().as_ref()))
        .on_tray_icon_event(|t, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(t.app_handle());
            }
        })
        .build(app);
    if built.is_err() {
        eprintln!("could not create the tray icon");
        return;
    }
    *TRAY.lock().unwrap() = Some(tray);
    TRAY_ON.store(true, Ordering::SeqCst);
}

/// A tray menu item was chosen (`id`: open, quit). The tray's menu handler calls this; the debug test hook (`UC_TEST_TRAY`)
/// calls it too.
pub fn tray_menu_click(a: &AppHandle, id: &str) {
    match id {
        "open" => show_main(a),
        "quit" => quit_app(a),
        _ => {}
    }
}

/// The menu labels as they stand (debug test hook).
#[cfg(debug_assertions)]
pub fn tray_state() -> String {
    match TRAY.lock().unwrap().as_ref() {
        Some(t) => format!("open_text={:?} icon_shown={}", t.open.text().unwrap_or_default(), !tray_icon_hidden()),
        None => "no tray".into(),
    }
}

fn remove_tray(app: &AppHandle) {
    let _ = app.remove_tray_by_id(TRAY_ID);
    *TRAY.lock().unwrap() = None;
    TRAY_ON.store(false, Ordering::SeqCst);
}

/// Tray "Quit": stop the work, then exit.
pub fn quit_app(app: &AppHandle) {
    arm_exit_watchdog("tray Quit");
    let a = app.clone();
    std::thread::spawn(move || {
        let b = a.clone();
        let _ = a.run_on_main_thread(move || {
            remove_tray(&b);
            b.exit(0);
        });
    });
}

/// Carry out a request from `--quit` / `--restart` / `--update` or the General page button: leave a detached helper when the
/// app is to come back (restart, update), then quit exactly like the tray's Quit. If the helper cannot be started the app does
/// not quit.
pub fn restart_app(app: &AppHandle, req: crate::quit_cmd::Request) {
    use crate::quit_cmd::Request;
    let pid = std::process::id();
    let started = match &req {
        Request::Quit => Ok(()),
        Request::Restart => crate::relaunch::spawn_helper(pid, true, None),
        Request::Update(p) => crate::relaunch::spawn_helper(pid, true, Some(std::path::Path::new(p))),
    };
    match started {
        Ok(()) => quit_app(app),
        Err(e) => uc_core::device::log(&format!("updater: could not start the helper ({e}); the app stays up")),
    }
}

/// General > "Restart Unified Control" (troubleshooting): the tray Quit path, then a fresh copy started by the app itself with
/// its window as usual. Refused with a clear message while a job runs.
#[tauri::command]
pub async fn app_restart(app: AppHandle, core: tauri::State<'_, Core>) -> Result<Value, CoreError> {
    if core.0.busy_for_quit() {
        return Err(CoreError::new("Something is still running. Restart when it has finished."));
    }
    uc_core::device::log("restart requested (General): the tray Quit path");
    let pid = std::process::id();
    crate::relaunch::spawn_helper(pid, false, None).map_err(|e| CoreError::new(format!("Could not restart: {e}")))?;
    let a = app.clone();
    quit_app(&a);
    Ok(serde_json::json!({ "restarting": true }))
}

/// Safety net: if quitting has not finished within ~3 s (a stuck device call, a lock, a thread that will not end), leave anyway.
pub fn arm_exit_watchdog(why: &'static str) {
    static ARMED: AtomicBool = AtomicBool::new(false);
    if ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(3000));
        eprintln!("quit ({why}) took more than 3 s: exiting anyway");
        std::process::exit(0);
    });
}

/// Create the tray (once) on the main thread. `wait`: block until it exists.
pub fn ensure_tray(app: &AppHandle, wait: bool) {
    let (tx, rx) = std::sync::mpsc::channel();
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        if !tray_on() {
            build_tray(&a);
        }
        let _ = tx.send(());
    });
    if wait && std::thread::current().name() != Some("main") {
        let _ = rx.recv_timeout(std::time::Duration::from_secs(5));
    }
}
