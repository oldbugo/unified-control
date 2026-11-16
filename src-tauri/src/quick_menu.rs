//! The Vibe Key quick menu window (core logic in `core/au05_menu.rs`).
//!
//! A small frameless, always-on-top window, out of the taskbar. It is not placed at the mouse: by default
//! near the top-right corner of the primary monitor's work area. The user can drag it by its header; the
//! monitor (device name and rectangle) and the position on it are remembered in `quick-menu.json` in the app
//! folder. If that monitor is gone, it goes back to the default place. Sizes are logical pixels, so each
//! monitor's own scale applies. It takes focus when it opens.
//!
//! Never installed in quiet (`UC_TEST_QUIET`) or fake (`UC_FAKE_DEVICE`) instances: the window never appears
//! during tests.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use uc_core::core::au05_menu::MenuView;
use uc_core::core::sync::MutexExt;

const LABEL: &str = "quickmenu";
const WIDTH: f64 = 340.0;
const MARGIN: f64 = 24.0;
/// The page's header (58), the list padding (8), one row per entry (40; at most 10 shown, more scroll) and
/// the one-line footer (36). Matches public/css/quick-menu.css.
fn height(rows: usize) -> f64 {
    58.0 + 8.0 + 40.0 * rows.clamp(1, 10) as f64 + 36.0
}

pub fn enabled() -> bool {
    !crate::window::quiet() && uc_core::core::buildinfo::fake_device().is_none()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Place {
    monitor: String,
    /// The monitor's rectangle (physical pixels) when the place was saved.
    rect: [i64; 4],
    /// The window's top-left corner relative to the monitor (physical pixels).
    x: i64,
    y: i64,
}

fn place_path() -> std::path::PathBuf {
    uc_core::core::paths::base().join("quick-menu.json")
}
fn load_place() -> Option<Place> {
    let text = std::fs::read_to_string(place_path()).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}
fn save_place(p: &Place) {
    let path = place_path();
    let tmp = path.with_extension("json.tmp");
    if let Ok(t) = serde_json::to_string(p) {
        if std::fs::write(&tmp, t).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

fn rect(m: &tauri::Monitor) -> [i64; 4] {
    [m.position().x as i64, m.position().y as i64, m.size().width as i64, m.size().height as i64]
}

/// Where the window was put last (physical), so our own move is not saved as the user's.
static PLACED: Mutex<Option<(i32, i32)>> = Mutex::new(None);

pub struct TauriMenu {
    app: AppHandle,
}

impl TauriMenu {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let (r, g, b) = crate::window::BG;
    let win = WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("quick-menu.html".into()))
        .title("Vibe Key quick menu")
        .inner_size(WIDTH, height(7))
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(true)
        .background_color(tauri::webview::Color(r, g, b, 255))
        .visible(false)
        .focused(false)
        .build()?;
    let w2 = win.clone();
    win.on_window_event(move |e| {
        if let WindowEvent::Moved(pos) = e {
            if *PLACED.lock_or_recover() == Some((pos.x, pos.y)) {
                return; // our own placement
            }
            if let Ok(Some(m)) = w2.current_monitor() {
                let r = rect(&m);
                save_place(&Place {
                    monitor: m.name().cloned().unwrap_or_default(),
                    rect: r,
                    x: pos.x as i64 - r[0],
                    y: pos.y as i64 - r[1],
                });
            }
        }
    });
    Ok(win)
}

/// Put the window on the remembered monitor and place, or near the top-right of the primary monitor.
fn place(app: &AppHandle, win: &WebviewWindow, rows: usize) {
    let monitors = app.available_monitors().unwrap_or_default();
    let saved = load_place();
    let remembered = saved.as_ref().and_then(|s| {
        monitors
            .iter()
            .find(|m| m.name().map(String::as_str) == Some(s.monitor.as_str()) && rect(m) == s.rect)
            .map(|m| (m.clone(), s.clone()))
    });
    let (monitor, saved) = match remembered {
        Some((m, s)) => (Some(m), Some(s)),
        None => (app.primary_monitor().ok().flatten().or_else(|| monitors.first().cloned()), None),
    };
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let (w, h) = ((WIDTH * scale).round() as i64, (height(rows) * scale).round() as i64);
    let wa = m.work_area();
    let (wx, wy, ww, wh) = (wa.position.x as i64, wa.position.y as i64, wa.size.width as i64, wa.size.height as i64);
    let (x, y) = match saved {
        Some(s) => (rect(&m)[0] + s.x, rect(&m)[1] + s.y),
        None => (wx + ww - w - (MARGIN * scale) as i64, wy + (MARGIN * scale) as i64),
    };
    // Always fully on the work area.
    let x = x.clamp(wx, (wx + ww - w).max(wx)) as i32;
    let y = y.clamp(wy, (wy + wh - h).max(wy)) as i32;
    *PLACED.lock_or_recover() = Some((x, y));
    let _ = win.set_position(PhysicalPosition::new(x, y));
    // Logical size after the move, so the new monitor's scale applies.
    let _ = win.set_size(LogicalSize::new(WIDTH, height(rows)));
}

impl MenuView for TauriMenu {
    fn show(&self, view: &Value, first: bool) {
        if !enabled() {
            return;
        }
        let (app, view) = (self.app.clone(), view.clone());
        let _ = self.app.run_on_main_thread(move || {
            let win = match app.get_webview_window(LABEL) {
                Some(w) => w,
                None => match build(&app) {
                    Ok(w) => w,
                    Err(_) => {
                        uc_core::device::log("quick menu window could not be created");
                        return;
                    }
                },
            };
            let rows = view["rows"].as_array().map(|r| r.len()).unwrap_or(0);
            if first {
                place(&app, &win, rows);
            } else {
                let _ = win.set_size(LogicalSize::new(WIDTH, height(rows)));
            }
            let _ = app.emit("uc:au05-menu", &view);
            if first {
                let _ = win.show();
                let _ = win.set_focus();
            }
        });
    }
    fn hide(&self) {
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            if let Some(w) = app.get_webview_window(LABEL) {
                let _ = w.hide();
            }
            let _ = app.emit("uc:au05-menu", serde_json::json!({"open": false}));
        });
    }
}
