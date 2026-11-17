//! Input tester: every key, button, wheel and HID control event, with the device that sent it.
//!
//! - Windows Raw Input on a hidden window that UC owns (never shown, never focused). It is registered with
//!   `RIDEV_INPUTSINK` only while the Input tester page is open: the page starts it, polls it, and stops it when it
//!   leaves. If the page stops polling (window closed to the tray, page reloaded) the listener stops itself after
//!   10 s. Raw Input only reads; it never blocks or changes input for other apps.
//! - Keys that UC claims as global hotkeys (the Vibe Key's F19–F24 actions) never reach other apps, but Raw Input
//!   still sees them: such key presses are marked `claimed`.
//! - Fake and quiet test instances (`UC_FAKE_DEVICE`, `UC_TEST_QUIET`) never register Raw Input. They list simulated
//!   devices and take events from `input_tester_test_inject` (debug fake instances only).
//! - Decoding (`decode_key`, `decode_mouse`, HID usage changes) is pure and unit-tested with fake records.
//! - A microphone recording is saved only when the user presses Save (`input_tester_save_audio`).
use super::commands::Core;
use super::error::{CoreError, CoreResult};
use super::sync::MutexExt;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::State;

/// Events kept for the page (newest last). Older ones are dropped and counted.
const KEEP: usize = 4000;
/// The listener stops itself when the page has not polled for this long.
pub const IDLE_STOP: Duration = Duration::from_secs(10);
/// HID reports without button changes (sticks, sliders) are logged at most this often per device.
const HID_REPORT_GAP: Duration = Duration::from_millis(100);

// ---------- raw records (what Windows delivers, or a test injects) ----------

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// RAWKEYBOARD: MakeCode, Flags (RI_KEY_BREAK 1, RI_KEY_E0 2, RI_KEY_E1 4), VKey.
    Key { device: String, make: u16, flags: u16, vk: u16 },
    /// RAWMOUSE: usButtonFlags, usButtonData, lLastX, lLastY.
    Mouse { device: String, buttons: u16, data: u16, dx: i32, dy: i32 },
    /// RAWHID: the usages that are on in this report (page, usage) and the report bytes.
    Hid { device: String, usages: Vec<(u16, u16)>, bytes: Vec<u8> },
}

impl Record {
    fn device(&self) -> &str {
        match self {
            Record::Key { device, .. } | Record::Mouse { device, .. } | Record::Hid { device, .. } => device,
        }
    }
    /// A test record: `{"type":"key","device":"fake-au05","make":109,"flags":0,"vk":133}`.
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let device = v["device"].as_str().ok_or("device is required")?.to_string();
        let n = |k: &str| v[k].as_i64().unwrap_or(0);
        match v["type"].as_str() {
            Some("key") => Ok(Record::Key { device, make: n("make") as u16, flags: n("flags") as u16, vk: n("vk") as u16 }),
            Some("mouse") => Ok(Record::Mouse {
                device,
                buttons: n("buttons") as u16,
                data: n("data") as u16,
                dx: n("dx") as i32,
                dy: n("dy") as i32,
            }),
            Some("hid") => Ok(Record::Hid {
                device,
                usages: v["usages"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|u| Some((u[0].as_u64()? as u16, u[1].as_u64()? as u16)))
                            .collect()
                    })
                    .unwrap_or_default(),
                bytes: v["bytes"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|b| b.as_u64().map(|b| b as u8)).collect())
                    .unwrap_or_default(),
            }),
            _ => Err("type must be key, mouse or hid".into()),
        }
    }
}

// ---------- decoding ----------

pub const RI_KEY_BREAK: u16 = 1;
pub const RI_KEY_E0: u16 = 2;
pub const RI_KEY_E1: u16 = 4;

/// One decoded key transition.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    /// Physical position, as in the web `KeyboardEvent.code` ("KeyA", "F22", "NumpadEnter").
    pub code: String,
    /// What the key means in the current layout, from the virtual key ("A", "F22", "Home").
    pub name: String,
    pub vk: u16,
    pub scan: u16,
    pub ext: bool,
    pub up: bool,
}

/// Scan code (set 1, as Raw Input gives it) to the physical key code. `e0`: the extended prefix.
pub fn scan_code(scan: u16, e0: bool) -> Option<&'static str> {
    Some(match (e0, scan) {
        (false, 0x01) => "Escape",
        (false, 0x02..=0x0A) => ["Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9"]
            [(scan - 0x02) as usize],
        (false, 0x0B) => "Digit0",
        (false, 0x0C) => "Minus",
        (false, 0x0D) => "Equal",
        (false, 0x0E) => "Backspace",
        (false, 0x0F) => "Tab",
        (false, 0x10..=0x19) => ["KeyQ", "KeyW", "KeyE", "KeyR", "KeyT", "KeyY", "KeyU", "KeyI", "KeyO", "KeyP"]
            [(scan - 0x10) as usize],
        (false, 0x1A) => "BracketLeft",
        (false, 0x1B) => "BracketRight",
        (false, 0x1C) => "Enter",
        (false, 0x1D) => "ControlLeft",
        (false, 0x1E..=0x26) => ["KeyA", "KeyS", "KeyD", "KeyF", "KeyG", "KeyH", "KeyJ", "KeyK", "KeyL"]
            [(scan - 0x1E) as usize],
        (false, 0x27) => "Semicolon",
        (false, 0x28) => "Quote",
        (false, 0x29) => "Backquote",
        (false, 0x2A) => "ShiftLeft",
        (false, 0x2B) => "Backslash",
        (false, 0x2C..=0x32) => ["KeyZ", "KeyX", "KeyC", "KeyV", "KeyB", "KeyN", "KeyM"][(scan - 0x2C) as usize],
        (false, 0x33) => "Comma",
        (false, 0x34) => "Period",
        (false, 0x35) => "Slash",
        (false, 0x36) => "ShiftRight",
        (false, 0x37) => "NumpadMultiply",
        (false, 0x38) => "AltLeft",
        (false, 0x39) => "Space",
        (false, 0x3A) => "CapsLock",
        (false, 0x3B..=0x44) => ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"][(scan - 0x3B) as usize],
        (_, 0x45) => "NumLock",
        (false, 0x46) => "ScrollLock",
        (false, 0x47) => "Numpad7",
        (false, 0x48) => "Numpad8",
        (false, 0x49) => "Numpad9",
        (false, 0x4A) => "NumpadSubtract",
        (false, 0x4B) => "Numpad4",
        (false, 0x4C) => "Numpad5",
        (false, 0x4D) => "Numpad6",
        (false, 0x4E) => "NumpadAdd",
        (false, 0x4F) => "Numpad1",
        (false, 0x50) => "Numpad2",
        (false, 0x51) => "Numpad3",
        (false, 0x52) => "Numpad0",
        (false, 0x53) => "NumpadDecimal",
        (false, 0x54) => "PrintScreen", // Alt + Print Screen (SysRq)
        (false, 0x56) => "IntlBackslash",
        (false, 0x57) => "F11",
        (false, 0x58) => "F12",
        (false, 0x59) => "NumpadEqual",
        (false, 0x64..=0x6E) => ["F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23"]
            [(scan - 0x64) as usize],
        (false, 0x70) => "KanaMode",
        (false, 0x73) => "IntlRo",
        (false, 0x76) => "F24",
        (false, 0x79) => "Convert",
        (false, 0x7B) => "NonConvert",
        (false, 0x7D) => "IntlYen",
        (false, 0x7E) => "NumpadComma",
        (true, 0x10) => "MediaTrackPrevious",
        (true, 0x19) => "MediaTrackNext",
        (true, 0x1C) => "NumpadEnter",
        (true, 0x1D) => "ControlRight",
        (true, 0x20) => "AudioVolumeMute",
        (true, 0x21) => "LaunchApp2",
        (true, 0x22) => "MediaPlayPause",
        (true, 0x24) => "MediaStop",
        (true, 0x2E) => "AudioVolumeDown",
        (true, 0x30) => "AudioVolumeUp",
        (true, 0x32) => "BrowserHome",
        (true, 0x35) => "NumpadDivide",
        (true, 0x37) => "PrintScreen",
        (true, 0x38) => "AltRight",
        (true, 0x47) => "Home",
        (true, 0x48) => "ArrowUp",
        (true, 0x49) => "PageUp",
        (true, 0x4B) => "ArrowLeft",
        (true, 0x4D) => "ArrowRight",
        (true, 0x4F) => "End",
        (true, 0x50) => "ArrowDown",
        (true, 0x51) => "PageDown",
        (true, 0x52) => "Insert",
        (true, 0x53) => "Delete",
        (true, 0x5B) => "MetaLeft",
        (true, 0x5C) => "MetaRight",
        (true, 0x5D) => "ContextMenu",
        (true, 0x5E) => "Power",
        (true, 0x5F) => "Sleep",
        (true, 0x63) => "WakeUp",
        (true, 0x65) => "BrowserSearch",
        (true, 0x66) => "BrowserFavorites",
        (true, 0x67) => "BrowserRefresh",
        (true, 0x68) => "BrowserStop",
        (true, 0x69) => "BrowserForward",
        (true, 0x6A) => "BrowserBack",
        (true, 0x6B) => "LaunchApp1",
        (true, 0x6C) => "LaunchMail",
        (true, 0x6D) => "MediaSelect",
        _ => return None,
    })
}

/// Virtual key to (physical code, display name). Used for the name, and for the code when there is no scan code
/// (input sent by software often has only a virtual key).
pub fn vk_key(vk: u16) -> Option<(String, String)> {
    let s = |c: &str, n: &str| Some((c.to_string(), n.to_string()));
    match vk {
        0x08 => s("Backspace", "Backspace"),
        0x09 => s("Tab", "Tab"),
        0x0C => s("Numpad5", "Clear"),
        0x0D => s("Enter", "Enter"),
        0x13 => s("Pause", "Pause"),
        0x14 => s("CapsLock", "Caps Lock"),
        0x1B => s("Escape", "Esc"),
        0x20 => s("Space", "Space"),
        0x21 => s("PageUp", "Page Up"),
        0x22 => s("PageDown", "Page Down"),
        0x23 => s("End", "End"),
        0x24 => s("Home", "Home"),
        0x25 => s("ArrowLeft", "Left"),
        0x26 => s("ArrowUp", "Up"),
        0x27 => s("ArrowRight", "Right"),
        0x28 => s("ArrowDown", "Down"),
        0x2C => s("PrintScreen", "Print Screen"),
        0x2D => s("Insert", "Insert"),
        0x2E => s("Delete", "Delete"),
        0x30..=0x39 => {
            let d = (vk - 0x30).to_string();
            Some((format!("Digit{d}"), d))
        }
        0x41..=0x5A => {
            let c = (vk as u8 as char).to_string();
            Some((format!("Key{c}"), c))
        }
        0x5B => s("MetaLeft", "Left Win"),
        0x5C => s("MetaRight", "Right Win"),
        0x5D => s("ContextMenu", "Menu"),
        0x5F => s("Sleep", "Sleep"),
        0x60..=0x69 => {
            let d = (vk - 0x60).to_string();
            Some((format!("Numpad{d}"), format!("Numpad {d}")))
        }
        0x6A => s("NumpadMultiply", "Numpad *"),
        0x6B => s("NumpadAdd", "Numpad +"),
        0x6C => s("NumpadComma", "Numpad ,"),
        0x6D => s("NumpadSubtract", "Numpad -"),
        0x6E => s("NumpadDecimal", "Numpad ."),
        0x6F => s("NumpadDivide", "Numpad /"),
        0x70..=0x87 => {
            let f = format!("F{}", vk - 0x6F);
            Some((f.clone(), f))
        }
        0x90 => s("NumLock", "Num Lock"),
        0x91 => s("ScrollLock", "Scroll Lock"),
        0xA0 => s("ShiftLeft", "Left Shift"),
        0xA1 => s("ShiftRight", "Right Shift"),
        0xA2 => s("ControlLeft", "Left Ctrl"),
        0xA3 => s("ControlRight", "Right Ctrl"),
        0xA4 => s("AltLeft", "Left Alt"),
        0xA5 => s("AltRight", "Right Alt"),
        0xA6 => s("BrowserBack", "Browser Back"),
        0xA7 => s("BrowserForward", "Browser Forward"),
        0xA8 => s("BrowserRefresh", "Browser Refresh"),
        0xA9 => s("BrowserStop", "Browser Stop"),
        0xAA => s("BrowserSearch", "Browser Search"),
        0xAB => s("BrowserFavorites", "Favourites"),
        0xAC => s("BrowserHome", "Browser Home"),
        0xAD => s("AudioVolumeMute", "Mute"),
        0xAE => s("AudioVolumeDown", "Volume Down"),
        0xAF => s("AudioVolumeUp", "Volume Up"),
        0xB0 => s("MediaTrackNext", "Next Track"),
        0xB1 => s("MediaTrackPrevious", "Previous Track"),
        0xB2 => s("MediaStop", "Stop"),
        0xB3 => s("MediaPlayPause", "Play / Pause"),
        0xB4 => s("LaunchMail", "Mail"),
        0xB5 => s("MediaSelect", "Media"),
        0xB6 => s("LaunchApp1", "App 1"),
        0xB7 => s("LaunchApp2", "App 2"),
        0xBA => s("Semicolon", ";"),
        0xBB => s("Equal", "="),
        0xBC => s("Comma", ","),
        0xBD => s("Minus", "-"),
        0xBE => s("Period", "."),
        0xBF => s("Slash", "/"),
        0xC0 => s("Backquote", "`"),
        0xDB => s("BracketLeft", "["),
        0xDC => s("Backslash", "\\"),
        0xDD => s("BracketRight", "]"),
        0xDE => s("Quote", "'"),
        0xE2 => s("IntlBackslash", "\\ (ISO)"),
        _ => None,
    }
}

/// Decode one keyboard record. `None` for the parts of a key sequence that are not a key of their own
/// (the "fake Shift" around Print Screen and Num Lock keys, and VKey 0xFF fillers).
pub fn decode_key(make: u16, flags: u16, vk: u16) -> Option<Key> {
    let up = flags & RI_KEY_BREAK != 0;
    let e0 = flags & RI_KEY_E0 != 0;
    let e1 = flags & RI_KEY_E1 != 0;
    if vk == 0xFF && !e1 {
        return None;
    }
    if e0 && (make == 0x2A || make == 0x36) {
        return None;
    }
    // Raw Input gives the generic Shift/Ctrl/Alt virtual keys; the side comes from the scan code or prefix.
    let vk = match vk {
        0x10 => if make == 0x36 { 0xA1 } else { 0xA0 },
        0x11 => if e0 { 0xA3 } else { 0xA2 },
        0x12 => if e0 { 0xA5 } else { 0xA4 },
        v => v,
    };
    let from_vk = vk_key(vk);
    let code = if e1 || vk == 0x13 {
        Some("Pause".to_string())
    } else if make != 0 {
        scan_code(make, e0).map(String::from)
    } else {
        None
    }
    .or_else(|| from_vk.as_ref().map(|(c, _)| c.clone()))
    .unwrap_or_else(|| format!("Scan{}{make:02X}", if e0 { "E0" } else { "" }));
    let name = from_vk
        .map(|(_, n)| n)
        .unwrap_or_else(|| if vk == 0 { code.clone() } else { format!("Key 0x{vk:02X}") });
    // Enter on the number pad shares the main Enter's virtual key.
    let name = if code == "NumpadEnter" { "Numpad Enter".to_string() } else { name };
    Some(Key { code, name, vk, scan: make, ext: e0, up })
}

/// One decoded mouse transition: (kind, action, name, value).
#[derive(Debug, Clone, PartialEq)]
pub struct MouseEvent {
    pub kind: &'static str,
    pub action: &'static str,
    pub code: &'static str,
    pub name: &'static str,
    pub delta: i32,
}

const BUTTONS: [(u16, u16, &str, &str); 5] = [
    (0x0001, 0x0002, "MouseLeft", "Left button"),
    (0x0004, 0x0008, "MouseRight", "Right button"),
    (0x0010, 0x0020, "MouseMiddle", "Middle button"),
    (0x0040, 0x0080, "MouseBack", "Back button (X1)"),
    (0x0100, 0x0200, "MouseForward", "Forward button (X2)"),
];
const RI_MOUSE_WHEEL: u16 = 0x0400;
const RI_MOUSE_HWHEEL: u16 = 0x0800;

/// Decode one mouse record into its button and wheel transitions (movement is counted, not decoded here).
pub fn decode_mouse(buttons: u16, data: u16) -> Vec<MouseEvent> {
    let mut out = Vec::new();
    for (down, up, code, name) in BUTTONS {
        if buttons & down != 0 {
            out.push(MouseEvent { kind: "button", action: "down", code, name, delta: 0 });
        }
        if buttons & up != 0 {
            out.push(MouseEvent { kind: "button", action: "up", code, name, delta: 0 });
        }
    }
    let delta = data as i16 as i32;
    if buttons & RI_MOUSE_WHEEL != 0 && delta != 0 {
        let (code, name) = if delta > 0 { ("WheelUp", "Wheel up") } else { ("WheelDown", "Wheel down") };
        out.push(MouseEvent { kind: "wheel", action: "turn", code, name, delta });
    }
    if buttons & RI_MOUSE_HWHEEL != 0 && delta != 0 {
        let (code, name) = if delta > 0 { ("WheelRight", "Wheel right") } else { ("WheelLeft", "Wheel left") };
        out.push(MouseEvent { kind: "wheel", action: "turn", code, name, delta });
    }
    out
}

/// A readable name for a HID usage (consumer controls, system controls, buttons).
pub fn usage_name(page: u16, usage: u16) -> String {
    let known = match (page, usage) {
        (0x0C, 0x30) => "Power",
        (0x0C, 0x40) => "Menu",
        (0x0C, 0x6F) => "Brightness Up",
        (0x0C, 0x70) => "Brightness Down",
        (0x0C, 0xB0) => "Play",
        (0x0C, 0xB1) => "Pause",
        (0x0C, 0xB2) => "Record",
        (0x0C, 0xB3) => "Fast Forward",
        (0x0C, 0xB4) => "Rewind",
        (0x0C, 0xB5) => "Next Track",
        (0x0C, 0xB6) => "Previous Track",
        (0x0C, 0xB7) => "Stop",
        (0x0C, 0xB8) => "Eject",
        (0x0C, 0xCD) => "Play / Pause",
        (0x0C, 0xCF) => "Voice Command",
        (0x0C, 0xE2) => "Mute",
        (0x0C, 0xE9) => "Volume Up",
        (0x0C, 0xEA) => "Volume Down",
        (0x0C, 0x183) => "Media Player",
        (0x0C, 0x18A) => "Mail",
        (0x0C, 0x192) => "Calculator",
        (0x0C, 0x194) => "My Computer",
        (0x0C, 0x221) => "Search",
        (0x0C, 0x223) => "Browser Home",
        (0x0C, 0x224) => "Browser Back",
        (0x0C, 0x225) => "Browser Forward",
        (0x0C, 0x226) => "Browser Stop",
        (0x0C, 0x227) => "Browser Refresh",
        (0x0C, 0x22A) => "Favourites",
        (0x01, 0x81) => "System Power Down",
        (0x01, 0x82) => "System Sleep",
        (0x01, 0x83) => "System Wake Up",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    if page == 0x09 {
        return format!("Button {usage}");
    }
    format!("Usage {page:02X}:{usage:04X}")
}

// ---------- devices ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    /// keyboard, mouse or hid
    pub kind: String,
    pub name: String,
    /// What this part of the device does: "Keyboard", "Mouse", "Media keys", "Game controller", ...
    pub role: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub simulated: bool,
}

/// VID and PID from a device path such as `\\?\HID#VID_FFF1&PID_00DD&MI_00#...`.
pub fn vid_pid(path: &str) -> Option<(u16, u16)> {
    let up = path.to_ascii_uppercase();
    let hex = |tag: &str| {
        let i = up.find(tag)? + tag.len();
        u16::from_str_radix(up.get(i..i + 4)?, 16).ok()
    };
    Some((hex("VID_")?, hex("PID_")?))
}

/// Driver or bus names that do not say which product it is ("HID Keyboard Device", "Bluetooth LE Service {...}").
pub fn generic_name(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.starts_with("hid ")
        || l.starts_with("hid-compliant")
        || l.starts_with("bluetooth le service")
        || l.starts_with("bluetooth le device")
        || l.starts_with("generic bluetooth")
        || l.contains("usb input device")
        || l.contains("usb composite device")
        || l.contains("root hub")
        || l.contains("generic usb hub")
        || l.starts_with("{")
}

/// The role shown for a HID top-level collection.
pub fn hid_role(page: u16, usage: u16) -> &'static str {
    match (page, usage) {
        (0x0C, _) => "Media keys",
        (0x01, 0x80) => "System keys",
        (0x01, 0x04) => "Joystick",
        (0x01, 0x05) => "Game controller",
        _ => "HID controls",
    }
}

/// Fake devices for test instances (and the shape of the real list).
pub fn fake_devices() -> Vec<Device> {
    let d = |id: &str, kind: &str, name: &str, role: &str, ids: Option<(u16, u16)>| Device {
        id: id.into(),
        kind: kind.into(),
        name: name.into(),
        role: role.into(),
        vid: ids.map(|i| i.0),
        pid: ids.map(|i| i.1),
        simulated: true,
    };
    vec![
        d("fake-keyboard", "keyboard", "Keyboard (simulated)", "Keyboard", Some((0x046D, 0xC33F))),
        d("fake-mouse", "mouse", "Mouse (simulated)", "Mouse", Some((0x046D, 0xC08B))),
        d("fake-au05", "keyboard", "AU05 (simulated)", "Keyboard", Some((0xFFF1, 0x00DD))),
        d("fake-au05-media", "hid", "AU05 (simulated)", "Media keys", Some((0xFFF1, 0x00DD))),
    ]
}

// ---------- the tester ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub seq: u64,
    /// Milliseconds since the listener started.
    pub t: f64,
    pub device: String,
    /// key, button, wheel, move, hid, device
    pub kind: &'static str,
    /// down, up, repeat, turn, move, report, added, removed
    pub action: &'static str,
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vk: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<bool>,
    pub mods: Vec<&'static str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub claimed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dx: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dy: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub down: u64,
    pub up: u64,
    pub repeat: u64,
    pub buttons: u64,
    pub wheel: u64,
    pub moves: u64,
    pub hid: u64,
    /// ms (listener clock) of the last event; activity dots light from this.
    pub last: Option<f64>,
}

struct Session {
    started: Instant,
    seq: u64,
    events: VecDeque<Event>,
    dropped: u64,
    counts: BTreeMap<String, Counts>,
    /// (device, code) of keys and buttons that are down.
    held: HashSet<(String, String)>,
    hid_on: HashMap<String, Vec<(u16, u16)>>,
    hid_last: HashMap<String, (Vec<u8>, Instant)>,
    devices: Vec<Device>,
    log_moves: bool,
    claimed: Vec<u16>,
    last_poll: Instant,
    mode: &'static str,
    error: Option<String>,
}

impl Session {
    fn new(mode: &'static str, devices: Vec<Device>) -> Self {
        let now = Instant::now();
        Session {
            started: now,
            seq: 0,
            events: VecDeque::new(),
            dropped: 0,
            counts: BTreeMap::new(),
            held: HashSet::new(),
            hid_on: HashMap::new(),
            hid_last: HashMap::new(),
            devices,
            log_moves: false,
            claimed: Vec::new(),
            last_poll: now,
            mode,
            error: None,
        }
    }
    fn now(&self) -> f64 {
        (self.started.elapsed().as_micros() as f64 / 100.0).round() / 10.0
    }
    fn mods(&self) -> Vec<&'static str> {
        let held = |codes: &[&str]| self.held.iter().any(|(_, c)| codes.contains(&c.as_str()));
        let mut m = Vec::new();
        if held(&["ControlLeft", "ControlRight"]) {
            m.push("Ctrl");
        }
        if held(&["ShiftLeft", "ShiftRight"]) {
            m.push("Shift");
        }
        if held(&["AltLeft", "AltRight"]) {
            m.push("Alt");
        }
        if held(&["MetaLeft", "MetaRight"]) {
            m.push("Win");
        }
        m
    }
    fn push(&mut self, mut e: Event) {
        self.seq += 1;
        e.seq = self.seq;
        self.events.push_back(e);
        while self.events.len() > KEEP {
            self.events.pop_front();
            self.dropped += 1;
        }
    }
    fn blank(&self, device: &str, kind: &'static str, action: &'static str, code: String, name: String) -> Event {
        Event {
            seq: 0,
            t: self.now(),
            device: device.to_string(),
            kind,
            action,
            code,
            name,
            vk: None,
            scan: None,
            ext: None,
            mods: self.mods(),
            claimed: false,
            delta: None,
            dx: None,
            dy: None,
            bytes: None,
        }
    }

    pub fn ingest(&mut self, r: &Record) {
        let device = r.device().to_string();
        let t = self.now();
        match r {
            Record::Key { make, flags, vk, .. } => {
                let Some(k) = decode_key(*make, *flags, *vk) else { return };
                let id = (device.clone(), k.code.clone());
                let action = if k.up {
                    self.held.remove(&id);
                    "up"
                } else if self.held.contains(&id) {
                    "repeat"
                } else {
                    "down"
                };
                // Modifiers in the event are the ones held before this key (a combination's own modifiers).
                let mut e = self.blank(&device, "key", action, k.code.clone(), k.name.clone());
                if !k.up {
                    self.held.insert(id);
                }
                e.vk = Some(k.vk);
                e.scan = Some(k.scan);
                e.ext = Some(k.ext);
                // A hotkey UC holds (RegisterHotKey with no modifiers) swallows the key press, not the release.
                e.claimed = !k.up && self.claimed.contains(&k.vk) && e.mods.is_empty();
                let c = self.counts.entry(device).or_default();
                match action {
                    "up" => c.up += 1,
                    "repeat" => c.repeat += 1,
                    _ => c.down += 1,
                }
                c.last = Some(t);
                self.push(e);
            }
            Record::Mouse { buttons, data, dx, dy, .. } => {
                for m in decode_mouse(*buttons, *data) {
                    let id = (device.clone(), m.code.to_string());
                    if m.kind == "button" {
                        if m.action == "up" {
                            self.held.remove(&id);
                        } else {
                            self.held.insert(id);
                        }
                    }
                    let mut e = self.blank(&device, m.kind, m.action, m.code.into(), m.name.into());
                    if m.kind == "wheel" {
                        e.delta = Some(m.delta);
                    }
                    let c = self.counts.entry(device.clone()).or_default();
                    if m.kind == "wheel" {
                        c.wheel += 1;
                    } else if m.action == "down" {
                        c.buttons += 1; // a click counts once
                    }
                    c.last = Some(t);
                    self.push(e);
                }
                if *dx != 0 || *dy != 0 {
                    let c = self.counts.entry(device.clone()).or_default();
                    c.moves += 1;
                    c.last = Some(t);
                    if self.log_moves {
                        let mut e = self.blank(&device, "move", "move", "MouseMove".into(), "Move".into());
                        e.dx = Some(*dx);
                        e.dy = Some(*dy);
                        self.push(e);
                    }
                }
            }
            Record::Hid { usages, bytes, .. } => {
                let before = self.hid_on.get(&device).cloned().unwrap_or_default();
                let mut changed = false;
                for u in usages.iter().filter(|u| !before.contains(u)) {
                    let mut e = self.blank(&device, "hid", "down", format!("Hid{:02X}:{:04X}", u.0, u.1), usage_name(u.0, u.1));
                    e.bytes = Some(hex(bytes));
                    self.push(e);
                    changed = true;
                }
                for u in before.iter().filter(|u| !usages.contains(u)) {
                    let e = self.blank(&device, "hid", "up", format!("Hid{:02X}:{:04X}", u.0, u.1), usage_name(u.0, u.1));
                    self.push(e);
                    changed = true;
                }
                self.hid_on.insert(device.clone(), usages.clone());
                // Reports with no button change (sticks, sliders): logged when the bytes change, at most every 100 ms.
                if !changed {
                    let now = Instant::now();
                    let new_bytes = self.hid_last.get(&device).is_none_or(|(b, at)| b != bytes && now - *at >= HID_REPORT_GAP);
                    if new_bytes && !bytes.is_empty() {
                        let mut e = self.blank(&device, "hid", "report", "HidReport".into(), "Report".into());
                        e.bytes = Some(hex(bytes));
                        self.push(e);
                        self.hid_last.insert(device.clone(), (bytes.clone(), now));
                    }
                } else {
                    self.hid_last.insert(device.clone(), (bytes.clone(), Instant::now()));
                }
                let c = self.counts.entry(device).or_default();
                c.hid += 1;
                c.last = Some(t);
            }
        }
    }
    fn device_change(&mut self, devices: Vec<Device>) {
        let old: HashSet<String> = self.devices.iter().map(|d| d.id.clone()).collect();
        let new: HashSet<String> = devices.iter().map(|d| d.id.clone()).collect();
        for d in devices.iter().filter(|d| !old.contains(&d.id)) {
            let e = self.blank(&d.id, "device", "added", "Device".into(), format!("{} connected", d.name));
            self.push(e);
        }
        for d in self.devices.clone().iter().filter(|d| !new.contains(&d.id)) {
            let e = self.blank(&d.id, "device", "removed", "Device".into(), format!("{} disconnected", d.name));
            self.push(e);
            self.held.retain(|(dev, _)| dev != &d.id);
        }
        // Keep removed devices in the list (their events stay readable), marked by the page as gone.
        let mut all = devices;
        for d in &self.devices {
            if !new.contains(&d.id) && self.counts.contains_key(&d.id) {
                all.push(d.clone());
            }
        }
        self.devices = all;
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().take(64).map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// The listener: at most one session; the real Windows layer feeds it from its own thread.
#[derive(Default)]
pub struct Tester {
    session: Mutex<Option<Session>>,
    #[cfg(windows)]
    native: Mutex<Option<win::Listener>>,
}

pub fn global() -> &'static Arc<Tester> {
    static T: OnceLock<Arc<Tester>> = OnceLock::new();
    T.get_or_init(|| Arc::new(Tester::default()))
}

/// Test instances never open Raw Input.
pub fn simulated() -> bool {
    let quiet = std::env::var("UC_TEST_QUIET").is_ok_and(|v| !v.is_empty() && v != "0");
    quiet || super::buildinfo::fake_device().is_some() || cfg!(test)
}

impl Tester {
    /// Start listening (idempotent). `simulated`: fake devices, no Raw Input.
    pub fn start(self: &Arc<Self>, simulated: bool) -> Value {
        // One start at a time: two pages (or a quick reload) never make two listeners.
        #[cfg(windows)]
        let mut native = self.native.lock_or_recover();
        {
            let mut s = self.session.lock_or_recover();
            if let Some(s) = s.as_mut() {
                s.last_poll = Instant::now();
                return self.view(s, 0);
            }
            if simulated {
                *s = Some(Session::new("simulated", fake_devices()));
            } else {
                *s = Some(Session::new("raw", Vec::new()));
            }
        }
        #[cfg(windows)]
        if !simulated {
            match win::Listener::start(self.clone()) {
                Ok(l) => *native = Some(l),
                Err(e) => {
                    crate::device::log(&format!("input tester: raw input unavailable: {e}"));
                    if let Some(s) = self.session.lock_or_recover().as_mut() {
                        s.error = Some(e);
                    }
                }
            }
        }
        let s = self.session.lock_or_recover();
        self.view(s.as_ref().expect("started"), 0)
    }
    pub fn stop(&self) {
        #[cfg(windows)]
        if let Some(l) = self.native.lock_or_recover().take() {
            l.stop();
        }
        *self.session.lock_or_recover() = None;
    }
    pub fn running(&self) -> bool {
        self.session.lock_or_recover().is_some()
    }
    /// New events after `since`, the devices and their counts. Keeps the listener alive.
    pub fn poll(&self, since: u64, claimed: Vec<u16>) -> Value {
        let mut s = self.session.lock_or_recover();
        match s.as_mut() {
            None => json!({"running": false}),
            Some(s) => {
                s.last_poll = Instant::now();
                s.claimed = claimed;
                self.view(s, since)
            }
        }
    }
    pub fn configure(&self, patch: &Value) -> Value {
        let mut s = self.session.lock_or_recover();
        match s.as_mut() {
            None => json!({"running": false}),
            Some(s) => {
                if let Some(m) = patch["logMoves"].as_bool() {
                    s.log_moves = m;
                }
                if patch["clear"] == true {
                    s.counts.clear();
                    s.held.clear();
                    s.hid_on.clear();
                }
                self.view(s, s.seq)
            }
        }
    }
    pub fn ingest(&self, r: &Record) {
        if let Some(s) = self.session.lock_or_recover().as_mut() {
            s.ingest(r);
        }
    }
    pub fn set_devices(&self, devices: Vec<Device>) {
        if let Some(s) = self.session.lock_or_recover().as_mut() {
            if s.devices.is_empty() {
                s.devices = devices;
            } else {
                s.device_change(devices);
            }
        }
    }
    /// The page went away without saying so: stop after `IDLE_STOP` without a poll.
    pub fn idle(&self) -> bool {
        self.session.lock_or_recover().as_ref().is_none_or(|s| s.last_poll.elapsed() >= IDLE_STOP)
    }
    fn view(&self, s: &Session, since: u64) -> Value {
        let events: Vec<&Event> = s.events.iter().filter(|e| e.seq > since).collect();
        json!({
            "running": true,
            "mode": s.mode,
            "error": s.error,
            "now": s.now(),
            "seq": s.seq,
            "dropped": s.dropped,
            "logMoves": s.log_moves,
            "claimed": s.claimed,
            "devices": s.devices,
            "counts": s.counts,
            "events": events,
        })
    }
}

// ---------- audio save ----------

/// Decode standard base64 (the page sends a WAV recording this way).
pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' => continue,
            _ => return Err("The recording could not be read".into()),
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// Where a saved recording goes: the Downloads folder; test instances use their own app folder.
fn save_dir() -> std::path::PathBuf {
    if simulated() {
        return super::paths::base().join("input-tester");
    }
    dirs::download_dir().unwrap_or_else(|| super::paths::base().join("input-tester"))
}

pub fn save_wav(b64: &str, stamp: &str) -> CoreResult<String> {
    let bytes = base64_decode(b64).map_err(CoreError::new)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(CoreError::new("The recording is not a WAV file"));
    }
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(CoreError::new("The recording is too large"));
    }
    let stamp: String = stamp.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(32).collect();
    let dir = save_dir();
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::new(format!("The folder could not be made: {e}")))?;
    let mut path = dir.join(format!("Microphone test {stamp}.wav"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("Microphone test {stamp} ({n}).wav"));
        n += 1;
    }
    std::fs::write(&path, bytes).map_err(|e| CoreError::new(format!("The recording could not be saved: {e}")))?;
    Ok(path.to_string_lossy().to_string())
}

// ---------- Tauri commands ----------

fn claimed_hotkeys(core: &State<'_, Core>) -> Vec<u16> {
    core.0.au05.get().map(|a| a.claimed_hotkeys()).unwrap_or_default()
}

/// Listening is only for a page the user can see: never while the window is hidden (closed to the tray) or minimised.
fn hidden(window: &tauri::WebviewWindow) -> bool {
    !window.is_visible().unwrap_or(true) || window.is_minimized().unwrap_or(false)
}

fn paused() -> Value {
    json!({"running": false, "hidden": true})
}

/// `fresh`: the page was just opened; a session left from an earlier visit is replaced.
#[tauri::command]
pub async fn input_tester_start(core: State<'_, Core>, window: tauri::WebviewWindow, fresh: Option<bool>) -> Result<Value, CoreError> {
    let claimed = claimed_hotkeys(&core);
    let hide = hidden(&window);
    tauri::async_runtime::spawn_blocking(move || {
        let t = global();
        if hide || fresh == Some(true) {
            t.stop();
        }
        if hide {
            return paused();
        }
        t.start(simulated());
        t.poll(0, claimed)
    })
    .await
    .map_err(|e| CoreError::new(e.to_string()))
}

#[tauri::command]
pub async fn input_tester_poll(core: State<'_, Core>, window: tauri::WebviewWindow, since: u64) -> Result<Value, CoreError> {
    if hidden(&window) {
        tauri::async_runtime::spawn_blocking(|| global().stop()).await.map_err(|e| CoreError::new(e.to_string()))?;
        return Ok(paused());
    }
    Ok(global().poll(since, claimed_hotkeys(&core)))
}

#[tauri::command]
pub async fn input_tester_configure(patch: Value) -> Result<Value, CoreError> {
    Ok(global().configure(&patch))
}

#[tauri::command]
pub async fn input_tester_stop() -> Result<Value, CoreError> {
    tauri::async_runtime::spawn_blocking(|| global().stop())
        .await
        .map_err(|e| CoreError::new(e.to_string()))?;
    Ok(json!({"running": false}))
}

/// Debug fake instances only: feed fake raw records through the real decoder.
#[tauri::command]
pub async fn input_tester_test_inject(records: Vec<Value>) -> Result<Value, CoreError> {
    if !cfg!(debug_assertions) || super::buildinfo::fake_device().is_none() {
        return Err(CoreError::coded("test_only", "Input tester test records require a debug fake instance"));
    }
    let t = global();
    for r in &records {
        t.ingest(&Record::from_json(r).map_err(CoreError::new)?);
    }
    Ok(json!({"ok": true, "running": t.running()}))
}

#[tauri::command]
pub async fn input_tester_save_audio(wav: String, stamp: String) -> Result<Value, CoreError> {
    let path = tauri::async_runtime::spawn_blocking(move || save_wav(&wav, &stamp))
        .await
        .map_err(|e| CoreError::new(e.to_string()))??;
    Ok(json!({"path": path}))
}

// ---------- Windows Raw Input ----------

#[cfg(windows)]
mod win {
    use super::{hid_role, vid_pid, Device, Record, Tester};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::{mpsc, Arc};
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_PropertyW, CM_Get_Device_Interface_PropertyW, CM_Get_Parent, CM_Locate_DevNodeW, CR_SUCCESS,
    };
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HidP_GetUsagesEx, HidP_Input, HidP_MaxUsageListLength, HIDP_STATUS_SUCCESS, USAGE_AND_PAGE,
    };
    use windows_sys::Win32::Devices::Properties::{
        DEVPKEY_Device_BusReportedDeviceDesc, DEVPKEY_Device_DeviceDesc, DEVPKEY_Device_FriendlyName,
        DEVPKEY_Device_InstanceId, DEVPROP_TYPE_STRING,
    };
    use windows_sys::Win32::Foundation::{DEVPROPKEY, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::{
        GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, RegisterRawInputDevices, RAWINPUT,
        RAWINPUTDEVICE, RAWINPUTDEVICELIST, RAWINPUTHEADER, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_REMOVE,
        RIDI_DEVICEINFO, RIDI_DEVICENAME, RIDI_PREPARSEDDATA, RID_DEVICE_INFO, RID_INPUT, RIM_TYPEHID,
        RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer, PostQuitMessage,
        PostThreadMessageW, RegisterClassW, SetTimer, TranslateMessage, MSG, WM_APP, WM_INPUT, WM_INPUT_DEVICE_CHANGE,
        WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    /// The usages listened to: keyboards, mice, media keys, system keys, joysticks and game controllers.
    const USAGES: [(u16, u16); 6] = [(0x01, 0x06), (0x01, 0x02), (0x0C, 0x01), (0x01, 0x80), (0x01, 0x04), (0x01, 0x05)];
    const WM_STOP: u32 = WM_APP + 41;

    thread_local! {
        static SINK: RefCell<Option<Arc<Tester>>> = const { RefCell::new(None) };
        static HID: RefCell<HashMap<isize, Vec<u8>>> = RefCell::new(HashMap::new());
    }

    pub struct Listener {
        thread: u32,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl Listener {
        pub fn start(tester: Arc<Tester>) -> Result<Self, String> {
            let (tx, rx) = mpsc::channel::<Result<u32, String>>();
            let join = std::thread::Builder::new()
                .name("uc-input-tester".into())
                .spawn(move || run(tester, tx))
                .map_err(|e| e.to_string())?;
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(Ok(thread)) => Ok(Listener { thread, join: Some(join) }),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("The input listener did not start".into()),
            }
        }
        pub fn stop(mut self) {
            unsafe { PostThreadMessageW(self.thread, WM_STOP, 0, 0) };
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn register(hwnd: HWND, on: bool) -> bool {
        let list: Vec<RAWINPUTDEVICE> = USAGES
            .iter()
            .map(|&(page, usage)| RAWINPUTDEVICE {
                usUsagePage: page,
                usUsage: usage,
                dwFlags: if on { RIDEV_INPUTSINK | RIDEV_DEVNOTIFY } else { RIDEV_REMOVE },
                hwndTarget: if on { hwnd } else { std::ptr::null_mut() },
            })
            .collect();
        unsafe { RegisterRawInputDevices(list.as_ptr(), list.len() as u32, std::mem::size_of::<RAWINPUTDEVICE>() as u32) != 0 }
    }

    fn run(tester: Arc<Tester>, ready: mpsc::Sender<Result<u32, String>>) {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = wide("UnifiedControlInputTester");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc_),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassW(&wc); // fails harmlessly when already registered by an earlier session
            // A hidden pop-up window: never shown, never activated, not in the taskbar.
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                let _ = ready.send(Err("The input listener window could not be made".into()));
                return;
            }
            SINK.with(|s| *s.borrow_mut() = Some(tester.clone()));
            tester.set_devices(devices());
            if !register(hwnd, true) {
                DestroyWindow(hwnd);
                let _ = ready.send(Err("Windows refused Raw Input registration".into()));
                return;
            }
            crate::device::log("input tester: raw input on");
            SetTimer(hwnd, 1, 1000, None);
            let _ = ready.send(Ok(GetCurrentThreadId()));
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_STOP {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            KillTimer(hwnd, 1);
            register(hwnd, false);
            DestroyWindow(hwnd);
            SINK.with(|s| *s.borrow_mut() = None);
            HID.with(|h| h.borrow_mut().clear());
            crate::device::log("input tester: raw input off");
        }
    }

    unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_INPUT => {
                super::super::sync::guarded("input tester event", || unsafe { on_input(lparam) });
            }
            WM_INPUT_DEVICE_CHANGE => {
                super::super::sync::guarded("input tester device change", || {
                    HID.with(|h| h.borrow_mut().remove(&(lparam as isize)));
                    SINK.with(|s| {
                        if let Some(t) = s.borrow().as_ref() {
                            t.set_devices(devices());
                        }
                    });
                });
                return 0;
            }
            WM_TIMER => {
                // The page stopped polling (window closed or reloaded): stop listening.
                let idle = SINK.with(|s| s.borrow().as_ref().is_none_or(|t| t.idle()));
                if idle {
                    unsafe { PostQuitMessage(0) };
                    SINK.with(|s| {
                        if let Some(t) = s.borrow().as_ref() {
                            let t = t.clone();
                            // Off this thread: stop() joins it.
                            std::thread::spawn(move || t.stop());
                        }
                    });
                }
                return 0;
            }
            _ => {}
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn id(h: HANDLE) -> String {
        if h.is_null() {
            "injected".into()
        } else {
            format!("h{:x}", h as usize)
        }
    }

    unsafe fn on_input(lparam: LPARAM) {
        let mut size = 0u32;
        let header = std::mem::size_of::<RAWINPUTHEADER>() as u32;
        unsafe { GetRawInputData(lparam as _, RID_INPUT, std::ptr::null_mut(), &mut size, header) };
        if size == 0 || size > 64 * 1024 {
            return;
        }
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        if unsafe { GetRawInputData(lparam as _, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, header) } != size {
            return;
        }
        let raw = unsafe { &*(buf.as_ptr() as *const RAWINPUT) };
        let device = id(raw.header.hDevice);
        let record = match raw.header.dwType {
            RIM_TYPEKEYBOARD => {
                let k = unsafe { raw.data.keyboard };
                Record::Key { device, make: k.MakeCode, flags: k.Flags, vk: k.VKey }
            }
            RIM_TYPEMOUSE => {
                let m = unsafe { raw.data.mouse };
                let b = unsafe { m.Anonymous.Anonymous };
                // Absolute positions (tablets, remote desktop) are not movement deltas.
                let abs = m.usFlags & 1 != 0;
                Record::Mouse {
                    device,
                    buttons: b.usButtonFlags,
                    data: b.usButtonData,
                    dx: if abs { 0 } else { m.lLastX },
                    dy: if abs { 0 } else { m.lLastY },
                }
            }
            RIM_TYPEHID => {
                let hid = unsafe { &raw.data.hid };
                let (each, count) = (hid.dwSizeHid as usize, hid.dwCount as usize);
                let start = (&hid.bRawData as *const u8 as usize) - (buf.as_ptr() as usize);
                let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), size as usize) };
                let Some(all) = bytes.get(start..start + each * count) else { return };
                // One record per report; most devices send one at a time.
                for report in all.chunks(each.max(1)) {
                    let usages = unsafe { usages(raw.header.hDevice, report) };
                    let r = Record::Hid { device: device.clone(), usages, bytes: report.to_vec() };
                    SINK.with(|s| {
                        if let Some(t) = s.borrow().as_ref() {
                            t.ingest(&r)
                        }
                    });
                }
                return;
            }
            _ => return,
        };
        SINK.with(|s| {
            if let Some(t) = s.borrow().as_ref() {
                t.ingest(&record)
            }
        });
    }

    /// The button usages that are on in a HID input report (media keys, system keys, controller buttons).
    unsafe fn usages(h: HANDLE, report: &[u8]) -> Vec<(u16, u16)> {
        HID.with(|cache| {
            let mut cache = cache.borrow_mut();
            let data = cache.entry(h as isize).or_insert_with(|| unsafe {
                let mut n = 0u32;
                GetRawInputDeviceInfoW(h, RIDI_PREPARSEDDATA, std::ptr::null_mut(), &mut n);
                let mut d = vec![0u8; n as usize];
                if n == 0 || GetRawInputDeviceInfoW(h, RIDI_PREPARSEDDATA, d.as_mut_ptr().cast(), &mut n) == u32::MAX {
                    d.clear();
                }
                d
            });
            if data.is_empty() {
                return Vec::new();
            }
            let pp = data.as_ptr() as isize;
            let max = unsafe { HidP_MaxUsageListLength(HidP_Input, 0, pp) }.clamp(1, 256);
            let mut list = vec![USAGE_AND_PAGE { Usage: 0, UsagePage: 0 }; max as usize];
            let mut len = max;
            let st = unsafe {
                HidP_GetUsagesEx(HidP_Input, 0, list.as_mut_ptr(), &mut len, pp, report.as_ptr(), report.len() as u32)
            };
            if st != HIDP_STATUS_SUCCESS {
                return Vec::new();
            }
            list.truncate(len as usize);
            list.into_iter().filter(|u| u.Usage != 0).map(|u| (u.UsagePage, u.Usage)).collect()
        })
    }

    fn string_prop(get: impl Fn(*mut u32, *mut u8, *mut u32) -> u32) -> Option<String> {
        let mut buf = vec![0u16; 512];
        let mut len = (buf.len() * 2) as u32;
        let mut kind = 0u32;
        if get(&mut kind, buf.as_mut_ptr().cast(), &mut len) != CR_SUCCESS || kind != DEVPROP_TYPE_STRING {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    fn node_prop(node: u32, key: &DEVPROPKEY) -> Option<String> {
        string_prop(|k, b, l| unsafe { CM_Get_DevNode_PropertyW(node, key, k, b, l, 0) })
    }

    /// The product name: the USB product string when Windows has it, else the device description.
    fn product_name(path: &str) -> Option<String> {
        let p = wide(path);
        let instance =
            string_prop(|k, b, l| unsafe { CM_Get_Device_Interface_PropertyW(p.as_ptr(), &DEVPKEY_Device_InstanceId, k, b, l, 0) })?;
        let iw = wide(&instance);
        let mut node = 0u32;
        if unsafe { CM_Locate_DevNodeW(&mut node, iw.as_ptr(), 0) } != CR_SUCCESS {
            return None;
        }
        // Walk up from the HID collection: USB interface, then USB device (product string); for Bluetooth, the
        // service, then the device (its friendly name). Generic driver names are skipped.
        let mut cur = node;
        for _ in 0..3 {
            for key in [&DEVPKEY_Device_BusReportedDeviceDesc, &DEVPKEY_Device_FriendlyName] {
                if let Some(n) = node_prop(cur, key).filter(|s| !super::generic_name(s)) {
                    return Some(n);
                }
            }
            let mut parent = 0u32;
            if unsafe { CM_Get_Parent(&mut parent, cur, 0) } != CR_SUCCESS {
                break;
            }
            cur = parent;
        }
        node_prop(node, &DEVPKEY_Device_FriendlyName).or_else(|| node_prop(node, &DEVPKEY_Device_DeviceDesc))
    }

    fn device_path(h: HANDLE) -> String {
        let mut n = 0u32;
        unsafe { GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, std::ptr::null_mut(), &mut n) };
        let mut buf = vec![0u16; n as usize + 1];
        if n == 0 || unsafe { GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, buf.as_mut_ptr().cast(), &mut n) } == u32::MAX {
            return String::new();
        }
        String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string()
    }

    /// Every keyboard, mouse and listened-to HID collection Windows reports now.
    pub fn devices() -> Vec<Device> {
        let mut n = 0u32;
        let size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
        unsafe { GetRawInputDeviceList(std::ptr::null_mut(), &mut n, size) };
        let mut list = vec![RAWINPUTDEVICELIST { hDevice: std::ptr::null_mut(), dwType: 0 }; n as usize + 8];
        let mut cap = list.len() as u32;
        let got = unsafe { GetRawInputDeviceList(list.as_mut_ptr(), &mut cap, size) };
        if got == u32::MAX {
            return Vec::new();
        }
        list.truncate(got as usize);
        let mut out = Vec::new();
        for d in list {
            let mut info: RID_DEVICE_INFO = unsafe { std::mem::zeroed() };
            info.cbSize = std::mem::size_of::<RID_DEVICE_INFO>() as u32;
            let mut sz = info.cbSize;
            if unsafe { GetRawInputDeviceInfoW(d.hDevice, RIDI_DEVICEINFO, (&mut info as *mut RID_DEVICE_INFO).cast(), &mut sz) } == u32::MAX {
                continue;
            }
            let path = device_path(d.hDevice);
            let ids = vid_pid(&path);
            let (kind, role, ids) = match d.dwType {
                RIM_TYPEKEYBOARD => ("keyboard", "Keyboard", ids),
                RIM_TYPEMOUSE => ("mouse", "Mouse", ids),
                RIM_TYPEHID => {
                    let h = unsafe { info.Anonymous.hid };
                    if !USAGES.contains(&(h.usUsagePage, h.usUsage)) {
                        continue;
                    }
                    ("hid", hid_role(h.usUsagePage, h.usUsage), Some((h.dwVendorId as u16, h.dwProductId as u16)))
                }
                _ => continue,
            };
            let lower = path.to_ascii_lowercase();
            let name = product_name(&path).unwrap_or_else(|| {
                if lower.contains("rdp_") || lower.contains("termserv") {
                    "Remote Desktop".into()
                } else if lower.contains("acpi") {
                    "Built-in keyboard".into()
                } else {
                    role.into()
                }
            });
            out.push(Device {
                id: id(d.hDevice),
                kind: kind.into(),
                name,
                role: role.into(),
                vid: ids.map(|i| i.0),
                pid: ids.map(|i| i.1),
                simulated: false,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(device: &str, make: u16, flags: u16, vk: u16) -> Record {
        Record::Key { device: device.into(), make, flags, vk }
    }

    #[test]
    fn letters_digits_and_function_keys_decode_by_position_and_name() {
        let a = decode_key(0x1E, 0, 0x41).unwrap();
        assert_eq!((a.code.as_str(), a.name.as_str(), a.up, a.ext), ("KeyA", "A", false, false));
        assert!(decode_key(0x1E, RI_KEY_BREAK, 0x41).unwrap().up);
        assert_eq!(decode_key(0x0B, 0, 0x30).unwrap().code, "Digit0");
        // The Vibe Key types F19–F24.
        for (scan, vk, f) in [(0x6A, 0x82, "F19"), (0x6B, 0x83, "F20"), (0x6C, 0x84, "F21"), (0x6D, 0x85, "F22"), (0x6E, 0x86, "F23"), (0x76, 0x87, "F24")] {
            let k = decode_key(scan, 0, vk).unwrap();
            assert_eq!((k.code.as_str(), k.name.as_str()), (f, f));
        }
        // Software input often has no scan code: the virtual key gives the position.
        assert_eq!(decode_key(0, 0, 0x85).unwrap().code, "F22");
        // A layout that moves letters: the position stays, the name follows the layout (AZERTY A on the Q key).
        let q = decode_key(0x10, 0, 0x41).unwrap();
        assert_eq!((q.code.as_str(), q.name.as_str()), ("KeyQ", "A"));
    }

    #[test]
    fn modifier_sides_extended_keys_and_sequences() {
        assert_eq!(decode_key(0x2A, 0, 0x10).unwrap().code, "ShiftLeft");
        assert_eq!(decode_key(0x36, 0, 0x10).unwrap().name, "Right Shift");
        assert_eq!(decode_key(0x1D, 0, 0x11).unwrap().vk, 0xA2);
        let rc = decode_key(0x1D, RI_KEY_E0, 0x11).unwrap();
        assert_eq!((rc.code.as_str(), rc.vk, rc.ext), ("ControlRight", 0xA3, true));
        assert_eq!(decode_key(0x38, RI_KEY_E0, 0x12).unwrap().code, "AltRight");
        assert_eq!(decode_key(0x1C, RI_KEY_E0, 0x0D).unwrap().name, "Numpad Enter");
        assert_eq!(decode_key(0x48, RI_KEY_E0, 0x26).unwrap().code, "ArrowUp");
        // Num Lock off: the number pad 8 sends Up arrow's virtual key without the extended prefix.
        let n8 = decode_key(0x48, 0, 0x26).unwrap();
        assert_eq!((n8.code.as_str(), n8.name.as_str()), ("Numpad8", "Up"));
        // Fake Shift around Print Screen, and fillers, are not keys.
        assert_eq!(decode_key(0x2A, RI_KEY_E0, 0x10), None);
        assert_eq!(decode_key(0x45, 0, 0xFF), None);
        assert_eq!(decode_key(0x1D, RI_KEY_E1, 0x13).unwrap().code, "Pause");
        assert_eq!(decode_key(0x37, RI_KEY_E0, 0x2C).unwrap().code, "PrintScreen");
        assert_eq!(decode_key(0x56, 0, 0xE2).unwrap().code, "IntlBackslash");
        // Unknown keys keep their numbers.
        assert_eq!(decode_key(0x7F, 0, 0x00).unwrap().code, "Scan7F");
    }

    #[test]
    fn mouse_buttons_and_wheels() {
        let e = decode_mouse(0x0001 | 0x0200, 0);
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].code, e[0].action), ("MouseLeft", "down"));
        assert_eq!((e[1].code, e[1].action), ("MouseForward", "up"));
        let w = decode_mouse(RI_MOUSE_WHEEL, (-120i16) as u16);
        assert_eq!((w[0].code, w[0].delta), ("WheelDown", -120));
        assert_eq!(decode_mouse(RI_MOUSE_HWHEEL, 120)[0].code, "WheelRight");
        assert!(decode_mouse(0, 0).is_empty());
    }

    #[test]
    fn paths_give_vid_and_pid() {
        assert_eq!(vid_pid(r"\\?\HID#VID_FFF1&PID_00DD&MI_00#7&2a&0&0000#{884b96c3}"), Some((0xFFF1, 0x00DD)));
        assert_eq!(vid_pid(r"\\?\ACPI#PNP0303#4&1d401fb5&0#{884b96c3}"), None);
        assert_eq!(usage_name(0x0C, 0xE9), "Volume Up");
        assert_eq!(usage_name(0x09, 3), "Button 3");
        assert_eq!(usage_name(0xFF00, 1), "Usage FF00:0001");
        for g in ["HID Keyboard Device", "HID-compliant consumer control device", "Bluetooth LE Service {00001812-0000}", "USB Input Device"] {
            assert!(generic_name(g), "{g}");
        }
        for n in ["AU05", "CIDOO ABM066", "USB Receiver", "Moonriver 2"] {
            assert!(!generic_name(n), "{n}");
        }
    }

    #[test]
    fn a_session_counts_repeats_combinations_and_claimed_hotkeys() {
        let mut s = Session::new("simulated", fake_devices());
        s.claimed = vec![0x85];
        s.ingest(&key("fake-keyboard", 0x1D, 0, 0x11)); // Ctrl down
        s.ingest(&key("fake-keyboard", 0x2A, 0, 0x10)); // Shift down
        s.ingest(&key("fake-keyboard", 0x20, 0, 0x44)); // D down
        s.ingest(&key("fake-keyboard", 0x20, 0, 0x44)); // D repeat
        s.ingest(&key("fake-keyboard", 0x20, 1, 0x44)); // D up
        let d: Vec<&Event> = s.events.iter().filter(|e| e.code == "KeyD").collect();
        assert_eq!(d.iter().map(|e| e.action).collect::<Vec<_>>(), ["down", "repeat", "up"]);
        assert_eq!(d[0].mods, ["Ctrl", "Shift"]);
        let c = &s.counts["fake-keyboard"];
        assert_eq!((c.down, c.repeat, c.up), (3, 1, 1));
        s.ingest(&key("fake-keyboard", 0x1D, 1, 0x11));
        s.ingest(&key("fake-keyboard", 0x2A, 1, 0x10));
        // The Vibe Key's K3 types F22, which UC holds as a hotkey.
        s.ingest(&key("fake-au05", 0x6D, 0, 0x85));
        s.ingest(&key("fake-au05", 0x6D, 1, 0x85));
        let f22: Vec<&Event> = s.events.iter().filter(|e| e.code == "F22").collect();
        assert!(f22[0].claimed, "the press is swallowed by UC's hotkey");
        assert!(!f22[1].claimed, "the release is not");
        assert_eq!(s.counts["fake-au05"].down, 1);
        // The same key held on two devices is two keys.
        s.ingest(&key("fake-keyboard", 0x1E, 0, 0x41));
        s.ingest(&key("fake-au05", 0x1E, 0, 0x41));
        assert_eq!(s.events.back().unwrap().action, "down");
        // Sequence numbers only grow.
        assert!(s.events.iter().zip(s.events.iter().skip(1)).all(|(a, b)| b.seq == a.seq + 1));
    }

    #[test]
    fn mouse_moves_are_counted_and_logged_only_when_asked() {
        let mut s = Session::new("simulated", fake_devices());
        let mv = Record::Mouse { device: "fake-mouse".into(), buttons: 0, data: 0, dx: 3, dy: -1 };
        s.ingest(&mv);
        assert!(s.events.is_empty());
        assert_eq!(s.counts["fake-mouse"].moves, 1);
        s.log_moves = true;
        s.ingest(&mv);
        assert_eq!((s.events[0].kind, s.events[0].dx, s.events[0].dy), ("move", Some(3), Some(-1)));
        s.ingest(&Record::Mouse { device: "fake-mouse".into(), buttons: 0x0400, data: 120, dx: 0, dy: 0 });
        assert_eq!(s.events.back().unwrap().delta, Some(120));
        assert_eq!(s.counts["fake-mouse"].wheel, 1);
    }

    #[test]
    fn hid_usages_become_presses_and_releases() {
        let mut s = Session::new("simulated", fake_devices());
        let hid = |u: Vec<(u16, u16)>, b: Vec<u8>| Record::Hid { device: "fake-au05-media".into(), usages: u, bytes: b };
        s.ingest(&hid(vec![(0x0C, 0xE9)], vec![2, 0xE9, 0]));
        s.ingest(&hid(vec![], vec![2, 0, 0]));
        let e: Vec<(&str, &str)> = s.events.iter().map(|e| (e.action, e.name.as_str())).collect();
        assert_eq!(e, [("down", "Volume Up"), ("up", "Volume Up")]);
        assert_eq!(s.events[0].bytes.as_deref(), Some("02 E9 00"));
        assert_eq!(s.counts["fake-au05-media"].hid, 2);
        // A report without button changes is logged once while its bytes stay the same.
        s.ingest(&hid(vec![], vec![1, 0x80, 0x7F]));
        let n = s.events.len();
        s.ingest(&hid(vec![], vec![1, 0x80, 0x7F]));
        assert_eq!(s.events.len(), n);
    }

    #[test]
    fn the_log_is_bounded_and_devices_coming_and_going_are_logged() {
        let mut s = Session::new("simulated", fake_devices());
        for _ in 0..(KEEP + 10) / 2 {
            s.ingest(&key("fake-keyboard", 0x1E, 0, 0x41));
            s.ingest(&key("fake-keyboard", 0x1E, 1, 0x41));
        }
        assert_eq!(s.events.len(), KEEP);
        assert_eq!(s.dropped, 10);
        let mut devs = fake_devices();
        devs.retain(|d| d.id != "fake-au05");
        s.device_change(devs);
        assert_eq!(s.events.back().unwrap().action, "removed");
        assert!(!s.devices.iter().any(|d| d.id == "fake-au05"), "a removed device with no events leaves the list");
    }

    #[test]
    fn the_tester_runs_only_while_polled_and_test_records_parse() {
        let t = Arc::new(Tester::default());
        assert_eq!(t.poll(0, vec![])["running"], false);
        let v = t.start(true);
        assert_eq!(v["mode"], "simulated");
        assert_eq!(v["devices"].as_array().unwrap().len(), 4);
        t.ingest(&Record::from_json(&json!({"type":"key","device":"fake-au05","make":0x6D,"flags":0,"vk":0x85})).unwrap());
        let p = t.poll(0, vec![0x85]);
        assert_eq!(p["events"][0]["code"], "F22");
        assert_eq!(t.poll(p["seq"].as_u64().unwrap(), vec![])["events"].as_array().unwrap().len(), 0);
        assert!(!t.idle());
        assert_eq!(t.configure(&json!({"logMoves": true}))["logMoves"], true);
        t.stop();
        assert!(!t.running());
        assert!(Record::from_json(&json!({"type":"nope","device":"x"})).is_err());
        let h = Record::from_json(&json!({"type":"hid","device":"d","usages":[[12,233]],"bytes":[2,233,0]})).unwrap();
        assert_eq!(h, Record::Hid { device: "d".into(), usages: vec![(12, 233)], bytes: vec![2, 233, 0] });
    }

    /// Read-only: lists this PC's keyboards, mice and HID controls with their names (no device is opened, no Raw
    /// Input is registered, no key is read). Run by hand: `cargo test --lib list_real_input_devices -- --ignored --nocapture`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn list_real_input_devices() {
        let list = win::devices();
        for d in &list {
            println!("{:8} {:16} {:40} {:?}:{:?}", d.kind, d.role, d.name, d.vid.map(|v| format!("{v:04X}")), d.pid.map(|v| format!("{v:04X}")));
        }
        assert!(list.iter().any(|d| d.kind == "keyboard"));
    }

    #[test]
    fn recordings_are_saved_only_as_wav_and_in_the_test_folder() {
        assert_eq!(base64_decode("UklGRg==").unwrap(), b"RIFF");
        assert_eq!(base64_decode("TWFu").unwrap(), b"Man");
        assert!(base64_decode("!!").is_err());
        assert!(save_wav("TWFu", "x").is_err(), "not a WAV file");
        // Unit tests are a simulated instance: the file goes to the app folder, never Downloads.
        assert!(simulated());
        let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
        wav.resize(44, 0);
        let b64 = {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in wav.chunks(3) {
                let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                for i in 0..4 {
                    if i <= c.len() {
                        s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
                    } else {
                        s.push('=');
                    }
                }
            }
            s
        };
        assert_eq!(base64_decode(&b64).unwrap(), wav);
        let p = save_wav(&b64, "2026-11-17-120000").unwrap();
        assert!(p.contains("uc-core-unit-tests") || std::env::var_os("UC_HOME").is_some(), "{p}");
        assert!(p.ends_with(".wav"));
        let _ = std::fs::remove_file(p);
    }
}
