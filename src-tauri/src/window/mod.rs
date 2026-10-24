//! Seamless frameless window (vault pattern "Seamless frameless window").
//!
//! The window has no native frame; the page draws a transparent 32 px title bar with three 46x32 buttons
//! (public/js/titlebar.js). Windows 11 Snap Layouts (hover over maximise, Win+Z) need the system to see
//! HTMAXBUTTON in WM_NCHITTEST over the maximise button, but WebView2 covers the whole client area and
//! answers hit tests itself. So `snap` puts a small invisible native child window over the maximise
//! button: it answers HTMAXBUTTON (triggering the flyout) and forwards its mouse state to the page as
//! `cs-snap` events, so the HTML button can still draw hover / pressed / click.
//! Size, position and maximised state are remembered in %APPDATA%\UnifiedControl\window.json (`rect`).
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, WebviewWindow, WebviewWindowBuilder, WindowEvent};

#[cfg(windows)]
mod rect;
#[cfg(windows)]
mod snap;

pub const BG: (u8, u8, u8) = (0x0f, 0x11, 0x15); // public/style.css --bg
pub const TITLEBAR_H: u32 = 32;
pub const BUTTON_W: u32 = 46;
/// Buttons right of maximise (close).
pub const BUTTONS_RIGHT_OF_MAX: u32 = 1;

static FORCE_CLOSE: AtomicBool = AtomicBool::new(false);

/// True after the page confirmed "close anyway" (skips the owner's close guard).
pub fn force_close_requested() -> bool {
    FORCE_CLOSE.load(Ordering::SeqCst)
}

/// Forget a "close anyway" confirmation (the window was hidden to the tray instead of closed).
pub fn clear_force_close() {
    FORCE_CLOSE.store(false, Ordering::SeqCst);
}

/// `UC_TEST_QUIET=1`: test/development instances never intrude on the user's desktop. The window is created without
/// focus, off-screen, out of the taskbar and never activated or brought to the front (WebView2 still renders, so the page can
/// be driven over the debug port). Its position is not remembered.
pub fn quiet() -> bool {
    std::env::var("UC_TEST_QUIET").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// How a window is put on the screen: normally (shown and focused), minimised without activation (`--background` with Live off),
/// or quietly (test instances: off-screen, never activated).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Present {
    Normal,
    MinimizedNoActivate,
}

/// Show the window. Quiet mode always wins: no activation, no taskbar button, off-screen.
pub fn present(win: &WebviewWindow, how: Present) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let hwnd = win.hwnd().map(|h| h.0 as *mut core::ffi::c_void);
        if quiet() {
            if let Ok(h) = hwnd {
                unsafe {
                    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
                    SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize);
                    SetWindowPos(h, std::ptr::null_mut(), -32000, -32000, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                    ShowWindow(h, SW_SHOWNOACTIVATE);
                }
                return;
            }
        }
        if how == Present::MinimizedNoActivate {
            if let Ok(h) = hwnd {
                unsafe {
                    ShowWindow(h, SW_SHOWMINNOACTIVE);
                }
                return;
            }
        }
    }
    if quiet() {
        let _ = win.set_position(tauri::PhysicalPosition::new(-32000, -32000));
        let _ = win.show();
        return;
    }
    if how == Present::MinimizedNoActivate {
        let _ = win.minimize();
        let _ = win.show();
        return;
    }
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
}

/// Builds the main window (hidden; the caller shows it). The caller adds its own `CloseRequested` guard.
pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let saved = load_saved(app);
    let win = WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title(uc_core::core::buildinfo::title(env!("CARGO_PKG_VERSION")))
        .inner_size(1280.0, 860.0)
        .min_inner_size(900.0, 640.0)
        .decorations(false)
        .shadow(true)
        .background_color(tauri::webview::Color(BG.0, BG.1, BG.2, 255))
        .disable_drag_drop_handler() // HTML5 drop reaches the page, as in Electron
        .visible(false)
        .focused(false)
        .skip_taskbar(quiet())
        .build()?;
    let saved = if quiet() { None } else { saved }; // test instances neither restore nor remember a position
    #[cfg(windows)]
    if let Some(s) = saved {
        // Same logical size on the monitor the window lands on (scale may differ from when it was saved).
        let k = (win.scale_factor().unwrap_or(s.scale) / s.scale).clamp(0.25, 4.0);
        let (w, h) = ((s.w as f64 * k).round().min(100_000.0) as u32, (s.h as f64 * k).round().min(100_000.0) as u32);
        rect::set(&win, s.x, s.y, w, h);
        if s.maximized {
            let _ = win.maximize(); // un-maximising returns to the rectangle above
        }
    }
    *CURRENT.lock().unwrap() = saved;
    let w2 = win.clone();
    win.on_window_event(move |e| match e {
        WindowEvent::Resized(_) => {
            let _ = w2.emit("window-state", json!({ "maximized": w2.is_maximized().unwrap_or(false) }));
            remember(&w2, false);
        }
        WindowEvent::Moved(_) => remember(&w2, false),
        WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => remember(&w2, true),
        _ => {}
    });
    #[cfg(windows)]
    snap::install(&win, TITLEBAR_H, BUTTON_W, BUTTONS_RIGHT_OF_MAX);
    Ok(win)
}

// ---------- remembered size / position / maximised state ----------

/// Outer rectangle in physical pixels of the last normal (not maximised, not minimised) window, plus the scale
/// factor it was saved at, so a different monitor scale restores the same logical size.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
struct Saved {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    scale: f64,
    maximized: bool,
}

static CURRENT: Mutex<Option<Saved>> = Mutex::new(None);
static LAST_WRITE: Mutex<Option<Instant>> = Mutex::new(None);

fn state_path() -> std::path::PathBuf {
    uc_core::core::paths::base().join("window.json")
}

/// Reads window.json. Anything unreadable, nonsensical or not on a connected monitor gives `None` (default size and
/// place). Otherwise the rectangle is clamped into the work area of the monitor it mostly sits on. All arithmetic is
/// in i64 so a corrupt file cannot overflow.
fn load_saved(app: &AppHandle) -> Option<Saved> {
    let text = std::fs::read_to_string(state_path()).ok()?;
    let mut s = serde_json::from_str::<Saved>(text.trim_start_matches('\u{feff}')).ok()?;
    if !(s.scale.is_finite() && s.scale > 0.4 && s.scale < 8.0) || s.w > 100_000 || s.h > 100_000 {
        return None;
    }
    if s.x.unsigned_abs() > 1_000_000 || s.y.unsigned_abs() > 1_000_000 {
        return None;
    }
    let mons = app.available_monitors().unwrap_or_default();
    if mons.is_empty() {
        return (s.w >= 920 && s.h >= 660).then_some(s); // cannot tell: trust a sane file, else the default
    }
    let (x, y, w, h) = (s.x as i64, s.y as i64, s.w as i64, s.h as i64);
    // Monitor with the largest overlap of the title-bar strip; its work area (without the taskbar) is the limit.
    let mut best: Option<(i64, (i64, i64, i64, i64))> = None;
    for m in &mons {
        let (mp, ms) = (m.position(), m.size());
        let (mx, my, mw, mh) = (mp.x as i64, mp.y as i64, ms.width as i64, ms.height as i64);
        let ox = (x + w).min(mx + mw) - x.max(mx);
        let oy = (y + 80).min(my + mh) - y.max(my);
        if ox >= 160 && oy >= 40 && best.is_none_or(|(a, _)| ox * oy > a) {
            let wa = m.work_area();
            let (wx, wy) = (wa.position.x as i64, wa.position.y as i64);
            let (ww, wh) = (wa.size.width as i64, wa.size.height as i64);
            let area = if ww >= 300 && wh >= 300 { (wx, wy, ww, wh) } else { (mx, my, mw, mh) };
            best = Some((ox * oy, area));
        }
    }
    let (wx, wy, ww, wh) = best?.1; // off-screen (monitor unplugged): the default size and place
    // At least the window's minimum size (900x640 inner; SetWindowPos ignores it), but never larger than the work area.
    let min_w = (920.0 * s.scale).round() as i64;
    let min_h = (660.0 * s.scale).round() as i64;
    let nw = w.max(min_w).min(ww);
    let nh = h.max(min_h).min(wh);
    let nx = x.clamp(wx, (wx + ww - nw).max(wx));
    let ny = y.clamp(wy, (wy + wh - nh).max(wy));
    (s.x, s.y, s.w, s.h) = (nx as i32, ny as i32, nw as u32, nh as u32);
    Some(s)
}

/// Write via a temp file and rename, so a crash mid-write never leaves a half-written window.json.
fn write_atomic(path: &std::path::Path, text: &str) {
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

fn remember(win: &WebviewWindow, force: bool) {
    if quiet() || win.is_minimized().unwrap_or(false) {
        return;
    }
    let maximized = win.is_maximized().unwrap_or(false);
    let mut cur = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = cur.unwrap_or(Saved { x: 0, y: 0, w: 0, h: 0, scale: 1.0, maximized });
    s.maximized = maximized;
    if !maximized {
        #[cfg(windows)]
        if let (Some((x, y, w, h)), Ok(f)) = (rect::get(win), win.scale_factor()) {
            (s.x, s.y, s.w, s.h, s.scale) = (x, y, w, h, f);
        }
    }
    if s.w == 0 {
        return; // never saw a normal rectangle yet
    }
    *cur = Some(s);
    drop(cur);
    let mut last = LAST_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    if force || last.is_none_or(|t| t.elapsed() > Duration::from_millis(500)) {
        *last = Some(Instant::now());
        if let Ok(t) = serde_json::to_string(&s) {
            let _ = std::fs::create_dir_all(uc_core::core::paths::base());
            write_atomic(&state_path(), &t);
        }
    }
}

#[tauri::command]
pub fn win_minimize(window: WebviewWindow) {
    let _ = window.minimize();
}

/// Maximise to the work area (taskbar stays visible); returns the new maximised state.
#[tauri::command]
pub fn win_toggle_maximize(window: WebviewWindow) -> bool {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
        false
    } else {
        let _ = window.maximize();
        true
    }
}

#[tauri::command]
pub fn win_state(window: WebviewWindow) -> bool {
    window.is_maximized().unwrap_or(false)
}

#[tauri::command]
pub fn win_close(window: WebviewWindow) {
    let _ = window.close(); // goes through the owner's close guard
}

#[tauri::command]
pub fn win_force_close(window: WebviewWindow) {
    FORCE_CLOSE.store(true, Ordering::SeqCst);
    let _ = window.close();
}
