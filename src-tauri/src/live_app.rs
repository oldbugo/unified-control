//! The tray lifecycle (copied from CoolerScreen's `live_app.rs`; the cooler's live engine, pause/fps items and the
//! sleep/lock window are not here). Unified Control always has its tray icon and dock controls. Closing the window hides it to
//! the tray (or destroys it to free the WebView memory, see `DESTROY_ON_CLOSE`); only Quit ends the app. Nothing starts with
//! Windows unless the user turns that on (opt-in, off by default).
//!
//! Quiet test instances (`UC_TEST_QUIET=1`) keep the tray logic but do not put an icon in the notification area, so a test
//! never adds anything to the user's taskbar (`UC_TEST_TRAY_ICON=1` shows it anyway, for a test that needs the real icon).
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindow, WindowEvent, Wry};
use uc_core::core::commands::Core;
use uc_core::core::error::CoreError;

/// True while the tray exists (closing the window then hides it, and the app does not exit with its last window).
static TRAY_ON: AtomicBool = AtomicBool::new(false);

const TRAY_ID: &str = "uc";
const TOOLTIP: &str = "Unified Control";
/// Closing the window to the tray destroys it (and its WebView2), as CoolerScreen's default "free the window memory".
// The editor owns pending profile edits. Keep its context until Quit has flushed
// them; destroying it on a normal close would drop queued or failed writes.
const DESTROY_ON_CLOSE: bool = false;
static EDITOR_READY: AtomicBool = AtomicBool::new(false);
static PENDING_EXIT: Mutex<Option<(crate::quit_cmd::Request, bool)>> = Mutex::new(None);
static EXIT_FINISH: Mutex<()> = Mutex::new(());
static DOCK_TOGGLE_BUSY: AtomicBool = AtomicBool::new(false);
static TRAY_DIRTY: AtomicBool = AtomicBool::new(true);

/// The menu items (kept so later phases can change their labels from any thread).
struct Tray {
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    open: MenuItem<Wry>,
    dock_enabled: CheckMenuItem<Wry>,
    profile: MenuItem<Wry>,
}

static TRAY: Mutex<Option<Tray>> = Mutex::new(None);

pub fn tray_on() -> bool {
    TRAY_ON.load(Ordering::SeqCst)
}

/// The tray icon is left out of the notification area in quiet test runs.
fn tray_icon_hidden() -> bool {
    crate::window::quiet()
        && !std::env::var("UC_TEST_TRAY_ICON").is_ok_and(|v| !v.is_empty() && v != "0")
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
pub fn create_window_as(
    app: &AppHandle,
    how: Option<crate::window::Present>,
) -> tauri::Result<WebviewWindow> {
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
            let c: [u8; 4] = if key {
                [64, 200, 160, 255]
            } else {
                [28, 32, 48, 255]
            };
            px[(y * n + x) * 4..(y * n + x) * 4 + 4].copy_from_slice(&c);
        }
    }
    Image::new_owned(px, n as u32, n as u32)
}

fn build_tray(app: &AppHandle) {
    let items = (|| -> tauri::Result<(Menu<Wry>, Tray)> {
        let open = MenuItem::with_id(app, "open", "Open Unified Control", true, None::<&str>)?;
        let dock_enabled = CheckMenuItem::with_id(
            app,
            "dock-toggle",
            "Dock enabled",
            true,
            false,
            None::<&str>,
        )?;
        let profile =
            MenuItem::with_id(app, "dock-profile", "Profile: loading", false, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
        let sep = PredefinedMenuItem::separator(app)?;
        let menu = Menu::with_items(app, &[&open, &dock_enabled, &profile, &sep, &quit])?;
        Ok((
            menu,
            Tray {
                open,
                dock_enabled,
                profile,
            },
        ))
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
        start_tray_refresh(app);
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
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
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
    start_tray_refresh(app);
}

/// Keep profile/device reads off the window thread. Only changed menu labels are
/// posted back, so the status loop does not queue repeated native menu updates.
fn start_tray_refresh(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut previous = None;
        while tray_on() {
            let core = app.state::<Core>();
            let status = core.0.dock_state();
            let enabled = status["enabled"].as_bool().unwrap_or(false);
            let mut profile = status["currentProfileName"].as_str().map(str::to_owned);
            let mut page = status["currentPageName"].as_str().map(str::to_owned);
            // A disabled controller has no active runtime yet; show the saved
            // selection rather than claiming there is no profile.
            if profile.is_none() {
                if let Ok(saved) = core.0.profiles.state() {
                    profile = saved.active_profile().map(|p| p.name.clone());
                    page = saved.active_page().map(|p| p.name.clone());
                }
            }
            let label = match (profile, page) {
                (Some(profile), Some(page)) => format!("Profile: {profile} / {page}"),
                (Some(profile), None) => format!("Profile: {profile}"),
                _ => "Profile: unavailable".into(),
            }
            .replace('&', "&&")
            .replace(['\r', '\n', '\t'], " ");
            let busy = DOCK_TOGGLE_BUSY.load(Ordering::SeqCst);
            let next = (enabled, label, busy);
            if TRAY_DIRTY.swap(false, Ordering::SeqCst) || previous.as_ref() != Some(&next) {
                let (enabled, label, busy) = next.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Some(tray) = TRAY.lock().unwrap().as_ref() {
                        let _ = tray.dock_enabled.set_checked(enabled);
                        let _ = tray.dock_enabled.set_enabled(!busy);
                        let _ = tray.profile.set_text(label);
                    }
                });
                previous = Some(next);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

/// A tray menu item was chosen (`id`: open, dock-toggle, quit). The tray's menu handler calls this; the debug test hook (`UC_TEST_TRAY`)
/// calls it too.
pub fn tray_menu_click(a: &AppHandle, id: &str) {
    match id {
        "open" => show_main(a),
        "dock-toggle" => {
            // Native check items flip immediately, even if the operation is
            // refused. Force a later refresh to restore the actual saved value.
            TRAY_DIRTY.store(true, Ordering::SeqCst);
            if DOCK_TOGGLE_BUSY.swap(true, Ordering::SeqCst) {
                return;
            }
            let app = a.clone();
            std::thread::spawn(move || {
                // Do not enable a device while an editor flush/exit is pending.
                if PENDING_EXIT.lock().unwrap().is_none() {
                    let core = app.state::<Core>();
                    let enabled = core.0.settings.load().0["dock"]["enabled"]
                        .as_bool()
                        .unwrap_or(false);
                    if let Err(error) = core
                        .0
                        .dock_configure(&serde_json::json!({"enabled": !enabled}))
                    {
                        uc_core::device::log(&format!(
                            "tray dock setting failed: {}",
                            error.message
                        ));
                    }
                }
                DOCK_TOGGLE_BUSY.store(false, Ordering::SeqCst);
                TRAY_DIRTY.store(true, Ordering::SeqCst);
            });
        }
        "quit" => quit_app(a),
        _ => {}
    }
}

/// The menu labels as they stand (debug test hook).
#[cfg(debug_assertions)]
pub fn tray_state() -> String {
    match TRAY.lock().unwrap().as_ref() {
        Some(t) => format!(
            "open_text={:?} dock_enabled={} profile_text={:?} icon_shown={}",
            t.open.text().unwrap_or_default(),
            t.dock_enabled.is_checked().unwrap_or(false),
            t.profile.text().unwrap_or_default(),
            !tray_icon_hidden()
        ),
        None => "no tray".into(),
    }
}

fn remove_tray(app: &AppHandle) {
    let _ = app.remove_tray_by_id(TRAY_ID);
    *TRAY.lock().unwrap() = None;
    TRAY_ON.store(false, Ordering::SeqCst);
}

/// Tray Quit first flushes editor-owned pending writes.
pub fn quit_app(app: &AppHandle) {
    request_exit(app, crate::quit_cmd::Request::Quit, true);
}

fn request_exit(app: &AppHandle, request: crate::quit_cmd::Request, background: bool) {
    *PENDING_EXIT.lock().unwrap() = Some((request, background));
    if EDITOR_READY.load(Ordering::SeqCst) {
        if let Some(window) = app.get_webview_window("main") {
            if tauri::Emitter::emit(&window, "uc:prepare-quit", ()).is_ok() {
                return;
            }
            // A live editor whose guard cannot be reached must keep its edits.
            *PENDING_EXIT.lock().unwrap() = None;
            uc_core::device::log("quit deferred: editor flush guard unavailable");
            return;
        }
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = finish_exit(&app) {
            uc_core::device::log(&error.message);
        }
    });
}

fn finish_exit(app: &AppHandle) -> Result<(), CoreError> {
    let _finish = EXIT_FINISH.lock().unwrap();
    // A late/unsolicited editor acknowledgement must not stop a live device.
    if PENDING_EXIT.lock().unwrap().is_none() {
        return Ok(());
    }
    // Refuse before shutdown: a busy scheduler leaves the existing runtime usable.
    if app.state::<Core>().0.sched.any_visible_job() {
        return Err(CoreError::new(
            "Work is still running; the app has stayed open.",
        ));
    }
    app.state::<Core>().0.prepare_cooler_exit()?;
    let Some((request, background)) = PENDING_EXIT.lock().unwrap().take() else {
        app.state::<Core>().0.cancel_cooler_exit();
        return Ok(());
    };
    // Prepare any fallible relaunch first. Its helper waits for this process to
    // exit; a spawn failure must leave the current device runtime usable.
    let helper = match request {
        crate::quit_cmd::Request::Quit => Ok(()),
        crate::quit_cmd::Request::Restart => {
            crate::relaunch::spawn_helper(std::process::id(), background, None)
                .map_err(|e| CoreError::new(e.to_string()))
        }
        crate::quit_cmd::Request::Update(path) => crate::relaunch::spawn_helper(
            std::process::id(),
            true,
            Some(std::path::Path::new(&path)),
        )
        .map_err(|e| CoreError::new(e.to_string())),
    };
    if let Err(error) = helper {
        app.state::<Core>().0.cancel_cooler_exit();
        return Err(error);
    }
    // Stop admission and wait for in-flight action recovery before arming the
    // exit watchdog. This function is always called off the window event thread.
    app.state::<Core>().0.stop_runtime();
    arm_exit_watchdog("tray Quit");
    let a = app.clone();
    std::thread::spawn(move || {
        let b = a.clone();
        let _ = a.run_on_main_thread(move || {
            remove_tray(&b);
            b.exit(0);
        });
    });
    Ok(())
}

#[tauri::command]
pub fn editor_session_ready(ready: bool) {
    EDITOR_READY.store(ready, Ordering::SeqCst);
}

#[tauri::command]
pub async fn editor_quit_ready(app: AppHandle, saved: bool) -> Result<(), CoreError> {
    if !saved {
        *PENDING_EXIT.lock().unwrap() = None;
        show_main(&app);
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || finish_exit(&app))
        .await
        .map_err(|_| CoreError::new("Could not finish shutdown"))?
}

/// Carry out a request from `--quit` / `--restart` / `--update` or the General page button: leave a detached helper when the
/// app is to come back (restart, update), then quit exactly like the tray's Quit. If the helper cannot be started the app does
/// not quit.
pub fn restart_app(app: &AppHandle, req: crate::quit_cmd::Request) {
    request_exit(app, req, true);
}

/// General > "Restart Unified Control" (troubleshooting): the tray Quit path, then a fresh copy started by the app itself with
/// its window as usual. Refused with a clear message while a job runs.
#[tauri::command]
pub async fn app_restart(app: AppHandle, core: tauri::State<'_, Core>) -> Result<Value, CoreError> {
    if core.0.busy_for_quit() {
        return Err(CoreError::new(
            "Something is still running. Restart when it has finished.",
        ));
    }
    uc_core::device::log("restart requested (General): the tray Quit path");
    request_exit(&app, crate::quit_cmd::Request::Restart, false);
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
