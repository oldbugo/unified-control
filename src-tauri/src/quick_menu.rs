//! The Vibe Key quick menu window (core logic in `core/au05_menu.rs`).
//!
//! A small frameless, transparent, always-on-top window, out of the taskbar (0.34.0: a dial of rings; only
//! the rings and the caption show). It is not placed at the mouse: by default near the top-right corner of
//! the primary monitor's work area. The user can drag it by the hole in the middle; the monitor (device name
//! and rectangle) and the dial's centre on it are remembered in `quick-menu.json` in the app folder. If that
//! monitor is gone, it goes back to the default place. The window grows with the rings shown (1 to 3) around
//! that centre, always inside the work area. Sizes are logical pixels, so each monitor's own scale applies.
//! It takes focus when it opens.
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
const MARGIN: f64 = 24.0;
/// The dial window's logical size for this many rings (1 to 3): the active ring's outer radius (225, 254,
/// 305; 0.37.0: a thicker band), Close/Back's taper (12), the push-out of the highlighted segment (10) and a
/// margin (14) each side, at least 380 wide, plus the caption and the time bar (96). Matches `dialSize` in
/// public/js/quick-menu-dial.js. On a smaller work area the window is clamped and the page draws the dial
/// smaller.
pub fn dial_size(rings: usize) -> (f64, f64) {
    let outer = [225.0, 254.0, 305.0][rings.clamp(1, 3) - 1];
    let d = 2.0 * (outer + 12.0 + 10.0 + 14.0);
    (f64::max(d, 380.0), d + 96.0)
}
/// The list style's window (1.1.0).
pub const LIST_SIZE: (f64, f64) = (440.0, 560.0);
/// The window's logical size for a view: the list, or the dial with its rings.
pub fn view_size(view: &Value) -> (f64, f64) {
    if view["style"] == "list" { LIST_SIZE } else { dial_size(rings(view)) }
}
/// The rings a view shows: its level and up to two above it.
fn rings(view: &Value) -> usize {
    view["levels"].as_array().map_or(0, |l| l.len().min(2)) + 1
}
/// The caption under the dial takes 96 logical pixels: the window's centre is half of that below the dial's
/// centre (physical pixels at this scale).
fn half_caption(scale: f64) -> i64 {
    (48.0 * scale).round() as i64
}


pub fn enabled() -> bool {
    !crate::window::quiet() && uc_core::core::buildinfo::fake_device().is_none()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Place {
    monitor: String,
    /// The monitor's rectangle (physical pixels) when the place was saved.
    rect: [i64; 4],
    /// The window's top-left corner relative to the monitor (physical pixels; before 0.34.0).
    x: i64,
    y: i64,
    /// The dial's centre relative to the monitor (physical pixels; 0.34.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    center: Option<[i64; 2]>,
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
        .inner_size(dial_size(1).0, dial_size(1).1)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .background_color(tauri::webview::Color(r, g, b, 0))
        .visible(false)
        .focused(false)
        .build()?;
    let w2 = win.clone();
    win.on_window_event(move |e| {
        if let WindowEvent::Moved(pos) = e {
            if *PLACED.lock_or_recover() == Some((pos.x, pos.y)) {
                return; // our own placement
            }
            // The user is dragging it (0.35.0): the page ignores the focus loss of Windows' move loop and holds
            // the idle close until the moves settle.
            let _ = w2.emit("uc:au05-menu-moved", ());
            if let (Ok(Some(m)), Ok(size)) = (w2.current_monitor(), w2.outer_size()) {
                let r = rect(&m);
                save_place(&Place {
                    monitor: m.name().cloned().unwrap_or_default(),
                    rect: r,
                    x: pos.x as i64 - r[0],
                    y: pos.y as i64 - r[1],
                    // The dial's centre: the window's centre less half the caption.
                    center: Some([
                        pos.x as i64 + size.width as i64 / 2 - r[0],
                        pos.y as i64 + size.height as i64 / 2 - half_caption(m.scale_factor()) - r[1],
                    ]),
                });
            }
        }
    });
    Ok(win)
}

/// The top-left corner (physical) for a window of `w` x `h` centred at `c`, inside the work area `wa` (x, y,
/// width, height). A window larger than the work area starts at its corner.
fn fit(c: (i64, i64), w: i64, h: i64, wa: (i64, i64, i64, i64)) -> (i32, i32) {
    let (wx, wy, ww, wh) = wa;
    let x = (c.0 - w / 2).clamp(wx, (wx + ww - w).max(wx));
    let y = (c.1 - h / 2).clamp(wy, (wy + wh - h).max(wy));
    (x as i32, y as i32)
}

/// The logical size, made to fit the work area (physical) at this scale.
fn fitted_size(size: (f64, f64), scale: f64, wa: (i64, i64, i64, i64)) -> (f64, f64) {
    let (w, h) = size;
    (w.min(wa.2 as f64 / scale), h.min(wa.3 as f64 / scale))
}

fn work_area(m: &tauri::Monitor) -> (i64, i64, i64, i64) {
    let wa = m.work_area();
    (wa.position.x as i64, wa.position.y as i64, wa.size.width as i64, wa.size.height as i64)
}

/// Grow or shrink the window for another size (rings or style), around its current centre.
fn resize(win: &WebviewWindow, want: (f64, f64)) {
    let (Ok(pos), Ok(size), Ok(Some(m))) = (win.outer_position(), win.outer_size(), win.current_monitor()) else {
        return;
    };
    let scale = m.scale_factor();
    let wa = work_area(&m);
    let (lw, lh) = fitted_size(want, scale, wa);
    let (w, h) = ((lw * scale).round() as i64, (lh * scale).round() as i64);
    if (w, h) == (size.width as i64, size.height as i64) {
        return;
    }
    // The window's centre stays (so does the dial's: it is a fixed distance above it).
    let c = (pos.x as i64 + size.width as i64 / 2, pos.y as i64 + size.height as i64 / 2);
    let (x, y) = fit(c, w, h, wa);
    *PLACED.lock_or_recover() = Some((x, y));
    let _ = win.set_size(LogicalSize::new(lw, lh));
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

/// Put the window on the remembered monitor and place, or near the top-right of the primary monitor.
fn place(app: &AppHandle, win: &WebviewWindow, want: (f64, f64)) {
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
    let wa = work_area(&m);
    let (lw, lh) = fitted_size(want, scale, wa);
    let (w, h) = ((lw * scale).round() as i64, (lh * scale).round() as i64);
    let r = rect(&m);
    // The window's centre (the dial is at its top, the caption below).
    let c = match saved {
        Some(Place { center: Some([cx, cy]), .. }) => (r[0] + cx, r[1] + cy + half_caption(scale)),
        // Saved by an older version (the list window's top-left corner).
        Some(s) => (r[0] + s.x + w / 2, r[1] + s.y + h / 2),
        None => (wa.0 + wa.2 - w / 2 - (MARGIN * scale) as i64, wa.1 + h / 2 + (MARGIN * scale) as i64),
    };
    // Always fully on the work area.
    let (x, y) = fit(c, w, h, wa);
    *PLACED.lock_or_recover() = Some((x, y));
    let _ = win.set_position(PhysicalPosition::new(x, y));
    // Logical size after the move, so the new monitor's scale applies.
    let _ = win.set_size(LogicalSize::new(lw, lh));
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
            let want = view_size(&view);
            if first {
                place(&app, &win, want);
            } else {
                resize(&win, want);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The window grows with the rings (the same sizes as `dialSize` in public/js/quick-menu-dial.js, checked
    /// by scripts/check-quick-menu.cjs) and stays inside the work area.
    #[test]
    fn the_menu_window_fits_its_rings_or_style_and_the_work_area() {
        assert_eq!(dial_size(1), (522.0, 618.0));
        assert_eq!(dial_size(2), (580.0, 676.0));
        assert_eq!(dial_size(3), (682.0, 778.0));
        assert_eq!(dial_size(9), dial_size(3), "at most 3 rings");
        assert_eq!(rings(&serde_json::json!({"levels": [{}, {}]})), 3);
        assert_eq!(rings(&serde_json::json!({})), 1);
        assert_eq!(view_size(&serde_json::json!({"style": "list", "levels": [{}]})), LIST_SIZE);
        assert_eq!(view_size(&serde_json::json!({"style": "dial", "levels": [{}]})), dial_size(2));
        let wa = (0, 0, 1920, 1040);
        assert_eq!(fit((960, 520), 682, 778, wa), (619, 131));
        assert_eq!(fit((1900, 20), 682, 778, wa), (1238, 0), "clamped into the work area");
        assert_eq!(fitted_size(dial_size(3), 1.0, (0, 0, 1280, 600)), (682.0, 600.0), "never taller than the work area");
    }
}
