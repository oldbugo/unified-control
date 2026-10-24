//! Outer window rectangle in physical pixels, read and set through Win32 directly. (tao's inner/outer size
//! helpers disagree by the frameless window's invisible resize border, which made a remembered size grow on every
//! launch; GetWindowRect / SetWindowPos are the same on both sides.)
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::WebviewWindow;
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER},
};

fn hwnd(win: &WebviewWindow) -> Option<HWND> {
    match win.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as HWND),
        _ => None,
    }
}

/// (x, y, width, height)
pub fn get(win: &WebviewWindow) -> Option<(i32, i32, u32, u32)> {
    let h = hwnd(win)?;
    let mut r: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(h, &mut r) } == 0 {
        return None;
    }
    Some((r.left, r.top, (r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32))
}

pub fn set(win: &WebviewWindow, x: i32, y: i32, w: u32, h: u32) {
    if let Some(hw) = hwnd(win) {
        unsafe { SetWindowPos(hw, std::ptr::null_mut(), x, y, w as i32, h as i32, SWP_NOZORDER | SWP_NOACTIVATE) };
    }
}
