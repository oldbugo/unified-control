//! Ulanzi AU05 Vibe Key: the host-side worker. The driver (framing, allowlist, safe session, fake key and
//! Windows transport) is the `ulanzi_au05` crate in the uc-ulanzi repository.
//!
//! - Opt-in: nothing runs until the user enables the key in General (`au05.enabled`, off by default).
//! - One background thread owns the device. It is restarted after a crash (`sync::restarting`); a crash never
//!   reaches the dock, the cooler or the window.
//! - Settings that live on the key (standby, power-off, light, microphone, what each key types) are read from
//!   the key and written to it at once when the user changes them. UC keeps no copy of them.
//! - UC's own settings (enable switch, a UC action for each key) are in settings.json.
//! - Actions do not use the key's private events: K2, K3 and the knob press never sent one (real key,
//!   2026-11-16). A key set to "Unified Control action" types its own unused code (F20/F21/F22/F19), and UC
//!   claims that code as a global hotkey (`au05_hotkey`). Hotkeys are claimed only while the key is enabled
//!   and an action is chosen, and released on disable and quit.
//! - Every known setting can be sent to a real key (user request 2026-11-16: "expose all the parameters in
//!   the app so I can test it out all at once"). Settings not checked on the real key
//!   (`Request::verified_on_hardware`) are labelled "Untested" on the page. Each set is read back.
//! - With `UC_FAKE_DEVICE` set the real key is never opened, and a fake desktop claims no hotkey.
//! - Quick menu (`au05_menu`, 2026-11-17): a key with "Open quick menu" opens a menu window; knob turns move
//!   (private events, or F23/F24 hotkeys if the knob is set to type them), the knob press (F19) chooses.
//!   Profiles are named sets of key actions; "the target app" decides what "Target app" actions do. The
//!   menu never writes to the key.
use super::au05_hotkey::{self as hotkey, Action, Desktop, FakeDesktop, QUICK_MENU, TARGETS};
use super::au05_menu::{self as menu, Input, MenuState, MenuView, Outcome};
use super::sync::{restarting, MutexExt};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ulanzi_au05::fake::FakeHandle;
use ulanzi_au05::protocol::{self, Control, KeyId, KeyTyping, LightField, LightState, MicNr, Request, SleepTime};
use ulanzi_au05::{Failure, Output, Session, SessionConfig, SessionState, Transport};

/// Shown when the key has sent nothing for long enough that it has probably switched itself off.
pub const OFF_MESSAGE: &str = "Receiver connected · Vibe Key probably switched off: hold the power button on its right side for 2–3 s";
pub const AWAKE_MESSAGE: &str = "Receiver connected · Vibe Key awake";
pub const ASLEEP_MESSAGE: &str = "Receiver connected · Vibe Key asleep. Press a key or turn the knob to wake it";
pub const WAITING_MESSAGE: &str = "Receiver connected · press a key or turn the knob to wake the Vibe Key";

/// The standby time used before the key's own was read (0.36.0): the last one read (kept in the settings),
/// else 300 s.
pub const DEFAULT_STANDBY_S: u32 = 300;
/// The quick menu's fold times for Claude sessions, in hours (0 = never fold), and the default (1.0).
pub const SESSION_FOLD_H: [u64; 6] = [0, 1, 4, 8, 24, 72];
/// 1.0 (user feedback 2026-11-18): no "More" folding unless the user picks a time; long lists keep scrolling.
pub const SESSION_FOLD_DEFAULT_H: u64 = 0;

/// The state the page shows (0.36.0, deterministic: the key's own timers run from its last input). Display
/// only: sending still follows the session's own 11 s gate. `quiet`: since the last input, which is the latest
/// private message or hotkey from one of the key's own codes (`None`: none since the connection opened).
/// Awake while `quiet` is under the key's standby time; asleep from then on; probably switched off once it
/// reaches the power-off time (`None` or 0: never). Any message or key hotkey makes it awake at once.
/// (2026-11-17: during dictation the key sends nothing for 30 to 90 s, and status messages are not regular
/// when idle, so no guess from silence is used.)
pub fn shown_state(
    state: SessionState,
    quiet: Option<Duration>,
    since_open: Duration,
    standby: Duration,
    power_off: Option<Duration>,
) -> (&'static str, &'static str) {
    match state {
        SessionState::Paused => return ("paused", "A reply was slow. Sending is paused for 30 seconds"),
        SessionState::Suspended => return ("suspended", "Sending stopped after repeated slow replies. Use Reconnect"),
        _ => {}
    }
    let power_off = power_off.filter(|p| !p.is_zero());
    match quiet {
        Some(q) if power_off.is_some_and(|p| q >= p) => ("off", OFF_MESSAGE),
        Some(q) if q < standby || state == SessionState::Awake => ("connected", AWAKE_MESSAGE),
        Some(_) => ("asleep", ASLEEP_MESSAGE),
        // Nothing since the connection opened: waiting, then (past standby + 2 minutes) probably off.
        None if state == SessionState::Awake => ("connected", AWAKE_MESSAGE),
        None if since_open > standby + Duration::from_secs(120) => ("off", OFF_MESSAGE),
        None => ("waiting", WAITING_MESSAGE),
    }
}

/// The six controls in the order of their device index (test clicks and the event list).
pub const CONTROLS: [(&str, Control); 6] = [
    ("k1", Control::K1),
    ("k2", Control::K2),
    ("k3", Control::K3),
    ("knobPress", Control::KnobPress),
    ("knobRight", Control::KnobRight),
    ("knobLeft", Control::KnobLeft),
];
/// The keys that can run a UC action (through their own code as a hotkey), with their settings key.
pub const ACTION_KEYS: [(&str, KeyId); 4] =
    [("k1", KeyId::K1), ("k2", KeyId::K2), ("k3", KeyId::K3), ("knobPress", KeyId::KnobPress)];
const EVENT_LOG: usize = 40;
/// How long a settings change may wait for the key: asleep (20 s), quiet time, and a slow key-typing reply
/// (up to 20 s).
pub const SET_WAIT: Duration = Duration::from_secs(45);
/// 0.37.0: the strict send rule (a private message from the key within this long), used after a stall.
pub const STRICT_WINDOW: Duration = Duration::from_secs(11);
/// After a send stalled while the key's timers said awake, only the strict rule applies for this long.
pub const STRICT_FOR: Duration = Duration::from_secs(300);
/// A change made while the key is asleep waits this long for the key to be touched, then is not applied.
pub const TOUCH_WAIT: Duration = Duration::from_secs(60);
/// The send rule's margin before the key's standby time.
pub const STANDBY_MARGIN: Duration = Duration::from_secs(10);
/// The read burst after the key wakes is capped by total time; reads still queued then are dropped.
const READ_BURST_LIMIT: Duration = Duration::from_secs(10);
/// The automatic settings reads wait until the key has not been pressed or turned for this many settle times.
const READ_QUIET_SETTLES: u32 = 4;

/// The Windows virtual key of each key's own code.
pub fn own_vk(key: KeyId) -> u16 {
    match key {
        KeyId::K1 => hotkey::VK_F20,
        KeyId::K2 => hotkey::VK_F21,
        KeyId::K3 => hotkey::VK_F22,
        KeyId::KnobPress => hotkey::VK_F19,
        KeyId::KnobRight => hotkey::VK_F23,
        KeyId::KnobLeft => hotkey::VK_F24,
    }
}
fn settings_key(key: KeyId) -> &'static str {
    match key {
        KeyId::KnobRight => "knobRight",
        KeyId::KnobLeft => "knobLeft",
        k => ACTION_KEYS[k.index() as usize].0,
    }
}
/// What turning the knob does while the quick menu is closed and the knob types F23/F24.
pub const TURN_ACTIONS: [&str; 3] = ["scroll", "volume", "none"];

pub fn default_settings() -> Value {
    sanitize(&json!({}))
}

/// The six controls whose typing the key stores, with their settings names.
pub const LAYER_CONTROLS: [(&str, KeyId); 6] = [
    ("k1", KeyId::K1),
    ("k2", KeyId::K2),
    ("k3", KeyId::K3),
    ("knobPress", KeyId::KnobPress),
    ("knobRight", KeyId::KnobRight),
    ("knobLeft", KeyId::KnobLeft),
];

/// The offline layer (what the key types on its own, without UC): `{mode: "factory"}` or `{mode: "keys",
/// keys: "Ctrl + D"}` per control.
fn sanitize_offline(v: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (name, _) in LAYER_CONTROLS {
        let c = &v[name];
        let keys = c["keys"].as_str().unwrap_or("");
        let entry = if c["mode"] == "keys" && KeyTyping::parse(keys).is_ok() {
            json!({"mode": "keys", "keys": keys})
        } else {
            json!({"mode": "factory"})
        };
        out.insert(name.to_string(), entry);
    }
    Value::Object(out)
}
fn offline_typing(offline: &Value, name: &str) -> KeyTyping {
    let c = &offline[name];
    if c["mode"] == "keys" {
        KeyTyping::parse(c["keys"].as_str().unwrap_or("")).unwrap_or_else(|_| KeyTyping::factory())
    } else {
        KeyTyping::factory()
    }
}

/// What the key should hold: the live layer (UC running: a control with a UC action types its own code,
/// the others their offline typing) or the offline layer, per control in `LAYER_CONTROLS` order.
#[derive(Clone, Debug, PartialEq)]
pub struct Layers {
    pub live: Vec<KeyTyping>,
    pub offline: Vec<KeyTyping>,
    pub uc: Vec<bool>,
    /// Write the offline layer (quit, disable, session end, or "Store offline layer on the key now").
    pub target_offline: bool,
    /// Controls whose layer changed in this run: their unknown stored list is read (then written if it
    /// differs).
    pub dirty: Vec<bool>,
    /// The offline typing was set by the user or seeded from the key. Until then nothing is written for this
    /// control: its stored list (the user's keys) seeds the offline layer when it becomes known.
    pub seeded: Vec<bool>,
    /// Key typing for this control was checked on the real key. The others (knob press, knob left) are
    /// written only after the user's "Set up".
    pub verified: Vec<bool>,
    /// Seeds learned in this run that are not in the settings yet (kept across `configure`).
    pub seeds: Vec<Option<KeyTyping>>,
    /// The last list the key confirmed per control (saved, `au05.keyEcho`): at start a control whose saved
    /// list is what UC wants is neither read nor written again.
    pub echo: Vec<Option<KeyTyping>>,
}
impl Layers {
    pub fn from_settings(au: &Value) -> Self {
        let offline_cfg = &au["offline"];
        // A key that opens the quick menu needs the knob press on its own code (F19) to choose, as the
        // hotkeys do (`apply_hotkeys`).
        let any_menu = ACTION_KEYS.iter().any(|(k, _)| au["actions"][k]["kind"] == QUICK_MENU);
        let mut live = Vec::new();
        let mut offline = Vec::new();
        let mut uc = Vec::new();
        let mut seeded = Vec::new();
        let mut verified = Vec::new();
        for (name, id) in LAYER_CONTROLS {
            let off = offline_typing(offline_cfg, name);
            let is_uc = match id {
                KeyId::KnobRight | KeyId::KnobLeft => au["knobTurn"] == "uc",
                KeyId::KnobPress if any_menu => true,
                // K2 / K3 with a menu role need their own codes too (else K3's factory Esc reaches the app).
                KeyId::K2 | KeyId::K3 if any_menu && menu::role(&au["menuControls"], name) != "none" => true,
                _ => au["actions"][name]["kind"].as_str().unwrap_or(hotkey::OFFLINE) != hotkey::OFFLINE,
            };
            live.push(if is_uc { KeyTyping::own_code(id) } else { off.clone() });
            seeded.push(au["offlineSet"][name] == true || !off.is_factory());
            // Checked on the real key in the driver, or set up by the user on this key (the set's echo matched).
            verified.push(Request::SetKeyTyping(id, KeyTyping::own_code(id)).verified_on_hardware() || au["setUp"][name] == true);
            offline.push(off);
            uc.push(is_uc);
        }
        let echo = LAYER_CONTROLS
            .iter()
            .map(|(name, _)| match au["keyEcho"][name].as_str() {
                Some("factory") => Some(KeyTyping::factory()),
                Some(keys) => KeyTyping::parse(keys).ok(),
                None => None,
            })
            .collect();
        Self { live, offline, uc, target_offline: false, dirty: vec![false; 6], seeded, verified, seeds: vec![None; 6], echo }
    }
    /// Take what the key holds as this control's offline typing (UC's own codes count as factory).
    pub fn seed(&mut self, i: usize, held: &KeyTyping) {
        let own = held.0.len() == 1 && (0x6e..=0x73).any(|c| *held == KeyTyping(vec![protocol::KeyEntry::Usage(c)]));
        let typing = if own || held.validate().is_err() { KeyTyping::factory() } else { held.clone() };
        if !self.uc[i] {
            self.live[i] = typing.clone();
        }
        self.offline[i] = typing.clone();
        self.seeded[i] = true;
        self.seeds[i] = Some(typing);
    }
    pub fn want(&self, i: usize) -> &KeyTyping {
        if self.target_offline { &self.offline[i] } else { &self.live[i] }
    }
}

/// The four keys' actions, each `{kind, value, app}` with a known kind. Without a kind: "offline" (the key
/// types its offline layer).
fn sanitize_actions(v: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (key, _) in ACTION_KEYS {
        let a = &v[key];
        let kind = a["kind"].as_str().filter(|k| hotkey::kind_known(k)).unwrap_or(hotkey::OFFLINE);
        let value = a["value"].as_str().filter(|s| s.len() <= 4096 && !s.contains('\0')).unwrap_or("");
        let app = a["app"].as_str().filter(|id| hotkey::app(id).is_some()).unwrap_or("");
        let custom = kind == "hotkey";
        out.insert(
            key.to_string(),
            json!({"kind": kind, "value": if custom { value } else { "" }, "app": if custom { app } else { "" }}),
        );
    }
    Value::Object(out)
}

/// Keep only known keys with valid values. The live `actions` always belong to the current profile.
pub fn sanitize(v: &Value) -> Value {
    // Two layers (2026-11-17): settings from before had "none" for a key without a UC action; that is now
    // "offline" (the key types its offline layer). Done once, marked by `layers: 1`.
    let migrate = |a: &Value| -> Value {
        let mut a = a.clone();
        if v["layers"] != 1 {
            for (key, _) in ACTION_KEYS {
                if a[key]["kind"] == "none" || a[key]["kind"].is_null() {
                    a[key] = json!({"kind": hotkey::OFFLINE, "value": "", "app": ""});
                }
            }
        }
        a
    };
    let v = &{
        let mut v = v.clone();
        if v["layers"] != 1 {
            v["actions"] = migrate(&v["actions"]);
            if let Some(ps) = v["profiles"].as_array_mut() {
                for p in ps.iter_mut() {
                    p["actions"] = migrate(&p["actions"]);
                }
            }
        }
        v
    };
    let actions = sanitize_actions(&v["actions"]);
    let source = v["profiles"]
        .as_array()
        .filter(|a| !a.is_empty())
        .cloned()
        .unwrap_or_else(|| menu::default_profiles(&actions).as_array().cloned().unwrap_or_default());
    let mut profiles: Vec<Value> = Vec::new();
    for p in source {
        let id = p["id"].as_str().unwrap_or("");
        if profiles.len() >= menu::MAX_PROFILES || !menu::valid_id(id) || profiles.iter().any(|q| q["id"] == id) {
            continue;
        }
        let name = menu::clean_label(&p["name"]);
        // Each profile has its own quick menu (user request 2026-11-17). Settings from before had one menu:
        // it is copied to every profile.
        let own = if p["menu"].is_array() { p["menu"].clone() } else if v["menu"].is_array() { v["menu"].clone() } else { menu::default_menu() };
        profiles.push(json!({"id": id, "name": if name.is_empty() { id.to_string() } else { name },
                             "actions": sanitize_actions(&p["actions"]), "menu": own}));
    }
    if profiles.is_empty() {
        profiles = menu::default_profiles(&actions).as_array().cloned().unwrap_or_default();
        let start = if v["menu"].is_array() { v["menu"].clone() } else { menu::default_menu() };
        for p in profiles.iter_mut() {
            p["menu"] = start.clone();
        }
    }
    let current = v["profile"]
        .as_str()
        .filter(|id| profiles.iter().any(|p| p["id"] == *id))
        .map(str::to_string)
        .unwrap_or_else(|| profiles[0]["id"].as_str().unwrap_or("").to_string());
    let ids: Vec<String> = profiles.iter().filter_map(|p| p["id"].as_str().map(str::to_string)).collect();
    // The live menu is the current profile's (the top-level `menu` wins, as `actions` do).
    let entries = if v["menu"].is_array() {
        v["menu"].clone()
    } else {
        profiles.iter().find(|p| p["id"] == current.as_str()).map(|p| p["menu"].clone()).unwrap_or_else(menu::default_menu)
    };
    let entries = menu::sanitize_menu(&entries, &ids);
    for p in profiles.iter_mut() {
        if p["id"] == current.as_str() {
            p["actions"] = actions.clone();
            p["menu"] = entries.clone();
        } else {
            p["menu"] = menu::sanitize_menu(&p["menu"], &ids);
        }
    }
    json!({
        "enabled": v["enabled"].as_bool().unwrap_or(false),
        "actions": actions,
        "profiles": profiles,
        "profile": current,
        "target": v["target"].as_str().filter(|t| TARGETS.contains(t)).unwrap_or("claude"),
        "menu": entries,
        // While the quick menu is open (user request 2026-11-17): what K2, K3 and the knob press do there.
        "menuControls": menu::sanitize_controls(&{
            let mut m = v.clone();
            migrate_menu_controls(&mut m);
            m["menuControls"].clone()
        }),
        "menuControlsVersion": MENU_CONTROLS_VERSION,
        // "Reopen where I left off" (user request 2026-11-17): the menu opens where it was closed.
        "reopenWhereLeft": v["reopenWhereLeft"].as_bool().unwrap_or(true),
        // Claude sessions not used for this many hours fold into "More" in the quick menu (1.0; 0 = never).
        "sessionFoldHours": v["sessionFoldHours"].as_u64().filter(|h| SESSION_FOLD_H.contains(h)).unwrap_or(SESSION_FOLD_DEFAULT_H),
        // The menus K2 cycles through (1.0).
        "menuFaces": menu::sanitize_faces(&v["menuFaces"]),
        // "factory": the knob scrolls by itself (stored on the key). "uc": it types F23/F24 and UC moves the
        // menu or runs `knobTurnAction` (scroll by default).
        // Set only by Connect / Disconnect on the page (they also edit Claude's settings).
        "claudeSessions": v["claudeSessions"].as_bool().unwrap_or(false),
        "knobTurn": if v["knobTurn"] == "uc" { "uc" } else { "factory" },
        "layers": 1,
        "offline": sanitize_offline(&v["offline"]),
        // Controls whose offline typing the user set or that were seeded from the key.
        "offlineSet": LAYER_CONTROLS.iter().map(|(n, _)| (n.to_string(), json!(v["offlineSet"][n] == true))).collect::<serde_json::Map<_, _>>(),
        // When an earlier run gave up waiting for a key-typing command (ms since 1970): the next run waits.
        "slowAbandonedAt": v["slowAbandonedAt"].as_u64(),
        // Controls not checked on the real key that the user set up on this key (the set's echo matched).
        "setUp": LAYER_CONTROLS.iter().filter(|(n, _)| v["setUp"][n] == true).map(|(n, _)| (n.to_string(), json!(true))).collect::<serde_json::Map<_, _>>(),
        // The last list the key confirmed per control ("factory", keys, or absent).
        "keyEcho": LAYER_CONTROLS.iter().filter_map(|(n, _)| {
            v["keyEcho"][n].as_str().filter(|k| *k == "factory" || KeyTyping::parse(k).is_ok()).map(|k| (n.to_string(), json!(k)))
        }).collect::<serde_json::Map<_, _>>(),
        // The offline layer could not be stored at the last quit or disable (key asleep or no time).
        "offlinePending": v["offlinePending"].as_bool().unwrap_or(false),
        "knobTurnAction": v["knobTurnAction"].as_str().filter(|a| TURN_ACTIONS.contains(a)).unwrap_or("scroll"),
        // The key's standby and power-off times as last read (0.36.0: the shown state uses them before the
        // key answers after a start).
        "knownTimes": {
            "standbyS": v["knownTimes"]["standbyS"].as_u64().filter(|s| (1..=86_400).contains(s)),
            "sleepS": v["knownTimes"]["sleepS"].as_u64().filter(|s| *s <= 86_400),
        },
    })
}

/// Switch to another profile: its key actions apply at once (no device write). A profile without a menu key
/// keeps the current menu key, so the menu can always switch back.
fn switch_profile(au: &mut Value, id: &str) -> Result<(), String> {
    let current = au["profile"].as_str().unwrap_or("").to_string();
    let actions = au["actions"].clone();
    let current_menu = au["menu"].clone();
    let profiles = au["profiles"].as_array_mut().ok_or("No profiles")?;
    let target = profiles.iter().find(|p| p["id"] == id).ok_or("Choose a saved profile")?;
    let mut next = target["actions"].clone();
    // Each profile has its own menu: switching switches the menu.
    let next_menu = if target["menu"].is_array() { target["menu"].clone() } else { current_menu.clone() };
    for p in profiles.iter_mut() {
        if p["id"] == current.as_str() {
            p["actions"] = actions.clone();
            p["menu"] = current_menu.clone();
        }
    }
    if !ACTION_KEYS.iter().any(|(k, _)| next[k]["kind"] == QUICK_MENU) {
        for (k, _) in ACTION_KEYS {
            if actions[k]["kind"] == QUICK_MENU {
                next[k] = actions[k].clone();
            }
        }
    }
    au["actions"] = next;
    au["menu"] = next_menu;
    au["profile"] = json!(id);
    Ok(())
}

fn profile_id(name: &str, profiles: &[Value]) -> String {
    let slug: String = name.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(20).collect();
    let base = match slug.trim_matches('-') {
        "" => "profile".to_string(),
        s => s.to_string(),
    };
    let mut id = base.clone();
    let mut n = 2;
    while profiles.iter().any(|p| p["id"] == id.as_str()) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// Apply a page patch to the saved `au05` settings. Returns log lines (names only, no typed text).
/// `{enabled?, action?: {control, kind, value, app}, profile?, target?, menu?, knobTurn?, knobTurnAction?,
///   profileEdit?: {op: add|rename|remove, id?, name?}}`
/// The menu controls' settings version. 2 (0.31.0): a knob press "close" saved from the old default becomes
/// "select", once (user feedback 2026-11-17); after that a user who picks "close" keeps it.
pub const MENU_CONTROLS_VERSION: u64 = 3;

/// The one-time menu controls migration on the raw Vibe Key settings. True when it changed anything (the
/// marker included), so the caller saves it.
pub fn migrate_menu_controls(au: &mut Value) -> bool {
    if !au.is_object() || au["menuControlsVersion"].as_u64().unwrap_or(0) >= MENU_CONTROLS_VERSION {
        return false;
    }
    let from = au["menuControlsVersion"].as_u64().unwrap_or(0);
    if from < 2 && au["menuControls"]["knobPress"] == "close" {
        au["menuControls"]["knobPress"] = json!("select");
    }
    // 1.0: K2 next menu and K3 close are the new defaults; the old defaults (select, back) move to them once.
    // A role the user chose (anything else) is kept.
    if from < 3 && au["menuControls"].is_object() {
        if au["menuControls"]["k2"] == "select" {
            au["menuControls"]["k2"] = json!("nextMenu");
        }
        if au["menuControls"]["k3"] == "back" {
            au["menuControls"]["k3"] = json!("close");
        }
    }
    au["menuControlsVersion"] = json!(MENU_CONTROLS_VERSION);
    true
}

pub fn apply_patch(au: &mut Value, patch: &Value) -> Result<Vec<String>, String> {
    *au = sanitize(au);
    let mut lines = Vec::new();
    if let Some(v) = patch.get("enabled") {
        let v = v.as_bool().ok_or("Invalid Vibe Key setting")?;
        au["enabled"] = json!(v);
        lines.push(format!("page enabled = {v}"));
    }
    if let Some(a) = patch.get("action") {
        let control = a["control"].as_str().unwrap_or("");
        if !ACTION_KEYS.iter().any(|(k, _)| *k == control) {
            return Err("Unknown Vibe Key control".into());
        }
        let (kind, value, app) = (a["kind"].as_str().unwrap_or(""), a["value"].as_str().unwrap_or(""), a["app"].as_str().unwrap_or(""));
        // A custom hotkey with an empty value is saved while the user is still typing; it does nothing.
        validate_action(kind, value, app)?;
        au["actions"][control] = json!({"kind": kind, "value": value, "app": app});
        let app_note = if app.is_empty() { String::new() } else { format!(" (app {app})") };
        lines.push(format!("page action {control} = {kind}{app_note}"));
    }
    if let Some(e) = patch.get("profileEdit") {
        let name = menu::clean_label(&e["name"]);
        let id = e["id"].as_str().unwrap_or("").to_string();
        let profiles = au["profiles"].as_array().cloned().unwrap_or_default();
        let known = profiles.iter().any(|p| p["id"] == id.as_str());
        match e["op"].as_str().unwrap_or("") {
            "add" => {
                if profiles.len() >= menu::MAX_PROFILES {
                    return Err("At most 12 profiles".into());
                }
                if name.is_empty() {
                    return Err("Give the profile a name".into());
                }
                // A new profile starts as a copy of the current key actions and menu.
                let new_id = profile_id(&name, &profiles);
                let (actions, menu_now) = (au["actions"].clone(), au["menu"].clone());
                if let Some(ps) = au["profiles"].as_array_mut() {
                    ps.push(json!({"id": new_id, "name": name, "actions": actions, "menu": menu_now}));
                }
                lines.push(format!("page profile added {new_id}"));
            }
            "rename" => {
                if name.is_empty() || !known {
                    return Err(if known { "Give the profile a name" } else { "Choose a saved profile" }.into());
                }
                for p in au["profiles"].as_array_mut().into_iter().flatten() {
                    if p["id"] == id.as_str() {
                        p["name"] = json!(name);
                    }
                }
                lines.push(format!("page profile renamed {id}"));
            }
            "remove" => {
                if !known {
                    return Err("Choose a saved profile".into());
                }
                if profiles.len() <= 1 {
                    return Err("Keep at least one profile".into());
                }
                if au["profile"] == id.as_str() {
                    let other = profiles.iter().filter_map(|p| p["id"].as_str()).find(|p| *p != id).unwrap_or("").to_string();
                    switch_profile(au, &other)?;
                }
                if let Some(ps) = au["profiles"].as_array_mut() {
                    ps.retain(|p| p["id"] != id.as_str());
                }
                lines.push(format!("page profile removed {id}"));
            }
            _ => return Err("Unknown profile change".into()),
        }
    }
    if let Some(id) = patch.get("profile") {
        let id = id.as_str().ok_or("Choose a saved profile")?;
        switch_profile(au, id)?;
        lines.push(format!("profile = {id}"));
    }
    if let Some(t) = patch.get("target") {
        let t = t.as_str().filter(|t| TARGETS.contains(t)).ok_or("Choose Claude or ChatGPT")?;
        au["target"] = json!(t);
        lines.push(format!("target = {t}"));
    }
    if let Some(o) = patch.get("offline") {
        let name = o["control"].as_str().unwrap_or("");
        if !LAYER_CONTROLS.iter().any(|(n, _)| *n == name) {
            return Err("Unknown Vibe Key control".into());
        }
        let entry = match o["mode"].as_str().unwrap_or("") {
            "factory" => json!({"mode": "factory"}),
            "keys" => {
                let keys = o["keys"].as_str().unwrap_or("");
                KeyTyping::parse(keys)?;
                json!({"mode": "keys", "keys": keys})
            }
            _ => return Err("Choose what the key types on its own".into()),
        };
        let seeding = o["seed"] == true;
        if seeding && au["offlineSet"][name] == true {
            // The user set it meanwhile: a seed never overwrites a choice.
        } else {
            au["offline"][name] = entry;
            au["offlineSet"][name] = json!(true);
            lines.push(format!("{} offline layer {name} = {}", if seeding { "seeded from the key:" } else { "page" }, o["mode"].as_str().unwrap_or("")));
        }
    }
    if let Some(c) = patch.get("setUp") {
        // Saved by the worker after "Set up" worked on this key.
        let name = c["control"].as_str().unwrap_or("");
        if !LAYER_CONTROLS.iter().any(|(n, _)| *n == name) {
            return Err("Unknown Vibe Key control".into());
        }
        au["setUp"][name] = json!(true);
        lines.push(format!("{name} set up on this key"));
    }
    if let Some(t) = patch.get("knownTimes") {
        // Saved by the worker when it reads the key's times (no log line: not a user change).
        for (k, max, min) in [("standbyS", 86_400u64, 1u64), ("sleepS", 86_400, 0)] {
            if let Some(v) = t[k].as_u64().filter(|v| (min..=max).contains(v)) {
                au["knownTimes"][k] = json!(v);
            }
        }
    }
    if let Some(e) = patch.get("keyEcho") {
        // Saved by the worker when the key confirms a list (no log line: it is not a user change).
        let name = e["control"].as_str().unwrap_or("");
        if !LAYER_CONTROLS.iter().any(|(n, _)| *n == name) {
            return Err("Unknown Vibe Key control".into());
        }
        match e["keys"].as_str() {
            Some(k) if k == "factory" || KeyTyping::parse(k).is_ok() => au["keyEcho"][name] = json!(k),
            _ => {
                if let Some(m) = au["keyEcho"].as_object_mut() {
                    m.remove(name);
                }
            }
        }
    }
    if let Some(m) = patch.get("menu") {
        let m = if m == "default" { menu::default_menu() } else { m.clone() };
        if !m.is_array() {
            return Err("Invalid quick menu".into());
        }
        // The current profile's menu (sanitize copies it into the profile).
        au["menu"] = m.clone();
        lines.push(format!("page menu saved for profile {} ({} entries)", au["profile"].as_str().unwrap_or(""), m.as_array().map(|a| a.len()).unwrap_or(0)));
    }
    if let Some(c) = patch.get("menuControls") {
        if !c.is_object() {
            return Err("Invalid menu controls".into());
        }
        let mut next = au["menuControls"].clone();
        for (k, _) in menu::CONTROL_KEYS {
            if let Some(r) = c.get(k) {
                let r = r.as_str().filter(|r| menu::ROLES.contains(r)).ok_or("Choose what the control does in the menu")?;
                next[k] = json!(r);
                lines.push(format!("page menu control {k} = {r}"));
            }
        }
        au["menuControls"] = next;
    }
    if let Some(v) = patch.get("menuFaces") {
        let faces = v.as_array().ok_or("Invalid menus")?;
        if faces.is_empty() || faces.len() > menu::MAX_FACES {
            return Err(format!("Keep between 1 and {} menus", menu::MAX_FACES));
        }
        let clean = menu::sanitize_faces(v);
        if clean.as_array().map_or(0, |a| a.len()) != faces.len() {
            return Err("Choose a style and what each menu shows".into());
        }
        au["menuFaces"] = clean;
        lines.push(format!("page menus = {}", faces.len()));
    }
    if let Some(v) = patch.get("sessionFoldHours") {
        let h = v.as_u64().filter(|h| SESSION_FOLD_H.contains(h)).ok_or("Choose one of the offered times")?;
        au["sessionFoldHours"] = json!(h);
        lines.push(format!("page menu session fold = {h} h"));
    }
    if let Some(v) = patch.get("reopenWhereLeft") {
        let v = v.as_bool().ok_or("Invalid Vibe Key setting")?;
        au["reopenWhereLeft"] = json!(v);
        lines.push(format!("page menu reopen where left = {v}"));
    }
    if let Some(k) = patch.get("knobTurn") {
        let k = k.as_str().filter(|k| *k == "factory" || *k == "uc").ok_or("Choose what the knob turn does")?;
        au["knobTurn"] = json!(k);
        lines.push(format!("page knob turn = {k}"));
    }
    if let Some(a) = patch.get("knobTurnAction") {
        let a = a.as_str().filter(|a| TURN_ACTIONS.contains(a)).ok_or("Choose what turning the knob does")?;
        au["knobTurnAction"] = json!(a);
        lines.push(format!("page knob turn action = {a}"));
    }
    *au = sanitize(au);
    Ok(lines)
}

/// Check one UC action before it is saved.
pub fn validate_action(kind: &str, value: &str, app: &str) -> Result<(), String> {
    hotkey::validate(kind, value, app)
}

fn saved_action(actions: &Value, key: &str) -> Action {
    let a = &actions[key];
    Action::from_setting(
        a["kind"].as_str().unwrap_or("none"),
        a["value"].as_str().unwrap_or(""),
        a["app"].as_str().unwrap_or(""),
    )
    .unwrap_or(Action::Nothing)
}

/// A settings change the page can make. Each maps to exactly one allowlisted request.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    StandbyTime(u32),
    SleepTime(u32),
    LightMode(u8),
    LightBrightness(u8),
    MicNrLevel(u8),
    /// Clear a failed key job so the layer sync tries once more (the page's "Try again").
    Retry(KeyId),
    LightRecord { record: u8, field: LightField, value: u8 },
    HooksMode(u8),
}

/// The page's name for the setting a change edits (one entry per setting in the status `changes`).
pub fn change_key(c: &Change) -> String {
    match c {
        Change::StandbyTime(_) => "standbyTime".into(),
        Change::SleepTime(_) => "sleepTime".into(),
        Change::LightMode(_) => "lightMode".into(),
        Change::LightBrightness(_) => "lightBrightness".into(),
        Change::MicNrLevel(_) => "micNr".into(),
        Change::HooksMode(_) => "hooksMode".into(),
        Change::Retry(_) => "retry".into(),
        Change::LightRecord { record, field, .. } => {
            let name = LIGHT_FIELDS.iter().find(|(_, f)| f == field).map(|(n, _)| *n).unwrap_or("field");
            format!("lightRecord:{record}:{name}")
        }
    }
}
fn change_value(c: &Change) -> Value {
    match c {
        Change::StandbyTime(v) | Change::SleepTime(v) => json!(v),
        Change::LightMode(v) | Change::LightBrightness(v) | Change::MicNrLevel(v) | Change::HooksMode(v) => json!(v),
        Change::LightRecord { value, .. } => json!(value),
        Change::Retry(_) => Value::Null,
    }
}
/// What the key holds for the setting a change edits (after its read-back), and whether it is what was sent.
/// Values only (no raw bytes); for mic noise reduction the level and both 16-bit values.
pub fn read_back(c: &Change, k: &Known) -> Option<(bool, String)> {
    match c {
        Change::StandbyTime(s) => k.standby_s.map(|v| (v == *s, format!("{v} s"))),
        Change::SleepTime(s) => k.sleep_s.map(|v| (v == *s, format!("{v} s"))),
        Change::LightMode(m) => k.light.map(|l| (l.mode == *m, format!("mode {}", l.mode))),
        Change::LightBrightness(b) => k.light.map(|l| (l.brightness == *b, format!("brightness {}", l.brightness))),
        Change::MicNrLevel(l) => k.mic.map(|m| (m.level == *l, format!("level {} ({}, {})", m.level, m.a, m.b))),
        Change::HooksMode(m) => k.hooks_mode.map(|v| (v == *m, format!("hooks mode {v}"))),
        Change::LightRecord { record, field, value } => {
            let i = LIGHT_FIELDS.iter().position(|(_, f)| f == field)?;
            k.light.map(|l| {
                let v = l.records[usize::from(*record).min(3)][i];
                (v == *value, format!("{v}"))
            })
        }
        Change::Retry(_) => None,
    }
}

/// Page names of the light record fields.
pub const LIGHT_FIELDS: [(&str, LightField); 5] = [
    ("type", LightField::Type),
    ("time", LightField::Time),
    ("breatheLevel", LightField::BreatheLevel),
    ("breatheBrightness", LightField::BreatheBrightness),
    ("alwaysOnBrightness", LightField::AlwaysOnBrightness),
];

/// Parse a page request `{setting, value, key?, mode?, keys?}`.
pub fn parse_change(v: &Value) -> Result<Change, String> {
    let num = || v["value"].as_u64().ok_or_else(|| "Choose a value".to_string());
    let byte = || num().and_then(|n| u8::try_from(n).map_err(|_| "Value out of range".to_string()));
    match v["setting"].as_str().unwrap_or("") {
        "standbyTime" => {
            let s = u32::try_from(num()?).map_err(|_| "Value out of range".to_string())?;
            Ok(Change::StandbyTime(s))
        }
        "sleepTime" => {
            let s = u32::try_from(num()?).map_err(|_| "Value out of range".to_string())?;
            SleepTime::new(s).ok_or("Choose one of the offered power-off times")?;
            Ok(Change::SleepTime(s))
        }
        "lightMode" => Ok(Change::LightMode(byte()?)),
        "lightBrightness" => Ok(Change::LightBrightness(byte()?)),
        "micNr" => Ok(Change::MicNrLevel(byte()?)),
        "hooksMode" => Ok(Change::HooksMode(byte()?)),
        "lightRecord" => {
            let record = v["record"].as_u64().and_then(|r| u8::try_from(r).ok()).ok_or("Choose a light record")?;
            let field = LIGHT_FIELDS
                .iter()
                .find(|(n, _)| Some(*n) == v["field"].as_str())
                .map(|(_, f)| *f)
                .ok_or("Choose a light field")?;
            Ok(Change::LightRecord { record, field, value: byte()? })
        }
        // "Try again" on a failed key job, or "Set up" on a control not checked on the real key.
        "retryJob" => {
            let key = LAYER_CONTROLS
                .iter()
                .find(|(k, _)| Some(*k) == v["key"].as_str())
                .map(|(_, id)| *id)
                .ok_or("Choose a control")?;
            Ok(Change::Retry(key))
        }
        _ => Err("Unknown Vibe Key setting".into()),
    }
}

/// What the key reported. `None` = not read yet.
#[derive(Clone, Debug, Default)]
pub struct Known {
    battery: Option<(u16, u8)>,
    /// The status message's flags byte (class 0x7B, byte 4). Bit 0x08 is probably "charging" (not confirmed);
    /// changes are logged so the bits can be mapped on the real key.
    battery_flags: Option<u8>,
    /// When the battery was last reported (ms since 1970).
    battery_at: Option<u64>,
    standby_s: Option<u32>,
    sleep_s: Option<u32>,
    light: Option<LightState>,
    hooks_mode: Option<u8>,
    mic: Option<MicNr>,
    /// What each control types (K1..K3, knob press, knob right, knob left).
    keys: [Option<KeyTyping>; 6],
    standby_status: Option<u8>,
    codes: [Option<u8>; 6],
}

#[derive(Clone, Debug)]
struct EventRow {
    at_ms: u64,
    control: Control,
    pressed: bool,
    code: u8,
    /// `Some` for a hotkey press: what UC ran.
    action: Option<String>,
}

#[derive(Clone, Debug)]
struct Status {
    enabled: bool,
    state: &'static str,
    message: String,
    known: Known,
    events: VecDeque<EventRow>,
    last_error: Option<String>,
    pending_sets: usize,
    stalls: u64,
    sent: u64,
    generation: u64,
    jobs: [Option<KeyJob>; 6],
    sync: SyncState,
    /// The settings reads after a wake stalled twice: left "Not read" (the page offers Read again).
    reads_failed: bool,
    /// 0.37.0: per setting, the last change's state: "touch" (waiting for the key to be touched), "sending",
    /// "applied", "kept" (the key read back something else), "failed".
    changes: BTreeMap<String, Value>,
    /// Only the strict send rule applies until then (a send stalled while the timers said awake).
    strict_until: Option<Instant>,
}

/// Guards on key-typing commands (cmd 0x50; the 2026-11-16 incident and the 2026-11-17 review).
#[derive(Clone, Debug, Default)]
struct SyncState {
    /// The typing tried automatically per control: one automatic attempt per wanted typing.
    attempted: [Option<KeyTyping>; 6],
    /// An automatic read was tried for this control in this run.
    read_tried: [bool; 6],
    /// The last key-typing write for this control failed in a way that may have reached the key.
    uncertain: [bool; 6],
    /// Not checked on the real key: the user allowed writing it with "Set up".
    allowed: [bool; 6],
    /// "needs Set up" was logged for this control in this run (logged once).
    check_logged: [bool; 6],
    /// A key-typing command failed (no reply, write timeout...): no automatic one until Reconnect or Try again.
    blocked: bool,
    /// Quit or disable ran out of time: start no new key-typing command.
    no_new: bool,
    /// A key-typing command is queued or waiting for its answer.
    pending: bool,
    /// Send no key-typing command before this (an earlier run abandoned one).
    not_before: Option<Instant>,
    /// This run gave up waiting for a key-typing command at this time (ms since 1970).
    abandoned_at: Option<u64>,
}

fn push_event(status: &Mutex<Status>, control: Control, pressed: bool, code: u8, action: Option<String>) {
    let mut s = status.lock_or_recover();
    if s.events.len() >= EVENT_LOG {
        s.events.pop_front();
    }
    s.events.push_back(EventRow {
        at_ms: now_ms(),
        control,
        pressed,
        code,
        action,
    });
}

enum Cmd {
    Change(Change, Sender<Result<(), String>>),
    /// Read one key's typing list (cmd 0x50), only on the user's request.
    ReadKey(KeyId, Sender<Result<(), String>>),
    /// Queue every get except key typing (the page's "Read all settings").
    ReadAll(Sender<Result<(), String>>),
    Reconnect,
    Stop,
}

pub struct Au05 {
    fake: Option<FakeHandle>,
    status: Arc<Mutex<Status>>,
    actions: Arc<Mutex<Value>>,
    tx: Mutex<Option<Sender<Cmd>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    stop: Arc<AtomicBool>,
    desktop: Option<Arc<dyn Desktop>>,
    fake_desktop: Option<Arc<FakeDesktop>>,
    hotkeys: Mutex<BTreeMap<u16, Result<(), String>>>,
    /// The hotkey lines last logged (only changes are logged).
    hotkey_lines: Mutex<Vec<String>>,
    /// Serialises start, stop and hotkey claims, so two commands can never start two sessions.
    config_lock: Mutex<()>,
    config: SessionConfig,
    log: Arc<Log>,
    menu: Arc<MenuCtl>,
    layers: Arc<Mutex<Layers>>,
    /// The result of the last offline store at disable (taken by the app to save `offlinePending`).
    offline_result: Mutex<Option<bool>>,
    /// How long quit / disable wait for the offline layer, and how long a run waits after an abandoned
    /// key-typing command (tests shorten them).
    timing: Mutex<(Duration, Duration)>,
    /// The first `configure` builds the layers without marking anything changed.
    configured: AtomicBool,
    /// The strict send window and how long a change waits for the key to be touched (tests shorten them).
    gate: Mutex<(Duration, Duration)>,
    /// Fake key only (tests): change every key-typing echo, as a firmware that stores something else would.
    mangle_echo: Arc<AtomicBool>,
    /// The app is exiting (set at the start of `shutdown`): `configure` never starts the worker again and the
    /// worker saves nothing more (a late seed or echo save must not restart it).
    exiting: Arc<AtomicBool>,
}

/// `devices/au05/device.log` (rotated at about 1 MB, like the other device logs). Command names and results
/// only: no report bytes, no key, no window titles or typed text.
#[derive(Default)]
pub struct Log(Mutex<Option<std::path::PathBuf>>);
impl Log {
    pub fn line(&self, msg: &str) {
        if let Some(dir) = self.0.lock_or_recover().as_ref() {
            crate::device::log::log_in(dir, &format!("au05 {msg}"));
        }
    }
}

/// Saves a settings patch through the app (profile or target chosen in the menu).
pub type Persist = Arc<dyn Fn(Value) -> Result<(), String> + Send + Sync>;
/// The current Claude sessions (most recent first) and whether Claude sessions are connected.
pub type SessionsFn = Arc<dyn Fn() -> (Vec<Value>, bool) + Send + Sync>;

/// The quick menu and every hotkey press: routes key presses, knob turns and window input to the menu's
/// state machine, shows it through the view, and runs what was chosen.
pub struct MenuCtl {
    state: Mutex<MenuState>,
    /// The sanitized `au05` settings (menu, profiles, target, actions, knob input).
    settings: Mutex<Value>,
    view: Mutex<Option<Arc<dyn MenuView>>>,
    persist: Mutex<Option<Persist>>,
    desktop: Mutex<Option<std::sync::Weak<dyn Desktop>>>,
    log: Arc<Log>,
    status: Arc<Mutex<Status>>,
    opened: AtomicU64,
    /// The result of the last menu choice (shown on the page).
    last: Arc<Mutex<Option<String>>>,
    /// Dictate & send: running now, and when it last started (debounce).
    dictate_busy: Arc<AtomicBool>,
    dictate_last: Mutex<Option<Instant>>,
    sessions: Mutex<Option<SessionsFn>>,
    /// When a hotkey from one of the key's own codes last arrived (the key is in use: shown as awake).
    key_seen: Mutex<Option<Instant>>,
    /// The Claude desktop app's data folder (`claude_tree`), if it can be read.
    claude_data: Mutex<Option<std::path::PathBuf>>,
    /// The menu as shown, frozen while it is open (a changing session list never moves the highlight).
    frozen: Mutex<Option<Value>>,
    /// The menu shown (index into `menuFaces`, 1.0); kept between openings when "Reopen where I left off" is on.
    face: Mutex<usize>,
    /// Where the last session list came from (for the page).
    session_source: Mutex<Value>,
}

impl MenuCtl {
    fn settings(&self) -> Value {
        self.settings.lock_or_recover().clone()
    }
    pub fn is_open(&self) -> bool {
        self.state.lock_or_recover().open
    }
    /// True when knob turns reach UC as F23/F24: the key was read or set to type them, or (not read since
    /// start) the user chose that. Then the knob's private events do not move the menu, so a turn never
    /// moves twice.
    fn turns_are_hotkeys(&self) -> bool {
        let known = self.status.lock_or_recover().known.keys[KeyId::KnobRight.index() as usize].clone();
        match known {
            Some(t) => t == KeyTyping::own_code(KeyId::KnobRight),
            None => self.settings()["knobTurn"] == "uc",
        }
    }

    /// One input for the menu (`source` is for the log).
    /// The menu as shown: "Claude sessions" entries expanded to the current session list.
    fn shown_menu(&self, settings: &Value) -> Value {
        if self.is_open() {
            if let Some(m) = self.frozen.lock_or_recover().clone() {
                return m;
            }
        }
        self.fresh_menu(settings)
    }
    /// The menu with the current session list: Claude's own session files when readable, else the hook list.
    fn fresh_menu(&self, settings: &Value) -> Value {
        let (list, connected) = self.sessions.lock_or_recover().clone().map(|f| f()).unwrap_or_default();
        let base = self.claude_data.lock_or_recover().clone();
        let desktop = match &base {
            Some(b) => super::claude_tree::cached(b),
            None => Err("Claude's session files were not found".to_string()),
        };
        *self.session_source.lock_or_recover() = match &desktop {
            Ok(d) => {
                let working: Vec<String> = list
                    .iter()
                    .filter(|s| s["state"] == "working")
                    .filter_map(|s| s["desktopId"].as_str().map(str::to_string))
                    .collect();
                let mut v = super::claude_tree::summary(d, &working, now_ms(), 5);
                v["source"] = json!("desktop");
                v["groups"] = json!(d.groups.as_ref().ok().map(|g| g.scopes.values().map(|s| s.groups.len()).sum::<usize>()));
                v["groupsNote"] = json!(d.groups.as_ref().err());
                v["readAt"] = json!(now_ms());
                v
            }
            // The hook list (Connect): its sessions, newest first.
            Err(e) => {
                let mut rows: Vec<&Value> = list.iter().collect();
                rows.sort_by_key(|s| std::cmp::Reverse(s["last"].as_u64().unwrap_or(0)));
                let projects: std::collections::BTreeSet<&str> = list.iter().filter_map(|s| s["project"].as_str()).collect();
                json!({"source": "hooks", "note": e, "open": list.len(), "projects": projects.len(),
                       "working": list.iter().filter(|s| s["state"] == "working").count(), "readAt": now_ms(),
                       "recent": rows.iter().take(5).map(|s| json!({"title": s["title"].as_str().or(s["label"].as_str()).or(s["project"].as_str()).unwrap_or("Untitled session"),
                           "project": s["project"], "working": s["state"] == "working", "at": s["last"]})).collect::<Vec<_>>()})
            }
        };
        let fold = settings["sessionFoldHours"].as_u64().filter(|h| *h > 0).map(|h| h * 3600 * 1000);
        let face = self.face_of(settings);
        if face["source"] == "recent" {
            return match &desktop {
                Ok(d) => {
                    let working: Vec<String> =
                        list.iter().filter(|s| s["state"] == "working").filter_map(|s| s["desktopId"].as_str().map(str::to_string)).collect();
                    let recent = super::claude_tree::Recent {
                        spoke: list.iter().filter_map(|s| Some((s["desktopId"].as_str()?.to_string(), s["spoke"].as_u64()?))).collect(),
                        fold_ms: None,
                    };
                    let rows = super::claude_tree::recent_list(&d.sessions, d.groups.as_ref().ok(), &working, now_ms(), &recent);
                    if rows.is_empty() { json!([{"type": "note", "label": "No open Claude sessions"}]) } else { Value::Array(rows) }
                }
                Err(_) => {
                    let rows = menu::session_tree(&list, &json!("none"), now_ms());
                    let flat: Vec<Value> = rows.iter().flat_map(|p| p["entries"].as_array().cloned().unwrap_or_default()).collect();
                    if flat.is_empty() { json!([{"type": "note", "label": "No Claude sessions yet"}]) } else { Value::Array(flat) }
                }
            };
        }
        menu::expand_with(&settings["menu"], &list, connected, now_ms(), desktop.as_ref().ok(), fold)
    }

    /// The current menu (`menuFaces`), clamped.
    fn face_of(&self, settings: &Value) -> Value {
        let faces = settings["menuFaces"].as_array().cloned().unwrap_or_default();
        let i = *self.face.lock_or_recover();
        faces.get(i).or(faces.first()).cloned().unwrap_or_else(|| menu::default_faces()[0].clone())
    }
    /// The memory scope of the current menu: per profile for its own menu, shared for the recent list.
    fn scope_of(&self, settings: &Value) -> String {
        let i = *self.face.lock_or_recover();
        format!("{}#{i}", settings["profile"].as_str().unwrap_or(""))
    }
    /// The view with the current menu's style and the list of menus (for the window).
    fn with_face(&self, settings: &Value, mut view: Value) -> Value {
        let faces = settings["menuFaces"].as_array().cloned().unwrap_or_default();
        let face = self.face_of(settings);
        view["style"] = face["style"].clone();
        view["face"] = face["name"].clone();
        view["faceIndex"] = json!((*self.face.lock_or_recover()).min(faces.len().saturating_sub(1)));
        view["faces"] = json!(faces.iter().map(|f| f["name"].clone()).collect::<Vec<_>>());
        if face["source"] == "recent" {
            view["title"] = json!([face["name"]]);
        }
        view
    }

    pub fn input(self: &Arc<Self>, input: Input, source: &str) {
        let settings = self.settings();
        if input == Input::NextMenu {
            return self.next_menu(&settings, source);
        }
        if !self.is_open() && input == Input::Toggle && settings["reopenWhereLeft"] == false {
            *self.face.lock_or_recover() = 0;
        }
        // Read once when the menu opens; kept while it is open.
        let frozen = self.frozen.lock_or_recover().clone();
        let shown = match frozen {
            Some(m) if self.is_open() => m,
            _ => {
                let m = self.fresh_menu(&settings);
                *self.frozen.lock_or_recover() = Some(m.clone());
                m
            }
        };
        let (outcome, was_open, view) = {
            let mut st = self.state.lock_or_recover();
            let was = st.open;
            st.remember = settings["reopenWhereLeft"] != false;
            if !was {
                // The memory is per profile menu (and per menu, 1.0).
                st.scope = self.scope_of(&settings);
            }
            st.no_close = self.face_of(&settings)["style"] == "list";
            let o = st.step(&shown, input, Instant::now());
            (o, was, self.with_face(&settings, st.view(&settings, &shown)))
        };
        let open = view["open"] == true;
        if !open {
            *self.frozen.lock_or_recover() = None;
        }
        if let Some(v) = self.view.lock_or_recover().clone() {
            if open {
                v.show(&view, !was_open);
            } else if was_open {
                v.hide();
            }
        }
        match outcome {
            Outcome::Opened => {
                self.log.line(&format!("menu opened ({source})"));
                self.start_timer();
            }
            Outcome::Closed => self.log.line(&format!("menu closed ({source})")),
            Outcome::Run(a, label) => {
                self.log.line(&format!("menu chose {label}"));
                let action = saved_action(&json!({"x": a}), "x");
                self.run(action, None, true);
            }
            Outcome::Profile(id, label) => {
                self.log.line(&format!("menu chose profile {label}"));
                self.save(json!({"profile": id}), format!("Profile: {label}"));
            }
            Outcome::Target(t, label) => {
                self.log.line(&format!("menu chose target {label}"));
                self.save(json!({"target": t}), format!("Target app: {label}"));
            }
            Outcome::Session(id, then) => {
                // The label is the user's prompt text: never logged.
                self.log.line("menu chose a Claude session");
                self.open_session(id, then);
            }
            Outcome::Unavailable(m) => self.log.line(&format!("menu: {m}")),
            Outcome::Nothing | Outcome::Moved => {}
        }
    }

    /// K2 while open (1.0): show the next menu (after the last, the first again), where it was left.
    fn next_menu(self: &Arc<Self>, settings: &Value, source: &str) {
        if !self.is_open() {
            return;
        }
        let faces = settings["menuFaces"].as_array().map_or(1, |a| a.len().max(1));
        let before = self.shown_menu(settings);
        {
            let mut f = self.face.lock_or_recover();
            *f = (*f + 1) % faces;
        }
        let shown = self.fresh_menu(settings);
        *self.frozen.lock_or_recover() = Some(shown.clone());
        let view = {
            let mut st = self.state.lock_or_recover();
            st.no_close = self.face_of(settings)["style"] == "list";
            st.switch(&before, &shown, self.scope_of(settings), Instant::now());
            self.with_face(settings, st.view(settings, &shown))
        };
        self.log.line(&format!("menu next: {} ({source})", view["face"].as_str().unwrap_or("")));
        if let Some(v) = self.view.lock_or_recover().clone() {
            v.show(&view, false);
        }
    }

    /// Closes the menu after `CLOSE_AFTER` without input. One timer per opening.
    fn start_timer(self: &Arc<Self>) {
        let generation = self.opened.fetch_add(1, Ordering::SeqCst) + 1;
        let me = Arc::downgrade(self);
        let _ = std::thread::Builder::new().name("uc-au05-menu-timer".into()).spawn(move || loop {
            std::thread::sleep(Duration::from_millis(200));
            let Some(m) = me.upgrade() else { return };
            if m.opened.load(Ordering::SeqCst) != generation {
                return;
            }
            let (open, expired) = {
                let st = m.state.lock_or_recover();
                (st.open, st.expired(Instant::now()))
            };
            if !open {
                return;
            }
            if expired {
                m.input(Input::Close, "no input");
                return;
            }
        });
    }

    /// Save a profile or target choice through the app (never on the hotkey thread: saving re-claims hotkeys).
    fn save(self: &Arc<Self>, patch: Value, what: String) {
        let me = self.clone();
        let _ = std::thread::Builder::new().name("uc-au05-menu-save".into()).spawn(move || {
            let persist = me.persist.lock_or_recover().clone();
            let note = match persist.map(|p| p(patch)) {
                Some(Ok(())) => what,
                Some(Err(e)) => format!("{what}: {e}"),
                None => format!("{what}: not saved (no app)"),
            };
            me.log.line(&format!("menu saved: {note}"));
            *me.last.lock_or_recover() = Some(note);
        });
    }

    /// Run an action on its own thread. `from_menu`: wait a moment so the menu window has gone first.
    fn run(&self, action: Action, key: Option<KeyId>, from_menu: bool) {
        let target = self.settings()["target"].as_str().unwrap_or("claude").to_string();
        let action = action.resolve(&target);
        let dictate = matches!(action, Action::Entry(e) if e.effect == hotkey::Effect::DictateSend);
        // Dictate & send: one at a time, and a second press within 400 ms is ignored (debounce).
        if dictate {
            let mut last = self.dictate_last.lock_or_recover();
            if last.is_some_and(|t| t.elapsed() < Duration::from_millis(400)) {
                self.log.line("dictate & send: second press within 400 ms ignored");
                return;
            }
            if self.dictate_busy.swap(true, Ordering::SeqCst) {
                self.log.line("dictate & send: still running, press ignored");
                return;
            }
            *last = Some(Instant::now());
        }
        let busy = self.dictate_busy.clone();
        let desk = self.desktop.lock_or_recover().clone();
        let (status, log, last) = (self.status.clone(), self.log.clone(), self.last.clone());
        let _ = std::thread::Builder::new().name("uc-au05-action".into()).spawn(move || {
            if from_menu {
                std::thread::sleep(Duration::from_millis(150));
            }
            let result = desk
                .and_then(|d| d.upgrade())
                .ok_or_else(|| "Unified Control is closing".to_string())
                .and_then(|d| {
                    super::sync::guarded("Vibe Key action", || {
                        if dictate {
                            // Decided from Claude's real microphone state; logs what was seen and sent.
                            hotkey::dictate_send(&*d, hotkey::DictateTiming::default(), &|l| log.line(l)).map(|seen| {
                                log.line(&format!("dictate & send: {seen}"));
                            })
                        } else {
                            hotkey::run_action(&*d, &action, &|l| log.line(l))
                        }
                    })
                    .unwrap_or_else(|| Err("The action crashed".into()))
                });
            if dictate {
                busy.store(false, Ordering::SeqCst);
            }
            let note = match result {
                Ok(()) => format!("ran {}", action.label()),
                Err(e) => format!("{}: {e}", action.label()),
            };
            match key {
                Some(k) => {
                    log.line(&format!("hotkey {}: {note}", k.control().name()));
                    push_event(&status, k.control(), true, k.control().own_code().unwrap_or(0), Some(note.clone()));
                }
                None => log.line(&format!("menu: {note}")),
            }
            if from_menu {
                *last.lock_or_recover() = Some(note);
            }
        });
    }

    /// Open a Claude session, then (after it has had time to come forward) run the follow-up action.
    fn open_session(self: &Arc<Self>, id: String, then: String) {
        let Some(url) = super::claude_sessions::session_link(&id) else { return };
        let me = self.clone();
        let _ = std::thread::Builder::new().name("uc-au05-session".into()).spawn(move || {
            std::thread::sleep(Duration::from_millis(150)); // the menu window goes first
            let desk = me.desktop.lock_or_recover().clone().and_then(|d| d.upgrade());
            let result = desk.ok_or_else(|| "Unified Control is closing".to_string()).and_then(|d| d.open_link(&url));
            let note = match &result {
                Ok(()) => "Opened a Claude session".to_string(),
                Err(e) => format!("Claude session: {e}"),
            };
            me.log.line(&format!("menu: {note}"));
            *me.last.lock_or_recover() = Some(note);
            if result.is_ok() && then != "none" {
                if let Some(e) = hotkey::entry(&then) {
                    // Wait (at most 1.5 s) until Claude is in front with the session; the action itself then
                    // focuses Claude's message box first (or presses nothing).
                    let claude = hotkey::app("claude");
                    let desk = me.desktop.lock_or_recover().clone().and_then(|d| d.upgrade());
                    let end = Instant::now() + Duration::from_millis(1500);
                    let shown = loop {
                        let front = matches!((&desk, claude), (Some(d), Some(a)) if d.is_foreground(a));
                        if front || Instant::now() >= end {
                            break front;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    };
                    if shown {
                        std::thread::sleep(Duration::from_millis(150));
                        me.run(Action::Entry(e), None, false);
                    } else {
                        let note = "Claude did not show the session in time: nothing pressed".to_string();
                        me.log.line(&format!("menu: {note}"));
                        *me.last.lock_or_recover() = Some(note);
                    }
                }
            }
        });
    }

    /// A claimed hotkey arrived (on the hotkey thread: return quickly).
    pub fn hotkey(self: &Arc<Self>, vk: u16) {
        if vk == hotkey::VK_F23 || vk == hotkey::VK_F24 || KeyId::ALL.into_iter().any(|k| own_vk(k) == vk) {
            *self.key_seen.lock_or_recover() = Some(Instant::now());
        }
        let settings = self.settings();
        let open = self.is_open();
        if vk == hotkey::VK_F23 || vk == hotkey::VK_F24 {
            let right = vk == hotkey::VK_F23;
            if open {
                self.input(if right { Input::Next } else { Input::Prev }, "knob");
            } else {
                match settings["knobTurnAction"].as_str().unwrap_or("scroll") {
                    "scroll" => {
                        if let Some(d) = self.desktop.lock_or_recover().clone().and_then(|d| d.upgrade()) {
                            // One notch per click; turning right scrolls down, like the factory knob.
                            if let Err(e) = d.scroll(if right { -1 } else { 1 }) {
                                self.log.line(&format!("knob scroll: {e}"));
                            }
                        }
                    }
                    "volume" => {
                        let id = if right { "volumeUp" } else { "volumeDown" };
                        if let Some(e) = hotkey::entry(id) {
                            self.run(Action::Entry(e), None, false);
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        let Some(key) = KeyId::ALL.into_iter().find(|k| own_vk(*k) == vk) else { return };
        let name = key.control().name();
        // K1 on a highlighted Claude session (or project): dictate into it, with K1's own kind of dictation.
        if open && key == KeyId::K1 && menu::role(&settings["menuControls"], "k1") == "none" {
            let then = if settings["actions"]["k1"]["kind"] == "claudeDictateSend" { "claudeDictateSend" } else { "claudeDictation" };
            let shown = self.shown_menu(&settings);
            if self.state.lock_or_recover().highlighted_session(&shown).is_some() {
                return self.input(Input::Dictate(then.into()), name);
            }
        }
        // While the menu is open, a control with a menu role does only that (its normal action waits).
        if open {
            match menu::role(&settings["menuControls"], settings_key(key)) {
                "select" => return self.input(Input::Choose, name),
                "back" => return self.input(Input::Back, name),
                "close" => return self.input(Input::Close, name),
                "nextMenu" => return self.input(Input::NextMenu, name),
                _ => {}
            }
        }
        let action = saved_action(&settings["actions"], settings_key(key));
        match action {
            Action::QuickMenu => self.input(Input::Toggle, name),
            Action::Nothing => {
                // "Same as on its own" while it types its own code for the menu: type its offline keys.
                if settings["actions"][settings_key(key)]["kind"] == hotkey::OFFLINE {
                    if let Some(keys) = self.offline_keys(&settings, key) {
                        self.run(Action::Custom { keys, app: None }, Some(key), false);
                    }
                }
            }
            action => {
                if open {
                    self.input(Input::Close, "another key");
                }
                self.run(action, Some(key), false);
            }
        }
    }

    /// What a control types on its own, as keys UC can press (factory: the factory key), if UC can press it.
    fn offline_keys(&self, settings: &Value, key: KeyId) -> Option<String> {
        let name = settings_key(key);
        let o = &settings["offline"][name];
        let keys = if o["mode"] == "keys" { o["keys"].as_str().unwrap_or("").to_string() } else { key.factory_label().to_string() };
        let ok = super::actions::plan_action(&super::dock_runtime::PendingAction { kind: "hotkey".into(), value: keys.clone() }).is_ok();
        if !ok {
            self.log.line(&format!("{name}: its own typing cannot be pressed by UC"));
        }
        ok.then_some(keys)
    }

    pub fn status(&self) -> Value {
        let settings = self.settings();
        let shown = self.shown_menu(&settings);
        let view = self.state.lock_or_recover().view(&settings, &shown);
        let view = self.with_face(&settings, view);
        json!({
            "view": view,
            "faces": settings["menuFaces"],
            "entries": settings["menu"],
            "profiles": settings["profiles"].as_array().map(|ps| ps.iter().map(|p| json!({"id": p["id"], "name": p["name"]})).collect::<Vec<_>>()),
            "profile": settings["profile"],
            "target": settings["target"],
            "targets": TARGETS.iter().map(|t| json!({"id": t, "name": menu::target_name(t)})).collect::<Vec<_>>(),
            "knobTurn": settings["knobTurn"],
            "knobTurnAction": settings["knobTurnAction"],
            "controls": settings["menuControls"],
            "reopenWhereLeft": settings["reopenWhereLeft"] != false,
            "sessionFoldHours": settings["sessionFoldHours"].as_u64().unwrap_or(SESSION_FOLD_DEFAULT_H),
            "sessions": self.session_source.lock_or_recover().clone(),
            "closeAfterS": menu::CLOSE_AFTER.as_secs(),
            "windowAvailable": self.view.lock_or_recover().is_some(),
            "last": self.last.lock_or_recover().clone(),
        })
    }
}

/// A short, safe name for a request (no raw bytes).
pub fn describe(r: &Request) -> String {
    match r {
        Request::GetBattery => "get battery".into(),
        Request::GetStandbyStatus => "get standby status".into(),
        Request::GetStandbyTime => "get standby time".into(),
        Request::SetStandbyTime(s) => format!("set standby time {s} s"),
        Request::GetSleepTime => "get power-off time".into(),
        Request::SetSleepTime(t) => format!("set power-off time {} s", t.seconds()),
        Request::GetControlCode(i) => format!("get control code {i}"),
        Request::GetKeyTyping(k) => format!("get key typing {}", k.control().name()),
        Request::SetKeyTyping(k, t) => format!("set key typing {} ({} entries)", k.control().name(), t.0.len()),
        Request::GetLight => "get light".into(),
        Request::SetLightMode(m) => format!("set light mode {m}"),
        Request::SetLightBrightness(b) => format!("set light brightness {b}"),
        Request::SetLightRecord { record, field, value } => format!("set light record {record} {field:?} {value}"),
        Request::GetHooksMode => "get hooks mode".into(),
        Request::SetHooksMode(m) => format!("set hooks mode {m}"),
        Request::GetMicNr => "get mic noise reduction".into(),
        Request::SetMicNr(m) => format!("set mic noise reduction level {}", m.level),
    }
}

/// A background job that sets what one key types. Choosing an action never waits for the key: the job
/// waits until the key is awake, then sends one set (one key-typing command at a time).
#[derive(Clone, Debug)]
struct KeyJob {
    typing: KeyTyping,
    /// "waiting", "sending", "done" or "failed".
    state: &'static str,
    message: String,
    /// A newer choice made while this one was being sent: it is sent next (still one at a time).
    next: Option<KeyTyping>,
    /// Read what the key holds (before writing a list that is not known).
    read: bool,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Au05 {
    /// Creating the worker does no device I/O and claims no hotkey; nothing starts until `configure`.
    /// `real_desktop` is used only for a real key. With `fake`, a [`FakeDesktop`] is used.
    pub fn new(fake: bool, real_desktop: Option<Arc<dyn Desktop>>) -> Arc<Self> {
        Self::with_config(fake, real_desktop, uc_session_config())
    }
    pub fn with_config(fake: bool, real_desktop: Option<Arc<dyn Desktop>>, mut config: SessionConfig) -> Arc<Self> {
        // A real key: the session itself refuses commands not checked on the real key.
        config.real_key = !fake;
        let fake_desktop = fake.then(|| Arc::new(FakeDesktop::default()));
        let desktop: Option<Arc<dyn Desktop>> = match &fake_desktop {
            Some(f) => Some(f.clone()),
            None => real_desktop,
        };
        let status = Arc::new(Mutex::new(Status {
            enabled: false,
            state: "disabled",
            message: "Vibe Key is disabled".into(),
            known: Known::default(),
            events: VecDeque::new(),
            last_error: None,
            pending_sets: 0,
            stalls: 0,
            sent: 0,
            generation: 0,
            jobs: Default::default(),
            sync: SyncState::default(),
            reads_failed: false,
            changes: BTreeMap::new(),
            strict_until: None,
        }));
        let log = Arc::new(Log::default());
        let menu = Arc::new(MenuCtl {
            state: Mutex::new(MenuState::default()),
            settings: Mutex::new(default_settings()),
            view: Mutex::new(None),
            persist: Mutex::new(None),
            desktop: Mutex::new(desktop.as_ref().map(Arc::downgrade)),
            log: log.clone(),
            status: status.clone(),
            opened: AtomicU64::new(0),
            last: Arc::new(Mutex::new(None)),
            sessions: Mutex::new(None),
            key_seen: Mutex::new(None),
            // Fake and quiet (test) instances read only UC_CLAUDE_DATA.
            claude_data: Mutex::new(super::claude_tree::base_dir(
                fake || std::env::var("UC_TEST_QUIET").is_ok_and(|v| !v.is_empty() && v != "0"),
            )),
            frozen: Mutex::new(None),
            face: Mutex::new(0),
            session_source: Mutex::new(Value::Null),
            dictate_busy: Arc::new(AtomicBool::new(false)),
            dictate_last: Mutex::new(None),
        });
        let au = Arc::new(Self {
            fake: fake.then(FakeHandle::default),
            status: status.clone(),
            actions: Arc::new(Mutex::new(default_settings()["actions"].clone())),
            tx: Mutex::new(None),
            thread: Mutex::new(None),
            stop: Arc::new(AtomicBool::new(false)),
            desktop,
            fake_desktop,
            hotkeys: Mutex::new(BTreeMap::new()),
            hotkey_lines: Mutex::new(Vec::new()),
            config_lock: Mutex::new(()),
            config,
            log,
            menu: menu.clone(),
            layers: Arc::new(Mutex::new(Layers::from_settings(&default_settings()))),
            offline_result: Mutex::new(None),
            timing: Mutex::new((Duration::from_secs(8), Duration::from_secs(20))),
            gate: Mutex::new((STRICT_WINDOW, TOUCH_WAIT)),
            configured: AtomicBool::new(false),
            mangle_echo: Arc::new(AtomicBool::new(false)),
            exiting: Arc::new(AtomicBool::new(false)),
        });
        if let Some(d) = &au.desktop {
            // Actions never run on the hotkey thread (MenuCtl::run spawns one per action).
            d.set_handler(Arc::new(move |vk| menu.hotkey(vk)));
        }
        au
    }

    /// Called when Windows ends the session (only the real desktop reports it).
    pub fn on_session_end(&self, handler: Arc<dyn Fn() + Send + Sync>) {
        if let Some(d) = &self.desktop {
            d.set_session_end(handler);
        }
    }
    /// The app shell's menu window. Never set in tests, fake or quiet instances.
    pub fn set_menu_view(&self, view: Arc<dyn MenuView>) {
        *self.menu.view.lock_or_recover() = Some(view);
    }
    /// Tests: the Claude desktop app's data folder (`claude_tree`).
    pub fn set_claude_data(&self, dir: Option<std::path::PathBuf>) {
        *self.menu.claude_data.lock_or_recover() = dir;
    }
    /// Where the quick menu gets the Claude session list.
    pub fn set_sessions(&self, f: SessionsFn) {
        *self.menu.sessions.lock_or_recover() = Some(f);
    }
    /// How a profile or target chosen in the menu is saved (through the app's settings).
    pub fn set_persist(&self, persist: Persist) {
        *self.menu.persist.lock_or_recover() = Some(persist);
    }
    /// Input from the menu window or the page: `state`, `next`, `prev`, `choose`, `back`, `close`,
    /// `toggle`, `pick` (with `index`). Returns the menu's view.
    pub fn menu_input(&self, op: &str, index: Option<usize>) -> Result<Value, String> {
        let input = match op {
            "state" => None,
            "next" => Some(Input::Next),
            "prev" => Some(Input::Prev),
            "choose" => Some(Input::Choose),
            "back" => Some(Input::Back),
            "close" => Some(Input::Close),
            "nextMenu" => Some(Input::NextMenu),
            "toggle" => Some(Input::Toggle),
            "pick" => Some(Input::Pick(index.ok_or("Choose a row")?)),
            "hold" => Some(Input::Hold),
            _ => return Err("Unknown menu input".into()),
        };
        if let Some(i) = input {
            if i == Input::Toggle && !self.status.lock_or_recover().enabled {
                return Err("Enable the Vibe Key first".into());
            }
            self.menu.input(i, "window");
        }
        Ok(self.menu.status()["view"].clone())
    }

    /// One line in the Vibe Key log (for the app layer: page action choices).
    pub fn log_line(&self, msg: &str) {
        self.log.line(msg);
    }

    /// Write the log to this folder (`devices/au05`). Without it nothing is logged (tests).
    pub fn set_log_dir(&self, dir: std::path::PathBuf) {
        *self.log.0.lock_or_recover() = Some(dir);
    }

    pub fn fake_handle(&self) -> Option<&FakeHandle> {
        self.fake.as_ref()
    }
    pub fn fake_desktop(&self) -> Option<&Arc<FakeDesktop>> {
        self.fake_desktop.as_ref()
    }

    /// Claim exactly the hotkeys whose key has an action while enabled; release all otherwise.
    fn apply_hotkeys(&self, enabled: bool) {
        let Some(d) = &self.desktop else {
            self.hotkeys.lock_or_recover().clear();
            return;
        };
        let actions = self.actions.lock_or_recover().clone();
        let settings = self.menu.settings();
        let mut wanted: Vec<u16> = if enabled {
            ACTION_KEYS
                .iter()
                .filter(|(k, _)| saved_action(&actions, k) != Action::Nothing)
                .map(|(_, id)| own_vk(*id))
                .collect()
        } else {
            Vec::new()
        };
        let any_menu = ACTION_KEYS.iter().any(|(k, _)| saved_action(&actions, k) == Action::QuickMenu);
        // Keys with a menu role (default: K2 select, K3 back, knob press select) are claimed while a key opens
        // the menu, even with no action of their own: their own code reaches UC, which restores their normal
        // typing while the menu is closed.
        if enabled && any_menu {
            for (k, id) in ACTION_KEYS {
                if (id == KeyId::KnobPress || menu::role(&settings["menuControls"], k) != "none") && !wanted.contains(&own_vk(id)) {
                    wanted.push(own_vk(id));
                }
            }
        }
        // The knob types F23/F24: they move the menu, or run the knob turn action (UC's own scrolling).
        if enabled && settings["knobTurn"] == "uc" {
            wanted.extend([hotkey::VK_F23, hotkey::VK_F24]);
        }
        let results = d.set_hotkeys(&wanted);
        let mut lines = Vec::new();
        for (key, id) in ACTION_KEYS {
            let vk = own_vk(id);
            let what = match saved_action(&actions, key) {
                Action::Nothing if any_menu => format!("menu: {}", menu::role(&settings["menuControls"], key)),
                a => a.label(),
            };
            let line = match results.get(&vk) {
                Some(Ok(())) => format!("hotkey {key} claimed ({what})"),
                Some(Err(e)) => format!("hotkey {key} not claimed: {e}"),
                None => continue,
            };
            lines.push(line);
        }
        for (vk, name) in [(hotkey::VK_F23, "knob right F23"), (hotkey::VK_F24, "knob left F24")] {
            match results.get(&vk) {
                Some(Ok(())) => lines.push(format!("hotkey {name} claimed")),
                Some(Err(e)) => lines.push(format!("hotkey {name} not claimed: {e}")),
                None => {}
            }
        }
        if wanted.is_empty() {
            lines.push("hotkeys released".into());
        }
        // Logged only when something changed (every settings save re-applies the hotkeys).
        let mut last = self.hotkey_lines.lock_or_recover();
        if *last != lines {
            for line in &lines {
                self.log.line(line);
            }
            *last = lines;
        }
        drop(last);
        *self.hotkeys.lock_or_recover() = results;
    }

    /// Apply the saved settings: start or stop the worker, take the UC actions and claim their hotkeys.
    pub fn configure(&self, settings: &Value) {
        let _guard = self.config_lock.lock_or_recover();
        let settings = sanitize(settings);
        *self.actions.lock_or_recover() = settings["actions"].clone();
        *self.menu.settings.lock_or_recover() = settings.clone();
        let enabled = settings["enabled"].as_bool().unwrap_or(false);
        if !enabled && self.menu.is_open() {
            self.menu.input(Input::Close, "disabled");
        }
        {
            // New layers. A control whose layer changed in this run is marked (its unknown list is read, then
            // written if it differs); the first configure of a run marks nothing.
            let first = !self.configured.swap(true, Ordering::SeqCst);
            let mut l = self.layers.lock_or_recover();
            let mut next = Layers::from_settings(&settings);
            next.target_offline = l.target_offline;
            for i in 0..6 {
                // A seed learned in this run but not saved yet stays.
                if !next.seeded[i] {
                    if let Some(seed) = l.seeds[i].clone() {
                        next.seed(i, &seed);
                    }
                }
                next.dirty[i] = !first && (l.dirty[i] || next.live[i] != l.live[i] || next.offline[i] != l.offline[i]);
            }
            *l = next;
        }
        // An earlier run gave up waiting for a key-typing command: wait before sending the next one.
        if let Some(at) = settings["slowAbandonedAt"].as_u64() {
            let wait = self.timing.lock_or_recover().1;
            let since = Duration::from_millis(now_ms().saturating_sub(at));
            if since < wait {
                let mut s = self.status.lock_or_recover();
                let until = Instant::now() + (wait - since);
                if s.sync.not_before.is_none_or(|t| t < until) {
                    s.sync.not_before = Some(until);
                    drop(s);
                    self.log.line(&format!("key typing waits {} ms after a command an earlier run left unanswered", (wait - since).as_millis()));
                }
            }
        }
        let running = self.tx.lock_or_recover().is_some();
        if enabled && !running && self.exiting.load(Ordering::SeqCst) {
            self.log.line("not started: the app is exiting");
        } else if enabled && !running {
            self.layers.lock_or_recover().target_offline = false;
            self.start();
        } else if !enabled && running {
            // Disabling: the key gets its offline layer back first (if it is awake), then the worker stops.
            let budget = self.timing.lock_or_recover().0;
            let ok = self.store_offline(budget);
            *self.offline_result.lock_or_recover() = Some(ok);
            self.stop_worker();
        }
        self.apply_hotkeys(enabled);
    }

    fn start(&self) {
        let (tx, rx) = mpsc::channel();
        self.stop.store(false, Ordering::SeqCst);
        {
            let mut s = self.status.lock_or_recover();
            s.enabled = true;
            s.state = "starting";
            s.message = "Looking for the Vibe Key receiver".into();
            s.generation += 1;
        }
        self.log.line("worker start");
        let (strict, touch_wait) = *self.gate.lock_or_recover();
        let ctx = Worker {
            fake: self.fake.clone(),
            status: self.status.clone(),
            stop: self.stop.clone(),
            config: self.config.clone(),
            log: self.log.clone(),
            menu: self.menu.clone(),
            layers: self.layers.clone(),
            mangle_echo: self.mangle_echo.clone(),
            exiting: self.exiting.clone(),
            quiet_from: Mutex::new(None),
            strict: strict.min(self.config.awake_window),
            touch_wait,
        };
        let rx = Arc::new(Mutex::new(rx));
        let handle = std::thread::Builder::new().name("uc-au05".into()).spawn(move || {
            let stop = ctx.stop.clone();
            restarting("Vibe Key worker", || stop.load(Ordering::SeqCst), || ctx.run(&rx));
        });
        match handle {
            Ok(h) => {
                *self.tx.lock_or_recover() = Some(tx);
                *self.thread.lock_or_recover() = Some(h);
            }
            Err(_) => {
                let mut s = self.status.lock_or_recover();
                s.state = "error";
                s.message = "The Vibe Key worker could not start".into();
            }
        }
    }

    fn stop_worker(&self) {
        {
            let mut s = self.status.lock_or_recover();
            if s.sync.pending && s.sync.abandoned_at.is_none() {
                s.sync.abandoned_at = Some(now_ms());
                drop(s);
                self.log.line("key typing: a command was still waiting for its answer when the worker stopped");
            }
        }
        self.stop.store(true, Ordering::SeqCst);
        if let Some(tx) = self.tx.lock_or_recover().take() {
            let _ = tx.send(Cmd::Stop);
        }
        if let Some(h) = self.thread.lock_or_recover().take() {
            // Bounded: every device call inside has a time limit of at most 2 s.
            let _ = h.join();
        }
        let mut s = self.status.lock_or_recover();
        if s.enabled {
            self.log.line("worker stop");
        }
        s.enabled = false;
        s.state = "disabled";
        s.message = "Vibe Key is disabled".into();
        s.pending_sets = 0;
        s.jobs = Default::default();
        // What the key holds is unknown once the worker stops (it may change while UC is not listening).
        s.known.keys = Default::default();
        let abandoned = s.sync.abandoned_at;
        s.sync = SyncState { abandoned_at: abandoned, ..Default::default() };
    }

    /// The app kept running after a Windows session end: key-typing commands may start again.
    pub fn resume_after_session_end(&self) {
        let mut s = self.status.lock_or_recover();
        if s.sync.no_new {
            s.sync.no_new = false;
            drop(s);
            self.log.line("session end did not end Unified Control: key changes may start again");
        }
    }

    /// Is the worker running (the Vibe Key enabled)?
    pub fn running(&self) -> bool {
        self.tx.lock_or_recover().is_some()
    }

    /// When this run abandoned a key-typing command, once (the app saves `slowAbandonedAt`).
    pub fn take_abandoned(&self) -> Option<u64> {
        self.status.lock_or_recover().sync.abandoned_at.take()
    }

    /// Tests: the quit/disable budget and the wait after an abandoned key-typing command.
    pub fn set_timing(&self, store: Duration, abandoned_wait: Duration) {
        *self.timing.lock_or_recover() = (store, abandoned_wait);
    }

    /// Write the offline layer back, then stop the worker and release every hotkey (quit). Returns whether
    /// the offline layer is on the key (`None` when the Vibe Key was not running).
    pub fn shutdown(&self) -> Option<bool> {
        self.shutdown_within(None)
    }

    /// `shutdown` with the store capped at `budget` (`None`: the quit budget).
    pub fn shutdown_within(&self, budget: Option<Duration>) -> Option<bool> {
        // Before the lock: a settings save waiting for it must not start the worker again.
        self.exiting.store(true, Ordering::SeqCst);
        let _guard = self.config_lock.lock_or_recover();
        if self.menu.is_open() {
            self.menu.input(Input::Close, "quit");
        }
        let running = self.tx.lock_or_recover().is_some();
        let quit = self.timing.lock_or_recover().0;
        let budget = budget.map_or(quit, |b| b.min(quit));
        let stored = running.then(|| self.store_offline(budget));
        self.stop_worker();
        self.apply_hotkeys(false);
        stored
    }

    /// The result of the offline store at the last disable, once (the app saves `offlinePending`).
    pub fn take_offline_result(&self) -> Option<bool> {
        self.offline_result.lock_or_recover().take()
    }

    /// The key should hold its offline layer (`true`) or the live layer. "Store offline layer on the key
    /// now" on the page; the live layer comes back with "Back to Unified Control" or the next start.
    pub fn set_layer_offline(&self, offline: bool) {
        self.layers.lock_or_recover().target_offline = offline;
        self.log.line(if offline { "layer: storing the offline layer on the key" } else { "layer: back to the live layer" });
    }

    /// Does the key hold the offline layer? Every known list equals its offline typing, no key job waits or is
    /// being sent, no key-typing command is pending, and no earlier write is in doubt. Unknown lists count as
    /// stored (UC did not change them in this run).
    fn offline_on_key(&self) -> bool {
        let s = self.status.lock_or_recover();
        let l = self.layers.lock_or_recover();
        !s.sync.pending
            && !s.sync.uncertain.iter().any(|u| *u)
            && !s.jobs.iter().flatten().any(|j| j.state == "waiting" || j.state == "sending")
            && LAYER_CONTROLS.iter().enumerate().all(|(i, (_, id))| {
                s.known.keys[id.index() as usize].as_ref().is_none_or(|k| *k == l.offline[i])
            })
    }

    /// Switch the key to its offline layer and wait (at most `budget`) until it is stored. Only an awake key
    /// is written. A key-typing command already sent is always waited for within the budget; when the budget
    /// runs out no new one is started, and one still unanswered is recorded (the next run waits before its
    /// first key-typing command). Returns `false` when the offline layer is not (known to be) on the key.
    pub fn store_offline(&self, budget: Duration) -> bool {
        self.layers.lock_or_recover().target_offline = true;
        let end = Instant::now() + budget;
        loop {
            if self.offline_on_key() {
                self.log.line("layer: offline layer is on the key");
                return true;
            }
            let (state, pending, blocked) = {
                let s = self.status.lock_or_recover();
                (s.state, s.sync.pending, s.sync.blocked)
            };
            if !pending && (state != "connected" || blocked) {
                self.log.line(&format!("layer: offline layer not stored ({})", if blocked { "a key change failed" } else { "key not awake" }));
                return false;
            }
            if Instant::now() >= end {
                let mut s = self.status.lock_or_recover();
                s.sync.no_new = true;
                if s.sync.pending {
                    s.sync.abandoned_at = Some(now_ms());
                }
                drop(s);
                self.log.line(&format!("layer: offline layer not stored (no time left{})", if pending { "; a key command is still unanswered" } else { "" }));
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// A settings change is busy until the key answers or the request fails.
    pub fn busy(&self) -> bool {
        self.status.lock_or_recover().pending_sets > 0
    }

    /// Send one change to the key and wait for its answer (at most `SET_WAIT`).
    pub fn change(&self, change: Change) -> Result<(), String> {
        if let Change::Retry(key) = change {
            // "Try again" / "Set up": one more automatic attempt for this control (and the guard after a failed
            // key change is lifted); "Set up" also allows a control not checked on the real key.
            let i = key.index() as usize;
            let mut s = self.status.lock_or_recover();
            if s.jobs[i].as_ref().is_some_and(|j| j.state == "check") {
                s.sync.allowed[i] = true;
            }
            if s.jobs[i].as_ref().is_some_and(|j| j.state == "failed" || j.state == "check") {
                s.jobs[i] = None;
            }
            s.sync.blocked = false;
            s.sync.attempted[i] = None;
            s.sync.read_tried[i] = false;
            drop(s);
            self.layers.lock_or_recover().dirty[i] = true;
            self.log.line(&format!("page try again {}", key.control().name()));
            return Ok(());
        }
        self.log.line(&format!("page change {change:?}"));
        let tx = self.tx.lock_or_recover().clone().ok_or("Enable the Vibe Key first")?;
        let (rtx, rrx) = mpsc::channel();
        tx.send(Cmd::Change(change, rtx)).map_err(|_| "The Vibe Key worker is unavailable")?;
        rrx.recv_timeout(SET_WAIT + Duration::from_secs(5))
            .map_err(|_| "The Vibe Key did not answer in time".to_string())?
    }

    /// Queue every get except key typing, one at a time with the usual pacing. Returns once queued.
    pub fn read_all(&self) -> Result<(), String> {
        let tx = self.tx.lock_or_recover().clone().ok_or("Enable the Vibe Key first")?;
        let (rtx, rrx) = mpsc::channel();
        tx.send(Cmd::ReadAll(rtx)).map_err(|_| "The Vibe Key worker is unavailable")?;
        rrx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| "The Vibe Key worker did not answer".to_string())?
    }

    /// Read one key's typing list from the key (one at a time; the reply can take up to 20 s).
    pub fn read_key(&self, key: KeyId) -> Result<(), String> {
        let tx = self.tx.lock_or_recover().clone().ok_or("Enable the Vibe Key first")?;
        let (rtx, rrx) = mpsc::channel();
        tx.send(Cmd::ReadKey(key, rtx)).map_err(|_| "The Vibe Key worker is unavailable")?;
        rrx.recv_timeout(SET_WAIT + Duration::from_secs(5))
            .map_err(|_| "The Vibe Key did not answer in time".to_string())?
    }

    /// Tests: the strict send window and the "touch to apply" wait (set before enabling).
    pub fn set_gate_timing(&self, strict: Duration, touch_wait: Duration) {
        *self.gate.lock_or_recover() = (strict, touch_wait);
    }

    /// Clear a pause after slow replies and read everything again (the page's Reconnect button).
    pub fn reconnect(&self) -> Result<(), String> {
        let tx = self.tx.lock_or_recover().clone().ok_or("Enable the Vibe Key first")?;
        tx.send(Cmd::Reconnect).map_err(|_| "The Vibe Key worker is unavailable".to_string())
    }

    /// The hotkeys UC holds now (the Input tester marks these key presses: other apps never get them).
    pub fn claimed_hotkeys(&self) -> Vec<u16> {
        self.hotkeys.lock_or_recover().iter().filter(|(_, r)| r.is_ok()).map(|(vk, _)| *vk).collect()
    }
    pub fn status(&self) -> Value {
        let s = self.status.lock_or_recover().clone();
        let k = &s.known;
        let hotkeys = self.hotkeys.lock_or_recover().clone();
        let layers = self.layers.lock_or_recover().clone();
        let hotkey_state = |vk: u16| match hotkeys.get(&vk) {
            None => json!({"state": "off"}),
            Some(Ok(())) => json!({"state": "ready"}),
            Some(Err(e)) => json!({"state": "failed", "message": e}),
        };
        let fake = self.fake.is_some();
        let keys: Vec<Value> = KeyId::ALL
            .iter()
            .map(|&id| {
                let t = k.keys[id.index() as usize].as_ref();
                let mode = match t {
                    None => "unknown",
                    Some(t) if t.is_factory() => "factory",
                    Some(t) if *t == KeyTyping::own_code(id) => "uc",
                    Some(_) => "keys",
                };
                let hotkey = match hotkeys.get(&own_vk(id)) {
                    None => json!({"state": "off"}),
                    Some(Ok(())) => json!({"state": "ready"}),
                    Some(Err(e)) => json!({"state": "failed", "message": e}),
                };
                json!({
                    "key": settings_key(id),
                    "name": id.control().name(),
                    "mode": mode,
                    "label": t.map(|t| t.label(id)),
                    "keys": t.filter(|t| !t.is_factory()).map(|t| t.label(id)),
                    "factory": id.factory_label(),
                    "ownCode": protocol::usage_name(id.control().own_code().unwrap_or(0)),
                    "hotkey": hotkey,
                    "job": s.jobs[id.index() as usize].as_ref().map(|j| {
                        let message = match (j.state, s.state) {
                            ("waiting", "connected") if j.read => "Waiting to read what the key holds".to_string(),
                            ("waiting", "not_found") => "Waiting for the receiver: plug it in".to_string(),
                            ("waiting", "paused") => "Waiting: sending is paused after a slow reply".to_string(),
                            ("waiting", "suspended") => "Stopped after slow replies: use Reconnect".to_string(),
                            ("waiting", "off") => "Waiting: the key seems switched off. Hold its power button 2–3 s".to_string(),
                            ("waiting", _) => "Waiting for the key: press any key to wake it".to_string(),
                            ("sending", _) => "Sending to the key (it can take up to 20 seconds)".to_string(),
                            ("done", _) => "Done".to_string(),
                            _ => j.message.clone(),
                        };
                        json!({"state": j.state, "message": message, "label": j.typing.label(id), "read": j.read})
                    }),
                    // Every key can be set; the knob press typing is not checked on the real key yet.
                    "canSet": true,
                    "uc": layers.uc[id.index() as usize],
                    "offline": layers.offline[id.index() as usize].label(id),
                    "checked": Request::GetKeyTyping(id).verified_on_hardware(),
                })
            })
            .collect();
        let events: Vec<Value> = s
            .events
            .iter()
            .rev()
            .map(|e| {
                json!({"at": e.at_ms, "control": e.control.name(), "pressed": e.pressed,
                       "code": protocol::usage_name(e.code), "action": e.action})
            })
            .collect();
        json!({
            "enabled": s.enabled,
            "fake": fake,
            "state": s.state,
            "message": s.message,
            // Charging is not known yet (the flag bits are not mapped): null until verified on the real key.
            // Bit 0x08 of the flags: probably charging (real key 2026-11-17: 0x08 at 30 % while most likely
            // charging, 0x00 full and unplugged). Not confirmed yet: the page says "probably".
            "battery": k.battery.map(|(mv, pct)| json!({"millivolts": mv, "percent": pct, "flags": k.battery_flags,
                "at": k.battery_at, "charging": k.battery_flags.map(|f| f & 0x08 != 0), "chargingConfirmed": false,
                "full": pct >= 100})),
            "standbyS": k.standby_s,
            "sleepS": k.sleep_s,
            "sleepChoices": SleepTime::CHOICES,
            "standbyRange": [protocol::STANDBY_MIN_S, protocol::STANDBY_MAX_S],
            "light": k.light.map(|l| json!({"mode": l.mode, "brightness": l.brightness, "records": l.records})),
            "lightRange": {"modeMax": protocol::LIGHT_MODE_MAX, "brightnessMax": protocol::LIGHT_BRIGHTNESS_MAX},
            "hooksMode": k.hooks_mode,
            "micNr": k.mic.map(|m| json!({"level": m.level, "a": m.a, "b": m.b})),
            "standbyStatus": k.standby_status,
            "controlCodes": k.codes.iter().map(|c| c.map(protocol::usage_name)).collect::<Vec<_>>(),
            "controlCodesRaw": k.codes,
            "lightFieldMax": LIGHT_FIELDS.iter().map(|(n, f)| (n.to_string(), json!(f.max()))).collect::<serde_json::Map<_, _>>(),
            "micNrMax": protocol::MIC_NR_LEVEL_MAX,
            "keys": keys,
            // The knob turn directions: what each types (read or set; not part of the reads at connect) and its job.
            "turns": KeyId::TURNS.iter().map(|&id| {
                let t = k.keys[id.index() as usize].as_ref();
                json!({
                    "key": settings_key(id),
                    "name": id.control().name(),
                    "mode": match t { None => "unknown", Some(t) if t.is_factory() => "factory",
                                      Some(t) if *t == KeyTyping::own_code(id) => "uc", Some(_) => "keys" },
                    "label": t.map(|t| t.label(id)),
                    "ownCode": protocol::usage_name(id.control().own_code().unwrap_or(0)),
                    "job": s.jobs[id.index() as usize].as_ref().map(|j| json!({"state": j.state,
                        "message": if j.state == "failed" || j.state == "check" { j.message.clone() } else if j.state == "done" { "Done".into() }
                                   else if j.state == "sending" { "Sending to the key".into() }
                                   else { "Waiting for the key: press any key to wake it".to_string() }})),
                    "checked": Request::SetKeyTyping(id, KeyTyping::own_code(id)).verified_on_hardware(),
                })
            }).collect::<Vec<_>>(),
            "events": events,
            // Key-typing guard: paused after a failed key change (until Reconnect or Try again), or stopped at quit.
            "keySync": {"blocked": s.sync.blocked, "pending": s.sync.pending, "waiting": s.sync.not_before.is_some_and(|t| t > Instant::now())},
            // Two layers: what the key should hold now, and whether the offline layer is on it.
            "layer": {"target": if layers.target_offline { "offline" } else { "live" },
                      "offlineOnKey": !s.sync.pending && !s.jobs.iter().flatten().any(|j| j.state == "waiting" || j.state == "sending")
                          && LAYER_CONTROLS.iter().enumerate().all(|(i, (_, id))| k.keys[id.index() as usize].as_ref().is_none_or(|t| *t == layers.offline[i])),
                      "offline": LAYER_CONTROLS.iter().enumerate().map(|(i, (n, id))| (n.to_string(), json!({"label": layers.offline[i].label(*id), "factory": layers.offline[i].is_factory(),
                          "keys": (!layers.offline[i].is_factory()).then(|| layers.offline[i].label(*id))}))).collect::<serde_json::Map<_, _>>()},
            "actions": self.actions.lock_or_recover().clone(),
            "hotkeysAvailable": self.desktop.is_some(),
            "catalogue": hotkey::catalogue_json(),
            "menu": self.menu.status(),
            "menuHotkeys": {
                "choose": hotkey_state(hotkey::VK_F19),
                "knobRight": hotkey_state(hotkey::VK_F23),
                "knobLeft": hotkey_state(hotkey::VK_F24),
            },
            "lastError": s.last_error,
            "busy": s.pending_sets > 0,
            // Which settings were checked on the real key; the page labels the others "Untested".
            "checked": {
                "standbyTime": Request::SetStandbyTime(600).verified_on_hardware(),
                "keyTyping": Request::SetKeyTyping(KeyId::K1, KeyTyping::factory()).verified_on_hardware(),
                "sleepTime": Request::SetSleepTime(SleepTime::new(0).unwrap()).verified_on_hardware(),
                // Checked by the user on the real key 2026-11-17 (0 off, 1 all, 2 dial, 3 keys).
                "lightMode": true,
                "lightBrightness": Request::SetLightBrightness(0).verified_on_hardware(),
                "lightRecord": Request::SetLightRecord { record: 0, field: LightField::Type, value: 0 }.verified_on_hardware(),
                "hooksMode": Request::SetHooksMode(0).verified_on_hardware(),
                "micNr": Request::SetMicNr(MicNr { level: 0, a: 0, b: 0 }).verified_on_hardware(),
            },
            "diagnostics": {"sent": s.sent, "stalls": s.stalls, "generation": s.generation},
            "readsFailed": s.reads_failed.then_some(READS_FAILED_MESSAGE),
            "changes": s.changes,
            "strictSending": s.strict_until.is_some_and(|t| Instant::now() < t),
        })
    }

    /// Debug fake instances only: drive the fake key.
    pub fn test_control(&self, v: &Value) -> Result<(), String> {
        let fake = self.fake.as_ref().ok_or("Test controls need the fake key")?;
        match v["op"].as_str().unwrap_or("") {
            "click" => {
                let name = v["control"].as_str().unwrap_or("");
                let control = CONTROLS.iter().find(|(k, _)| *k == name).ok_or("Unknown control")?.1;
                fake.click(control);
                // If the fake key typed its own code, deliver it to the fake hotkey layer, as Windows would.
                if let Some(id) = KeyId::EVERY.into_iter().find(|k| k.control() == control) {
                    let own = fake.lock().typing[id.index() as usize] == KeyTyping::own_code(id);
                    if let (true, Some(d)) = (own, &self.fake_desktop) {
                        d.press(own_vk(id));
                    }
                }
            }
            "wake" => fake.wake(),
            "sleep" => fake.sleep(),
            "mangleEcho" => self.mangle_echo.store(v["value"].as_bool().unwrap_or(false), Ordering::SeqCst),
            "stallWrites" => fake.lock().stall_writes = v["value"].as_bool().unwrap_or(false),
            "dropReplies" => fake.lock().drop_replies = v["value"].as_u64().unwrap_or(0).min(10) as u32,
            "battery" => fake.lock().percent = v["value"].as_u64().unwrap_or(30).min(100) as u8,
            // Dictate & send on the fake desktop: Claude capturing or not, and Ctrl + D doing nothing.
            "claudeCapture" | "ctrlDIgnored" => {
                let d = self.fake_desktop.as_ref().ok_or("No fake desktop")?;
                let flag = if v["op"] == "claudeCapture" { &d.capture } else { &d.ctrl_d_ignored };
                flag.store(v["value"].as_bool().unwrap_or(false), Ordering::SeqCst);
            }
            "actionCrash" => {
                let d = self.fake_desktop.as_ref().ok_or("No fake desktop")?;
                d.crash.store(v["value"].as_bool().unwrap_or(false), Ordering::SeqCst);
            }
            "hotkeyBusy" => {
                // Another app holds this key's code: the next claim fails.
                let d = self.fake_desktop.as_ref().ok_or("No fake desktop")?;
                let key = ACTION_KEYS.iter().find(|(k, _)| Some(*k) == v["control"].as_str()).ok_or("Unknown key")?.1;
                d.busy.lock_or_recover().push(own_vk(key));
            }
            _ => return Err("Unknown test operation".into()),
        }
        Ok(())
    }
    /// Debug fake instances only: what the fake key received and typed, and what the fake desktop did.
    pub fn test_state(&self) -> Option<Value> {
        let f = self.fake.as_ref()?;
        let s = f.lock();
        let hex = |v: &Vec<u8>| v.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        Some(json!({
            "received": s.received.iter().map(hex).collect::<Vec<_>>(),
            "forbidden": s.forbidden.iter().map(hex).collect::<Vec<_>>(),
            "typed": s.typed,
            "awake": s.awake,
            "hotkeysClaimed": self.fake_desktop.as_ref().map(|d| d.claimed()),
            "actionsRun": self.fake_desktop.as_ref().map(|d| d.ran.lock_or_recover().clone()),
        }))
    }
}

/// UC's timings: the driver defaults, with a shorter wait for a sleeping key so the page answers sooner.
pub fn uc_session_config() -> SessionConfig {
    SessionConfig {
        request_expiry: Duration::from_secs(20),
        // 0.37.0: the key is silent when idle, so the driver's own gate (a message within this window) is
        // wide; UC decides when to send (`Worker::gate`): within the key's standby time since its last input,
        // or the strict 11 s rule after a stall. Reads and key typing keep the strict rule.
        awake_window: Duration::from_secs(3600),
        ..SessionConfig::default()
    }
}

struct Worker {
    layers: Arc<Mutex<Layers>>,
    mangle_echo: Arc<AtomicBool>,
    exiting: Arc<AtomicBool>,
    /// When the key last woke or sent a control event: key-typing commands (get and set) wait for the settle
    /// time after it.
    quiet_from: Mutex<Option<Instant>>,
    fake: Option<FakeHandle>,
    status: Arc<Mutex<Status>>,
    stop: Arc<AtomicBool>,
    config: SessionConfig,
    log: Arc<Log>,
    menu: Arc<MenuCtl>,
    /// The strict rule's window: a private message from the key within this long.
    strict: Duration,
    /// How long a change waits for the key to be touched.
    touch_wait: Duration,
}

/// A change made while the key could not be sent to (0.37.0): sent when the key is next awake.
struct Held {
    change: Change,
    since: Instant,
}

enum Purpose {
    Read,
    Set(Sender<Result<(), String>>),
    /// A key-typing job (see `KeyJob`).
    Key(KeyId),
    /// A key-typing read job (before writing a list that is not known).
    KeyRead(KeyId),
    /// A settings change from the page (0.37.0): its reply, and whether it was sent on the key's timers only
    /// (no fresh message): a stall then makes the strict rule apply.
    Change(Change, Sender<Result<(), String>>, bool),
    /// The read after a settings change: compared with what was sent.
    ReadBack(Change),
}

static TICKETS: AtomicU64 = AtomicU64::new(1);

impl Worker {
    fn set_state(&self, state: &'static str, message: &str) {
        let mut s = self.status.lock_or_recover();
        if s.state != state || s.message != message {
            if s.state != state {
                self.log.line(&format!("state {state}: {message}"));
            }
            s.state = state;
            s.message = message.into();
        }
    }

    /// Take what the key holds as the offline typing of controls whose offline typing was never set (the
    /// user's own keys are kept), and save that. Nothing is written for such a control before this.
    fn seed_known(&self) {
        let mut seeds = Vec::new();
        {
            let s = self.status.lock_or_recover();
            let mut l = self.layers.lock_or_recover();
            for (i, (name, id)) in LAYER_CONTROLS.iter().enumerate() {
                if l.seeded[i] {
                    continue;
                }
                if let Some(held) = s.known.keys[id.index() as usize].clone() {
                    l.seed(i, &held);
                    let typing = l.offline[i].clone();
                    seeds.push((name.to_string(), typing.is_factory(), typing.label(*id)));
                }
            }
        }
        if self.exiting.load(Ordering::SeqCst) {
            return;
        }
        for (name, factory, label) in seeds {
            self.log.line(&format!("layer: {name} offline typing taken from the key ({})", if factory { "factory" } else { "keys" }));
            let patch = if factory {
                json!({"offline": {"control": name, "mode": "factory", "seed": true}})
            } else {
                json!({"offline": {"control": name, "mode": "keys", "keys": label, "seed": true}})
            };
            let persist = self.menu.persist.lock_or_recover().clone();
            if let Some(p) = persist {
                // Not on the worker thread: saving re-runs configure.
                let _ = std::thread::Builder::new().name("uc-au05-seed".into()).spawn(move || {
                    let _ = p(patch);
                });
            }
        }
    }

    /// A control not checked on the real key, allowed with "Set up", was written and its echo matched: it
    /// counts as checked on this key from now on (saved).
    fn save_set_up(&self, key: KeyId) {
        let Some(i) = LAYER_CONTROLS.iter().position(|(_, id)| *id == key) else { return };
        {
            let mut l = self.layers.lock_or_recover();
            if l.verified[i] || !self.status.lock_or_recover().sync.allowed[key.index() as usize] {
                return;
            }
            l.verified[i] = true;
        }
        if self.exiting.load(Ordering::SeqCst) {
            return;
        }
        let patch = json!({"setUp": {"control": LAYER_CONTROLS[i].0}});
        let persist = self.menu.persist.lock_or_recover().clone();
        if let Some(p) = persist {
            let _ = std::thread::Builder::new().name("uc-au05-setup".into()).spawn(move || {
                let _ = p(patch);
            });
        }
    }

    /// Save the list the key just confirmed for a control (read, or a set whose echo matched).
    /// Keep the key's standby or power-off time in the settings when it changed (the shown state uses it after
    /// a restart, before the key answers).
    fn save_known_time(&self, name: &str, value: Option<u32>) {
        let Some(v) = value else { return };
        if self.menu.settings()["knownTimes"][name].as_u64() == Some(u64::from(v)) {
            return;
        }
        self.menu.settings.lock_or_recover()["knownTimes"][name] = json!(v);
        let patch = json!({"knownTimes": {name: v}});
        let persist = self.menu.persist.lock_or_recover().clone();
        if let Some(p) = persist {
            let _ = std::thread::Builder::new().name("uc-au05-times".into()).spawn(move || {
                let _ = p(patch);
            });
        }
    }

    fn save_echo(&self, key: KeyId, typing: Option<KeyTyping>) {
        let Some(i) = LAYER_CONTROLS.iter().position(|(_, id)| *id == key) else { return };
        let name = LAYER_CONTROLS[i].0;
        let text = typing.as_ref().map(|t| if t.is_factory() { "factory".to_string() } else { t.label(key) });
        {
            let mut l = self.layers.lock_or_recover();
            if l.echo[i] == typing {
                return;
            }
            l.echo[i] = typing;
        }
        if self.exiting.load(Ordering::SeqCst) {
            return;
        }
        let patch = json!({"keyEcho": {"control": name, "keys": text}});
        let persist = self.menu.persist.lock_or_recover().clone();
        if let Some(p) = persist {
            let _ = std::thread::Builder::new().name("uc-au05-echo".into()).spawn(move || {
                let _ = p(patch);
            });
        }
    }

    /// Keep the key holding the wanted layer, with the review's guards (2026-11-17):
    /// - a control whose list is not known is READ first (if UC would write it), one at a time;
    /// - nothing is written for a control whose offline typing is not set or seeded yet;
    /// - one automatic attempt per wanted typing; after a failed key-typing command nothing more is queued
    ///   until Reconnect or Try again; after quit / disable ran out of time, nothing new at all;
    /// - controls not checked on the real key (knob press, knob left) wait for the user's "Set up".
    fn sync_layers(&self) {
        self.seed_known();
        let l = self.layers.lock_or_recover().clone();
        let mut s = self.status.lock_or_recover();
        for (i, (name, id)) in LAYER_CONTROLS.iter().enumerate() {
            let idx = id.index() as usize;
            let want = l.want(i).clone();
            let known = s.known.keys[idx].clone();
            if let Some(j) = s.jobs[idx].as_mut() {
                match j.state {
                    "waiting" => {
                        if !j.read && j.typing != want {
                            j.typing = want;
                        }
                        continue;
                    }
                    "sending" => {
                        if !j.read {
                            j.next = (j.typing != want).then_some(want);
                        }
                        continue;
                    }
                    // A failed job stays until Try again, or until the wanted typing changes (one automatic
                    // attempt for the new one, below).
                    "failed" if j.read || j.typing == want => continue,
                    // Recomputed every time: it goes away when the control no longer needs writing.
                    "failed" | "check" => s.jobs[idx] = None,
                    _ => {}
                }
            }
            let check = !l.verified[i] && !s.sync.allowed[idx];
            // What UC would do for this control: read it first (unknown list) or write it (known, differs).
            let wanted = match &known {
                None => {
                    !s.sync.read_tried[idx]
                        && l.echo[i].as_ref() != Some(&want)
                        && (l.dirty[i] || (!l.target_offline && l.uc[i]) || l.echo[i].is_some())
                }
                Some(held) => l.seeded[i] && *held != want && s.sync.attempted[idx].as_ref() != Some(&want),
            };
            if !wanted {
                continue;
            }
            // Not checked on the real key: always visible as "Needs a check" with Set up (even while key changes
            // are paused), never written by UC on its own.
            if check {
                if !s.sync.check_logged[idx] {
                    s.sync.check_logged[idx] = true;
                    self.log.line(&format!("layer sync: {name} needs Set up (not checked on the real key; wants {})", want.label(*id)));
                }
                s.jobs[idx] = Some(KeyJob { typing: want, state: "check", message: "Not checked on the real key yet: press Set up".into(), next: None, read: false });
                continue;
            }
            if s.sync.blocked || s.sync.no_new {
                continue;
            }
            if known.is_none() {
                s.sync.read_tried[idx] = true;
                self.log.line(&format!("layer sync: read {name} first"));
                s.jobs[idx] = Some(KeyJob { typing: want, state: "waiting", message: String::new(), next: None, read: true });
            } else {
                self.log.line(&format!("layer sync: {name} -> {} ({})", want.label(*id), if l.target_offline { "offline" } else { "live" }));
                s.sync.attempted[idx] = Some(want.clone());
                s.jobs[idx] = Some(KeyJob { typing: want, state: "waiting", message: String::new(), next: None, read: false });
            }
        }
    }

    /// Send the next waiting key job when the key is awake, no other key-typing command is pending, and no
    /// guard holds key typing back.
    /// The settle time has passed since the key woke and since its last control event (the driver applies it to
    /// sets only; UC applies it to key-typing reads too).
    /// 1.2.2 (user report 2026-11-19, "very laggy" right after the key woke): a press or knob turn from the key's
    /// own codes counts too. Those report nothing privately, so before this UC sent its reads while the key was
    /// in use; one got no reply and sending paused for 30 s.
    fn settled(&self) -> bool {
        let quiet = *self.quiet_from.lock_or_recover();
        let used = *self.menu.key_seen.lock_or_recover();
        [quiet, used].into_iter().flatten().max().is_none_or(|t| t.elapsed() >= self.config.settle)
    }

    /// 1.2.2: the automatic settings reads (nothing the user asked for) wait longer: not during a dictation, and
    /// not until the key has not been pressed or turned for `READ_QUIET_SETTLES` × the settle time (6 s).
    fn idle_for_reads(&self) -> bool {
        if self.menu.dictate_busy.load(Ordering::SeqCst) {
            return false;
        }
        let used = *self.menu.key_seen.lock_or_recover();
        used.is_none_or(|t| t.elapsed() >= self.config.settle * READ_QUIET_SETTLES)
    }

    fn start_jobs(&self, session: &mut Session<Box<dyn Transport>>, tickets: &mut HashMap<u64, Purpose>, awake: bool) {
        if !awake || session.state() != SessionState::Awake || session.has_pending(Request::slow_reply) || !self.settled() {
            return;
        }
        let next = {
            let s = self.status.lock_or_recover();
            if s.sync.blocked || s.sync.no_new || s.sync.not_before.is_some_and(|t| Instant::now() < t) {
                return;
            }
            KeyId::EVERY.into_iter().find_map(|k| {
                s.jobs[k.index() as usize]
                    .as_ref()
                    .filter(|j| j.state == "waiting")
                    .map(|j| (k, j.typing.clone(), j.read))
            })
        };
        let Some((key, typing, read)) = next else { return };
        let t = TICKETS.fetch_add(1, Ordering::Relaxed);
        let request = if read { Request::GetKeyTyping(key) } else { Request::SetKeyTyping(key, typing) };
        let name = describe(&request);
        let result = session.submit(t, request);
        let mut s = self.status.lock_or_recover();
        let job = s.jobs[key.index() as usize].as_mut().unwrap();
        match result {
            Ok(()) => {
                job.state = "sending";
                tickets.insert(t, if read { Purpose::KeyRead(key) } else { Purpose::Key(key) });
                self.log.line(&format!("queued {name}"));
            }
            Err(f) => {
                job.state = "failed";
                job.message = f.message();
                self.log.line(&format!("refused {name}: {}", f.message()));
            }
        }
    }

    fn open(&self) -> std::io::Result<Box<dyn Transport>> {
        if let Some(f) = &self.fake {
            // The fake key is touched at start-up, so the page has something to show.
            f.wake();
            return Ok(Box::new(MangleEcho { inner: Box::new(f.transport()), on: self.mangle_echo.clone() }));
        }
        #[cfg(windows)]
        {
            if super::buildinfo::fake_device().is_some() {
                return Err(std::io::Error::other("test mode never opens the real key"));
            }
            Ok(Box::new(ulanzi_au05::windows::WindowsAu05::open_first()?))
        }
        #[cfg(not(windows))]
        Err(std::io::Error::other("Windows only"))
    }

    /// One connection lifetime after another until stopped. A crash restarts this from a clean state.
    fn run(&self, rx: &Arc<Mutex<Receiver<Cmd>>>) {
        let mut backoff = Duration::from_secs(1);
        while !self.stop.load(Ordering::SeqCst) {
            match self.open() {
                Ok(t) => {
                    backoff = Duration::from_secs(1);
                    self.log.line("receiver opened");
                    self.connected(t, rx);
                }
                Err(e) => {
                    let missing = e.kind() == std::io::ErrorKind::NotFound;
                    self.log.line(&format!("receiver not opened: {:?}", e.kind()));
                    self.set_state(
                        if missing { "not_found" } else { "error" },
                        if missing {
                            "The Vibe Key receiver is not plugged in"
                        } else {
                            "The Vibe Key receiver could not be opened"
                        },
                    );
                }
            }
            // Wait before the next try, still answering commands.
            let until = Instant::now() + backoff;
            while Instant::now() < until && !self.stop.load(Ordering::SeqCst) {
                match rx.lock_or_recover().recv_timeout(Duration::from_millis(100)) {
                    Ok(Cmd::Stop) => return,
                    Ok(Cmd::Change(_, r)) | Ok(Cmd::ReadKey(_, r)) | Ok(Cmd::ReadAll(r)) => {
                        let _ = r.send(Err("The Vibe Key is not connected".into()));
                    }
                    Ok(Cmd::Reconnect) => break,
                    Err(_) => {}
                }
            }
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    }

    fn connected(&self, transport: Box<dyn Transport>, rx: &Arc<Mutex<Receiver<Cmd>>>) {
        let mut session = Session::new(transport, self.config.clone());
        // What the driver opened (collections, capabilities; no paths) and, below, a heartbeat with read
        // counters: every 30 s for the first 5 minutes, then every 10 minutes (real-key bug 2026-11-17: the
        // receiver opened but no report ever arrived).
        if let Some(d) = session.diagnostics() {
            self.log.line(&format!("receiver: {d}"));
        }
        let opened_at = Instant::now();
        let mut next_beat = opened_at + Duration::from_secs(30);
        let (mut ticks, mut outputs_seen) = (0u64, 0u64);
        // The last message from the key itself (a status, a control event or an answer).
        let mut last_msg: Option<Instant> = None;
        let mut held: Vec<Held> = Vec::new();
        let mut tickets: HashMap<u64, Purpose> = HashMap::new();
        let mut reads = ConnectReads::default();
        reads.restart();
        let mut read_burst: Option<(Instant, Vec<u64>)> = None;
        // When the key last became awake (after asleep, a pause, or at connect).
        let mut awake_since: Option<Instant> = None;
        let mut last_state = None;
        // A "Read from key" waiting for the settle time (one at a time).
        let mut deferred_read: Option<(KeyId, Sender<Result<(), String>>, Instant)> = None;
        let mut was_awake = false;
        self.set_state("waiting", WAITING_MESSAGE);
        loop {
            if self.stop.load(Ordering::SeqCst) {
                break;
            }
            // Commands from the page.
            loop {
                let cmd = rx.lock_or_recover().try_recv();
                match cmd {
                    Ok(Cmd::Stop) => {
                        self.finish(&mut session, &mut tickets);
                        return;
                    }
                    Ok(Cmd::Reconnect) => {
                        self.log.line("page reconnect");
                        session.resume();
                        reads.restart();
                        let mut s = self.status.lock_or_recover();
                        s.last_error = None;
                        s.reads_failed = false;
                        // Reconnect also lifts the guard after a failed key change (one more attempt each).
                        s.sync.blocked = false;
                        s.sync.no_new = false;
                        s.sync.attempted = Default::default();
                        s.sync.read_tried = Default::default();
                    }
                    Ok(Cmd::Change(c, reply)) => {
                        let (fresh, timer) = self.gate(&session, last_msg, opened_at);
                        let busy = tickets.values().any(|p| matches!(p, Purpose::Change(..) | Purpose::ReadBack(_)));
                        if (fresh || timer) && self.settled() && held.is_empty() && !busy {
                            self.note_change(&c, "sending", None);
                            self.submit_change(&mut session, &mut tickets, c, reply, !fresh);
                        } else if matches!(session.state(), SessionState::Paused | SessionState::Suspended) {
                            let _ = reply.send(Err(Failure::Paused.message()));
                        } else {
                            // Not sent now: kept until the key is touched (or the change before it is done).
                            let key = change_key(&c);
                            held.retain(|h| change_key(&h.change) != key);
                            let asleep = !(fresh || timer);
                            if asleep {
                                self.log.line(&format!("held {key}: the key is asleep (touch it to apply)"));
                            }
                            self.note_change(&c, if asleep { "touch" } else { "sending" }, None);
                            held.push(Held { change: c, since: Instant::now() });
                            let _ = reply.send(Ok(()));
                        }
                    }
                    Ok(Cmd::ReadAll(reply)) => {
                        self.log.line("page read all");
                        let mut result = Ok(());
                        let mut sent = Vec::new();
                        for r in read_all_settings() {
                            let t = TICKETS.fetch_add(1, Ordering::Relaxed);
                            match session.submit(t, r.clone()) {
                                Ok(()) => {
                                    tickets.insert(t, Purpose::Read);
                                    if read_all().contains(&r) {
                                        sent.push((t, r));
                                    }
                                }
                                Err(f) => {
                                    result = Err(f.message());
                                    break;
                                }
                            }
                        }
                        if !sent.is_empty() {
                            reads.manual(sent);
                            self.status.lock_or_recover().reads_failed = false;
                        }
                        let _ = reply.send(result);
                    }
                    Ok(Cmd::ReadKey(k, reply)) => {
                        if session.has_pending(Request::slow_reply) || deferred_read.is_some() {
                            let _ = reply.send(Err("Another key is being read. Try again in a moment".into()));
                        } else {
                            // Sent once the key has settled (below).
                            deferred_read = Some((k, reply, Instant::now()));
                        }
                    }
                    Err(_) => break,
                }
            }
            let outputs = match session.tick(Duration::from_millis(25)) {
                Ok(o) => o,
                Err(e) => {
                    self.log.line(&format!("transport error {:?}", e.kind()));
                    self.finish(&mut session, &mut tickets);
                    self.set_state("not_found", "The Vibe Key receiver was unplugged or stopped answering");
                    return;
                }
            };
            ticks += 1;
            outputs_seen += outputs.len() as u64;
            for o in outputs {
                // Only what came from the key counts as a sign it is awake (not a failure made here).
                if matches!(&o, Output::Status(_) | Output::Control(_) | Output::Done { result: Ok(_), .. }) {
                    last_msg = Some(Instant::now());
                }
                self.track_read(&o, &mut reads);
                self.handle(o, &mut tickets);
            }
            if Instant::now() >= next_beat {
                let early = opened_at.elapsed() < Duration::from_secs(300);
                next_beat = Instant::now() + Duration::from_secs(if early { 30 } else { 600 });
                self.log.line(&format!(
                    "heartbeat: {:?}, {ticks} ticks, {outputs_seen} messages; {}",
                    session.state(),
                    session.diagnostics().unwrap_or_else(|| "no transport diagnostics".into())
                ));
            }
            // The settle time starts when the key wakes (control events restart it in `handle`). "Awake" here
            // is the strict rule (a message within the strict window): reads and key typing use it.
            let (fresh, timer) = self.gate(&session, last_msg, opened_at);
            let awake = fresh;
            if awake && !was_awake {
                *self.quiet_from.lock_or_recover() = Some(Instant::now());
                awake_since = Some(Instant::now());
                // Woke from "probably switched off": its values may have changed while it was off.
                if last_state.is_some_and(|(s, _)| s == "off") {
                    self.log.line("woke from switched off: reading the settings again");
                    reads.restart();
                    self.status.lock_or_recover().reads_failed = false;
                }
            }
            if !awake {
                awake_since = None;
            }
            was_awake = awake;
            if deferred_read.is_some() && !session.has_pending(Request::slow_reply) {
                let state = session.state();
                let waited = deferred_read.as_ref().unwrap().2.elapsed();
                if state == SessionState::Paused || state == SessionState::Suspended {
                    let (_, reply, _) = deferred_read.take().unwrap();
                    let _ = reply.send(Err(Failure::Paused.message()));
                } else if !awake && waited >= self.config.request_expiry {
                    // As a queued request would: the key did not wake in time.
                    let (_, reply, _) = deferred_read.take().unwrap();
                    let _ = reply.send(Err(Failure::Asleep.message()));
                } else if awake && self.settled() {
                    let (k, reply, _) = deferred_read.take().unwrap();
                    let t = TICKETS.fetch_add(1, Ordering::Relaxed);
                    match session.submit(t, Request::GetKeyTyping(k)) {
                        Ok(()) => {
                            tickets.insert(t, Purpose::Set(reply));
                        }
                        Err(f) => {
                            let _ = reply.send(Err(f.message()));
                        }
                    }
                }
            }
            // Held changes: sent one at a time once the key is awake; not applied after the touch wait.
            let busy = tickets.values().any(|p| matches!(p, Purpose::Change(..) | Purpose::ReadBack(_)));
            if !held.is_empty() && !busy && (fresh || timer) && self.settled() {
                let h = held.remove(0);
                self.log.line(&format!("sending held {}", change_key(&h.change)));
                self.note_change(&h.change, "sending", None);
                let (rtx, _rrx) = mpsc::channel();
                self.submit_change(&mut session, &mut tickets, h.change, rtx, !fresh);
            }
            held.retain(|h| {
                if h.since.elapsed() < self.touch_wait {
                    return true;
                }
                self.log.line(&format!("not applied {}: the key was asleep", change_key(&h.change)));
                self.note_change(&h.change, "failed", Some("Not applied: the key was asleep"));
                false
            });
            self.sync_layers();
            self.start_jobs(&mut session, &mut tickets, awake);
            let state = session.state();
            // The settings reads: awake for twice the settle time and settled since the last control event.
            let ready = awake
                && awake_since.is_some_and(|t| t.elapsed() >= self.config.settle * 2)
                && self.settled()
                && self.idle_for_reads();
            let cycle = reads.cycle;
            let batch = reads.next(ready);
            if !batch.is_empty() {
                if cycle >= 1 {
                    self.log.line(&format!("settings reads again after a stall: {}", describe(&batch[0])));
                }
                let mut burst = Vec::new();
                for r in batch {
                    let t = TICKETS.fetch_add(1, Ordering::Relaxed);
                    match session.submit(t, r.clone()) {
                        Ok(()) => {
                            tickets.insert(t, Purpose::Read);
                            reads.out.insert(t, r);
                            burst.push(t);
                        }
                        Err(_) => reads.todo.push(r),
                    }
                }
                if cycle == 0 {
                    read_burst = Some((Instant::now(), burst));
                }
            }
            // Cap the read burst by total time: reads still queued after the limit are dropped.
            if let Some((started, burst)) = &read_burst {
                if started.elapsed() >= READ_BURST_LIMIT {
                    for o in session.cancel_queued(|t, _| burst.contains(&t)) {
                        self.track_read(&o, &mut reads);
                        self.handle(o, &mut tickets);
                    }
                    read_burst = None;
                }
            }
            if reads.done() {
                read_burst = None;
            }
            // The shown state (display only). Switched off (2026-11-17, real key: after its power-off time the
            // key turns itself off and only its power button brings it back): no message since the connection
            // opened for longer than the standby time + 2 minutes, or none for longer than the power-off time.
            // A hotkey from the key's own codes counts as a sign of life (those controls report nothing privately).
            let shown = {
                let saved = self.menu.settings()["knownTimes"].clone();
                let key_at = *self.menu.key_seen.lock_or_recover();
                let k = self.status.lock_or_recover().known.clone();
                let signal = [last_msg, key_at.filter(|t| *t >= opened_at)].into_iter().flatten().max();
                let standby = k.standby_s.map(u64::from).or(saved["standbyS"].as_u64()).unwrap_or(u64::from(DEFAULT_STANDBY_S));
                let power_off = k.sleep_s.map(u64::from).or(saved["sleepS"].as_u64()).map(Duration::from_secs);
                // The driver's own "awake" is wide (see `uc_session_config`): a fresh message is what counts.
                let st = if fresh { SessionState::Awake } else if state == SessionState::Awake { SessionState::Asleep } else { state };
                shown_state(st, signal.map(|t| t.elapsed()), opened_at.elapsed(), Duration::from_secs(standby), power_off)
            };
            if last_state != Some(shown) {
                last_state = Some(shown);
                self.set_state(shown.0, shown.1);
            }
            {
                let mut s = self.status.lock_or_recover();
                s.sent = session.sent;
                s.stalls = session.stalled;
                s.pending_sets = tickets.values().filter(|p| !matches!(p, Purpose::Read)).count();
                s.sync.pending = session.has_pending(Request::slow_reply);
            }
        }
        self.finish(&mut session, &mut tickets);
    }

    /// The send rule (0.37.0). `fresh`: a message from the key within the strict window (the rule reads and
    /// key typing keep, and the only one for a while after a stall). `timer`: the key's own timers say it is
    /// awake: its last input (a message, or a hotkey from its own codes) is within its standby time less a
    /// 10 s margin, and not limited by the driver's window. Both need the driver ready (not paused, a message
    /// since the connection opened); sending also waits for the settle time (`settled`).
    fn gate(&self, session: &Session<Box<dyn Transport>>, last_msg: Option<Instant>, opened_at: Instant) -> (bool, bool) {
        let state = session.state();
        if matches!(state, SessionState::Paused | SessionState::Suspended | SessionState::WaitingForKey) || last_msg.is_none() {
            return (false, false);
        }
        let fresh = last_msg.is_some_and(|t| t.elapsed() <= self.strict);
        // One lock at a time (the settings, then the status): never both held.
        let saved = self.menu.settings()["knownTimes"]["standbyS"].as_u64();
        let (standby, strict) = {
            let s = self.status.lock_or_recover();
            let standby = s.known.standby_s.map(u64::from).or(saved).unwrap_or(u64::from(DEFAULT_STANDBY_S));
            (Duration::from_secs(standby), s.strict_until.is_some_and(|t| Instant::now() < t))
        };
        let key_at = (*self.menu.key_seen.lock_or_recover()).filter(|t| *t >= opened_at);
        let input = [last_msg, key_at].into_iter().flatten().max();
        let window = standby.saturating_sub(STANDBY_MARGIN).min(self.config.awake_window);
        let timer = !strict && input.is_some_and(|t| t.elapsed() <= window) && last_msg.is_some_and(|t| t.elapsed() <= self.config.awake_window);
        (fresh, timer)
    }

    /// The page's per-setting line for a change (and its value while it waits).
    fn note_change(&self, c: &Change, state: &str, message: Option<&str>) {
        let mut s = self.status.lock_or_recover();
        s.changes.insert(change_key(c), json!({"state": state, "value": change_value(c), "message": message, "at": now_ms()}));
    }

    /// Follow the settings reads' results (see `ConnectReads`).
    fn track_read(&self, o: &Output, reads: &mut ConnectReads) {
        if let Output::Done { ticket, result, .. } = o {
            if reads.result(*ticket, result) {
                self.log.line("settings reads stalled again: left not read (Read again on the page)");
                self.status.lock_or_recover().reads_failed = true;
            }
        }
    }

    fn finish(&self, session: &mut Session<Box<dyn Transport>>, tickets: &mut HashMap<u64, Purpose>) {
        for o in session.fail_all(Failure::Closed) {
            self.handle(o, tickets);
        }
        // A key job that never reached the key waits for the next connection.
        for j in self.status.lock_or_recover().jobs.iter_mut().flatten() {
            if j.state == "sending" {
                j.state = "waiting";
            }
        }
        self.status.lock_or_recover().pending_sets = 0;
    }

    fn submit_change(
        &self,
        session: &mut Session<Box<dyn Transport>>,
        tickets: &mut HashMap<u64, Purpose>,
        change: Change,
        reply: Sender<Result<(), String>>,
        timer_only: bool,
    ) {
        let known = self.status.lock_or_recover().known.clone();
        let original = change.clone();
        let request = match change {
            Change::StandbyTime(s) => Request::SetStandbyTime(s),
            Change::SleepTime(s) => match SleepTime::new(s) {
                Some(t) => Request::SetSleepTime(t),
                None => {
                    let _ = reply.send(Err("Choose one of the offered power-off times".into()));
                    return;
                }
            },
            Change::LightMode(m) => Request::SetLightMode(m),
            Change::LightBrightness(b) => Request::SetLightBrightness(b),
            Change::MicNrLevel(level) => match known.mic {
                // The two other values are written back exactly as read.
                Some(m) => Request::SetMicNr(MicNr { level, ..m }),
                None => {
                    let _ = reply.send(Err("Read the microphone setting first (wake the key)".into()));
                    return;
                }
            },
            Change::Retry(_) => {
                let _ = reply.send(Err("Try again is handled by the key jobs".into()));
                return;
            }
            Change::LightRecord { record, field, value } => Request::SetLightRecord { record, field, value },
            Change::HooksMode(m) => Request::SetHooksMode(m),
        };
        if session.state() == SessionState::Paused {
            let _ = reply.send(Err(Failure::Paused.message()));
            return;
        }
        // Key typing (0x50) one at a time; its set reply echoes the new list, so no extra read is sent.
        if request.slow_reply() && session.has_pending(Request::slow_reply) {
            let _ = reply.send(Err("Another key is being changed. Try again in a moment".into()));
            return;
        }
        let read_back = match &request {
            Request::SetStandbyTime(_) => Some(Request::GetStandbyTime),
            Request::SetSleepTime(_) => Some(Request::GetSleepTime),
            Request::SetLightMode(_) | Request::SetLightBrightness(_) | Request::SetLightRecord { .. } => {
                Some(Request::GetLight)
            }
            Request::SetHooksMode(_) => Some(Request::GetHooksMode),
            Request::SetMicNr(_) => Some(Request::GetMicNr),
            _ => None,
        };
        let t = TICKETS.fetch_add(1, Ordering::Relaxed);
        let name = describe(&request);
        match session.submit(t, request) {
            Ok(()) => {
                self.log.line(&format!("queued {name}{}", if timer_only { " (on the key's timers)" } else { "" }));
                tickets.insert(t, Purpose::Change(original.clone(), reply, timer_only));
                if let Some(r) = read_back {
                    let rt = TICKETS.fetch_add(1, Ordering::Relaxed);
                    if session.submit(rt, r).is_ok() {
                        tickets.insert(rt, Purpose::ReadBack(original));
                    }
                }
            }
            Err(f) => {
                self.log.line(&format!("refused {name}: {}", f.message()));
                self.note_change(&original, "failed", Some(&f.message()));
                let _ = reply.send(Err(f.message()));
            }
        }
    }

    fn handle(&self, o: Output, tickets: &mut HashMap<u64, Purpose>) {
        match o {
            Output::Status(st) => {
                let mut s = self.status.lock_or_recover();
                let before = s.known.battery_flags;
                s.known.battery = Some((st.millivolts, st.percent));
                s.known.battery_flags = Some(st.flags);
                s.known.battery_at = Some(now_ms());
                drop(s);
                if before != Some(st.flags) {
                    let was = before.map(|f| format!("{f:#04x}")).unwrap_or_else(|| "none".into());
                    self.log.line(&format!("battery status flags {was} -> {:#04x} at {} % ({} mV)", st.flags, st.percent, st.millivolts));
                }
            }
            // Private events are shown (K1 and the knob turns send them). Actions use hotkeys; knob turns
            // also move an open quick menu (unless the knob is set to type F23/F24 instead).
            Output::Control(e) => {
                *self.quiet_from.lock_or_recover() = Some(Instant::now());
                push_event(&self.status, e.control, e.pressed, e.code, None);
                if e.pressed && self.menu.is_open() && !self.menu.turns_are_hotkeys() {
                    match e.control {
                        Control::KnobRight => self.menu.input(Input::Next, "knob"),
                        Control::KnobLeft => self.menu.input(Input::Prev, "knob"),
                        _ => {}
                    }
                }
            }
            Output::Done { ticket, request, result } => {
                let purpose = tickets.remove(&ticket);
                self.log.line(&match &result {
                    Ok(_) => format!("done {}: ok", describe(&request)),
                    Err(f) => format!("done {}: {:?}", describe(&request), f),
                });
                match &result {
                    Ok(r) => self.learn(&request, r),
                    Err(f) => {
                        if !matches!(f, Failure::Closed) {
                            self.status.lock_or_recover().last_error = Some(f.message());
                        }
                    }
                }
                // Any key-typing command that failed after it may have reached the key: no automatic key-typing
                // command until Reconnect or Try again, and a write's result is in doubt.
                let not_sent = |f: &Failure| matches!(f, Failure::Asleep | Failure::Closed | Failure::Cancelled);
                if let (true, Err(f)) = (request.slow_reply(), &result) {
                    if !not_sent(f) {
                        let mut s = self.status.lock_or_recover();
                        s.sync.blocked = true;
                        if let Request::SetKeyTyping(id, _) = &request {
                            s.sync.uncertain[id.index() as usize] = true;
                            s.known.keys[id.index() as usize] = None;
                        }
                        drop(s);
                        self.log.line("key typing paused after a failed key change: use Reconnect or Try again");
                    }
                }
                match purpose {
                    Some(Purpose::Set(reply)) => {
                        let _ = reply.send(result.map(|_| ()).map_err(|f| f.message()));
                    }
                    Some(Purpose::Change(change, reply, timer_only)) => {
                        if let Err(f) = &result {
                            let message = if matches!(f, Failure::Asleep) { "Not applied: the key was asleep".to_string() } else { f.message() };
                            self.note_change(&change, "failed", Some(&message));
                            // Sent on the key's timers only and it stalled: the strict rule for a while.
                            if timer_only && matches!(f, Failure::NoReply | Failure::WriteTimeout) {
                                self.status.lock_or_recover().strict_until = Some(Instant::now() + STRICT_FOR);
                                self.log.line("a change sent on the key's timers stalled: strict sending (a fresh message) for 5 minutes");
                            }
                        }
                        let _ = reply.send(result.map(|_| ()).map_err(|f| f.message()));
                    }
                    Some(Purpose::ReadBack(change)) => {
                        let key = change_key(&change);
                        let got = read_back(&change, &self.status.lock_or_recover().known);
                        match (&result, got) {
                            (Ok(_), Some((same, value))) => {
                                // The value read back after every set (no raw bytes).
                                self.log.line(&format!("read back {key}: {value}{}", if same { "" } else { " (not what was sent)" }));
                                if same {
                                    self.note_change(&change, "applied", None);
                                } else {
                                    self.note_change(&change, "kept", Some(&format!("Key kept {value}")));
                                }
                            }
                            _ => {
                                self.log.line(&format!("read back {key}: no answer"));
                                if self.status.lock_or_recover().changes.get(&key).is_some_and(|c| c["state"] == "sending") {
                                    self.note_change(&change, "sent", Some("Sent; not read back"));
                                }
                            }
                        }
                    }
                    Some(Purpose::KeyRead(key)) => {
                        let mut s = self.status.lock_or_recover();
                        let i = key.index() as usize;
                        match &result {
                            Ok(_) => {
                                s.jobs[i] = None;
                                let held = s.known.keys[i].clone();
                                drop(s);
                                self.save_echo(key, held);
                                return;
                            }
                            Err(f) if not_sent(f) => {
                                if let Some(j) = s.jobs[i].as_mut() {
                                    j.state = "waiting";
                                }
                            }
                            Err(f) => {
                                if let Some(j) = s.jobs[i].as_mut() {
                                    j.state = "failed";
                                    j.message = f.message();
                                }
                            }
                        }
                    }
                    Some(Purpose::Key(key)) => {
                        let mut s = self.status.lock_or_recover();
                        let i = key.index() as usize;
                        let held = s.known.keys[i].clone();
                        if let Some(job) = s.jobs[i].as_mut() {
                            match &result {
                                // The echo is what the key stored: anything else than what was written is a
                                // failure, never repeated automatically.
                                Ok(_) if held.as_ref() == Some(&job.typing) => job.state = "done",
                                Ok(_) => {
                                    job.state = "failed";
                                    job.next = None;
                                    job.message = format!(
                                        "The key stored something else ({})",
                                        held.map(|h| h.label(key)).unwrap_or_else(|| "unknown".into())
                                    );
                                }
                                // Never written (the key went to sleep first): wait for it again.
                                Err(f) if not_sent(f) => job.state = "waiting",
                                Err(f) => {
                                    job.state = "failed";
                                    job.next = None;
                                    job.message = f.message();
                                }
                            }
                            // A newer choice was made meanwhile: it becomes the waiting job.
                            if job.state == "done" {
                                if let Some(next) = job.next.take() {
                                    job.typing = next;
                                    job.state = "waiting";
                                    job.message.clear();
                                }
                            }
                        }
                        if matches!(result, Ok(_)) {
                            s.sync.uncertain[i] = false;
                        }
                        // Saved only when confirmed; a failed or different result forgets the saved list.
                        let confirmed = s.jobs[i].as_ref().is_some_and(|j| j.state == "done" || (j.state == "waiting" && result.is_ok()));
                        let held = s.known.keys[i].clone();
                        let gone = !matches!(&result, Err(f) if not_sent(f));
                        drop(s);
                        if confirmed {
                            self.save_set_up(key);
                            self.save_echo(key, held);
                        } else if gone {
                            self.save_echo(key, None);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn learn(&self, request: &Request, r: &protocol::Reply) {
        let mut s = self.status.lock_or_recover();
        let k = &mut s.known;
        match request {
            Request::GetBattery => {
                k.battery = Some(protocol::battery(r));
                k.battery_at = Some(now_ms());
            }
            Request::GetStandbyTime | Request::SetStandbyTime(_) => {
                k.standby_s = Some(protocol::seconds(r));
                let v = k.standby_s;
                drop(s);
                self.save_known_time("standbyS", v);
                return;
            }
            Request::GetSleepTime | Request::SetSleepTime(_) => {
                k.sleep_s = Some(protocol::seconds(r));
                let v = k.sleep_s;
                drop(s);
                self.save_known_time("sleepS", v);
                return;
            }
            Request::GetLight => k.light = Some(protocol::light(r)),
            Request::GetHooksMode => k.hooks_mode = Some(protocol::hooks_mode(r)),
            Request::GetStandbyStatus => k.standby_status = Some(protocol::standby_status(r)),
            Request::GetControlCode(i) => k.codes[*i as usize] = Some(protocol::control_code(r)),
            Request::GetMicNr => k.mic = Some(protocol::mic_nr(r)),
            // The set reply echoes the list as stored: 81 06 50 10 <btn> 01 <n> <entries>.
            Request::GetKeyTyping(id) | Request::SetKeyTyping(id, _) => {
                k.keys[id.index() as usize] = Some(protocol::key_typing(r))
            }
            _ => {}
        }
    }
}

/// Fake key only (tests): when switched on, every key-typing set reply echoes one extra key, as a firmware
/// that stores something else would.
struct MangleEcho {
    inner: Box<dyn Transport>,
    on: Arc<AtomicBool>,
}
impl Transport for MangleEcho {
    fn write(&mut self, report: &protocol::Report, timeout: Duration) -> std::io::Result<()> {
        self.inner.write(report, timeout)
    }
    fn read(&mut self, timeout: Duration) -> std::io::Result<Option<protocol::Report>> {
        let r = self.inner.read(timeout)?;
        let Some(report) = r else { return Ok(None) };
        if !self.on.load(Ordering::SeqCst) {
            return Ok(Some(report));
        }
        let Some(mut p) = protocol::decode_frame(&report) else { return Ok(Some(report)) };
        // A set reply: 81 06 50 10 <btn> 01 <n> <entries>.
        if p[0] == 0x81 && p[1] == 0x06 && p[2] == 0x50 && p[3] == 0x10 && (p[6] as usize) < 4 {
            let n = p[6] as usize;
            p[7 + 2 * n] = 2;
            p[8 + 2 * n] = 0x04; // the letter A
            p[6] += 1;
            return Ok(Some(protocol::encode_frame(&p)));
        }
        Ok(Some(report))
    }
}

pub const READS_FAILED_MESSAGE: &str = "Couldn't read the settings: Read again";

/// The settings reads after a wake (0.33.0; real key 2026-11-17: sent the moment the key woke from a long
/// quiet, the first read got no reply, the others ended "Paused" and they were never sent again, so the page
/// kept "Not read"). They wait until the key has been awake for twice the settle time (3 s) and quiet for the
/// settle time (1.5 s) since its last control event. Reads that ended without an answer stay to do; after a
/// stall (no reply, a pause) they are sent once more when the pause has cleared and the key has settled
/// again, one at a time: at most one extra cycle per wake. A second stall leaves them not read. Never key
/// typing (cmd 0x50), and nothing still outstanding is ever resent.
#[derive(Debug, Default)]
struct ConnectReads {
    /// Still to read (not answered yet, not outstanding).
    todo: Vec<Request>,
    /// Sent or queued, not answered yet.
    out: HashMap<u64, Request>,
    /// 0: not sent yet; 1: the first burst was sent; 2: the one extra cycle (one at a time).
    cycle: u8,
    /// The extra cycle stalled too: nothing more is sent automatically until the next wake or Read again.
    failed: bool,
}

impl ConnectReads {
    /// Read everything again (connected, Reconnect, a wake from "probably switched off"). What is still
    /// outstanding is not sent again.
    fn restart(&mut self) {
        let out: Vec<Request> = self.out.values().cloned().collect();
        self.todo = read_all().into_iter().filter(|r| !out.contains(r)).collect();
        self.cycle = 0;
        self.failed = false;
    }
    /// The page's Read again: these tickets cover the reads; no automatic cycle follows a stall.
    fn manual(&mut self, sent: impl IntoIterator<Item = (u64, Request)>) {
        for (t, r) in sent {
            self.todo.retain(|x| *x != r);
            self.out.insert(t, r);
        }
        self.cycle = 2;
        self.failed = false;
    }
    /// A read's result. Returns true when it gave up just now.
    fn result(&mut self, ticket: u64, result: &Result<protocol::Reply, Failure>) -> bool {
        let Some(request) = self.out.remove(&ticket) else { return false };
        let Err(f) = result else { return false };
        if !self.todo.contains(&request) {
            self.todo.push(request);
        }
        let stalled = matches!(f, Failure::NoReply | Failure::WriteTimeout | Failure::Paused | Failure::Suspended);
        if stalled && self.cycle >= 2 && !self.failed {
            self.failed = true;
            return true;
        }
        false
    }
    /// What to send now (`ready`: the key is awake and settled). The first cycle sends every read (the
    /// driver queues them, one at a time); the extra cycle sends one read at a time.
    fn next(&mut self, ready: bool) -> Vec<Request> {
        if !ready || self.failed || self.todo.is_empty() || !self.out.is_empty() {
            return Vec::new();
        }
        match self.cycle {
            0 => {
                self.cycle = 1;
                std::mem::take(&mut self.todo)
            }
            _ => {
                self.cycle = 2;
                vec![self.todo.remove(0)]
            }
        }
    }
    fn done(&self) -> bool {
        self.todo.is_empty() && self.out.is_empty()
    }
}

/// Read once each time the key wakes after connecting. Never key typing (cmd 0x50, which can stall): a
/// key's list is read only when the user asks for it on the page, one at a time.
fn read_all() -> Vec<Request> {
    vec![
        Request::GetBattery,
        Request::GetStandbyTime,
        Request::GetSleepTime,
        Request::GetLight,
        Request::GetHooksMode,
        Request::GetMicNr,
    ]
}

/// The page's "Read all settings": every get except key typing (read per key on request).
fn read_all_settings() -> Vec<Request> {
    let mut v = vec![Request::GetBattery, Request::GetStandbyStatus];
    v.extend(read_all().into_iter().filter(|r| *r != Request::GetBattery));
    v.extend((0..6).map(Request::GetControlCode));
    v
}

/// Later (stub only, not active): the light follows Claude Code's state through Claude Code hooks.
/// Ulanzi Studio sends a work-mode type for light records 1..3 on each hook event (vault page "Ulanzi AU05
/// Vibe Key facts"). The allowlist already has `Request::SetWorkModeType { record, kind }`
/// (`01 0b 88 04 04 <idx> 00 00 <type>`, from static analysis, not sent yet). Still open before this is built:
/// which `kind` values mean which light pattern, which record shows which state, and a user switch (off by
/// default). Until then this returns nothing, so no light command is ever sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentLight {
    Idle,
    Working,
    NeedsApproval,
}
pub fn agent_light_requests(_state: AgentLight) -> Vec<Request> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick() -> SessionConfig {
        SessionConfig {
            write_timeout: Duration::from_millis(60),
            reply_timeout: Duration::from_millis(100),
            awake_window: Duration::from_secs(5),
            gap: Duration::from_millis(5),
            stall_pause: Duration::from_millis(200),
            max_stalls: 3,
            queue_limit: 16,
            request_expiry: Duration::from_millis(500),
            slow_reply_timeout: Duration::from_millis(800),
            settle: Duration::from_millis(30),
            real_key: false,
        }
    }
    fn wait_for(au: &Au05, f: impl Fn(&Value) -> bool) -> Value {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let s = au.status();
            if f(&s) || Instant::now() > end {
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn worker() -> Arc<Au05> {
        let au = Au05::with_config(true, None, quick());
        au.fake_handle().unwrap().lock().status_period = Some(Duration::from_millis(200));
        au
    }

    /// The shown state (0.36.0, user report 2026-11-17: "asleep" 30 s into a dictation, while the key was in
    /// use): driven by the key's own timers from its last input, never by silence.
    #[test]
    fn the_shown_state_follows_the_keys_own_timers() {
        let s = Duration::from_secs;
        let (sb, po) = (s(300), Some(s(3600)));
        let shown = |q: u64| shown_state(SessionState::Asleep, Some(s(q)), s(5000), sb, po).0;
        assert_eq!(shown(30), "connected", "30 s of dictation without a message: still awake");
        assert_eq!(shown(90), "connected");
        assert_eq!(shown(299), "connected");
        assert_eq!(shown(300), "asleep", "past the key's standby time");
        assert_eq!(shown(3599), "asleep");
        assert_eq!(shown(3600), "off", "past its power-off time");
        assert_eq!(shown_state(SessionState::Asleep, Some(s(9000)), s(9000), sb, Some(s(0))).0, "asleep", "power-off Never");
        assert_eq!(shown_state(SessionState::Asleep, Some(s(9000)), s(9000), sb, None).0, "asleep", "power-off not known");
        assert_eq!(shown_state(SessionState::Awake, Some(s(400)), s(400), sb, po), ("connected", AWAKE_MESSAGE), "a message: awake at once");
        assert_eq!(shown_state(SessionState::WaitingForKey, None, s(10), sb, po).0, "waiting");
        assert_eq!(shown_state(SessionState::WaitingForKey, None, s(421), sb, po).0, "off", "nothing since the connection opened");
        assert_eq!(shown_state(SessionState::Paused, Some(s(1)), s(30), sb, po).0, "paused");
        // The times are kept in the settings (used before the key answers after a start).
        let mut au = sanitize(&json!({"knownTimes": {"standbyS": 0, "sleepS": 99_999}}));
        assert_eq!(au["knownTimes"], json!({"standbyS": null, "sleepS": null}), "out of range: not kept");
        apply_patch(&mut au, &json!({"knownTimes": {"standbyS": 60, "sleepS": 0}})).unwrap();
        assert_eq!(sanitize(&au)["knownTimes"], json!({"standbyS": 60, "sleepS": 0}));
    }

    #[test]
    fn a_key_hotkey_counts_as_awake_for_the_shown_state() {
        let au = worker();
        // A short standby on the key, so the page shows "asleep" soon after it stops sending (0.36.0).
        au.fake_handle().unwrap().lock().standby_s = 2;
        let desk = au.fake_desktop().unwrap().clone();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "volumeUp"}}}));
        wait_for(&au, |s| s["micNr"].is_object());
        let fake = au.fake_handle().unwrap().clone();
        fake.sleep();
        let end = Instant::now() + Duration::from_secs(30);
        while au.status()["state"] != "asleep" && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(au.status()["state"], "asleep");
        assert_eq!(au.status()["message"], ASLEEP_MESSAGE);
        desk.press(own_vk(KeyId::K1));
        let s = wait_for(&au, |s| s["state"] == "connected");
        assert_eq!(s["message"], AWAKE_MESSAGE);
        assert!(!fake.lock().awake, "the fake key itself sent nothing");
        au.shutdown();
    }

    /// The reported "stuck" choice (2026-11-17): choosing a UC action must never wait for the key. The action
    /// is saved at once (before the key's list is read), the key change waits visibly while the key is
    /// asleep, and the page always ends in a clear state: done, or the error.
    #[test]
    fn choosing_a_uc_action_never_waits_for_the_key() {
        let au = worker();
        // A short standby on the key, so the page shows "asleep" soon after it stops sending (0.36.0).
        au.fake_handle().unwrap().lock().standby_s = 2;
        au.configure(&json!({"enabled": true}));
        wait_for(&au, |s| s["micNr"].is_object());
        let fake = au.fake_handle().unwrap().clone();
        // The key falls asleep before the user chooses; its K3 list was never read.
        fake.sleep();
        // Wait for the session itself to say "asleep" (the awake window is 5 s here): a fixed sleep raced with
        // a status message still on its way and failed now and then under load.
        let end = Instant::now() + Duration::from_secs(20);
        while au.status()["state"] != "asleep" && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(au.status()["state"], "asleep");
        assert_eq!(au.status()["keys"][0]["mode"], "unknown");
        let started = Instant::now();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind":"claudeDictation"}}}));
        // "At once" compared with the 20 s a key-typing reply may take; generous for a loaded test machine.
        assert!(started.elapsed() < Duration::from_secs(2), "the choice returns at once");
        let s = wait_for(&au, |s| s["keys"][0]["job"]["state"] == "waiting");
        assert_eq!(s["actions"]["k1"]["kind"], "claudeDictation", "saved before the list is read");
        assert_eq!(s["keys"][0]["hotkey"]["state"], "ready", "the hotkey is claimed at once");
        assert_eq!(s["keys"][0]["job"]["read"], true, "the unknown list is read first");
        assert_eq!(s["keys"][0]["job"]["message"], "Waiting for the key: press any key to wake it");
        std::thread::sleep(Duration::from_millis(300));
        assert!(fake.received().iter().all(|p| p[2] != 0x50), "nothing is sent to a sleeping key");
        // Waking the key: K1 is read (factory), then set to its own code; the job ends in Done.
        fake.wake();
        let s = wait_for(&au, |s| s["keys"][0]["job"]["state"] == "done");
        assert_eq!(s["keys"][0]["job"]["message"], "Done");
        assert_eq!(s["keys"][0]["mode"], "uc");
        // A failure ends in the error, not in a spinner.
        fake.lock().drop_replies = 1;
        au.configure(&json!({"enabled": true}));
        let s = wait_for(&au, |s| s["keys"][0]["job"]["state"] == "failed");
        assert!(s["keys"][0]["job"]["message"].as_str().unwrap().contains("did not answer"), "{}", s["keys"][0]["job"]);
        assert_eq!(s["busy"], false);
        au.shutdown();
    }

    /// Two layers (2026-11-17): while UC runs the key holds the live layer (own codes for controls with a UC
    /// action, offline typing for the others); quit writes the offline layer back; only differences are
    /// written; a key asleep at quit is reported as "offline layer not stored".
    #[test]
    fn the_key_holds_the_live_layer_while_uc_runs_and_the_offline_layer_after_quit() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        let sets = |f: &FakeHandle| f.received().into_iter().filter(|p| p[2] == 0x50 && p[3] == 0x00).count();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}},
                             "offline": {"k1": {"mode": "keys", "keys": "Ctrl + D"}}}));
        let s = wait_for(&au, |s| s["keys"][0]["mode"] == "uc");
        assert_eq!(s["layer"]["target"], "live");
        assert_eq!(s["keys"][0]["offline"], "Ctrl + D");
        assert_eq!(sets(&fake), 1, "K1 (its own code) only: K2 is not known and did not change");
        // The user changes K2's offline typing: K2 types it while UC runs too (read first, then written).
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}},
                             "offline": {"k1": {"mode": "keys", "keys": "Ctrl + D"}, "k2": {"mode": "keys", "keys": "Ctrl + Enter"}}}));
        wait_for(&au, |s| s["keys"][1]["mode"] == "keys");
        assert_eq!(sets(&fake), 2);
        assert_eq!(fake.lock().typing[0], KeyTyping::own_code(KeyId::K1));
        // Quit: K1 gets its offline typing back; K2 already holds its offline typing and is not written again.
        assert_eq!(au.shutdown(), Some(true));
        assert_eq!(sets(&fake), 3);
        assert_eq!(fake.lock().typing[0], KeyTyping::parse("Ctrl + D").unwrap());
        assert_eq!(fake.lock().typing[1], KeyTyping::parse("Ctrl + Enter").unwrap());
        assert_eq!(fake.lock().typing[2], KeyTyping::own_code(KeyId::K3), "K3 (unknown, unchanged) is left alone");

        // The key asleep at quit: nothing written, the offline layer is reported as not stored.
        let au = worker();
        // A short standby on the key, so the page shows "asleep" soon after it stops sending (0.36.0).
        au.fake_handle().unwrap().lock().standby_s = 2;
        let fake = au.fake_handle().unwrap().clone();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
        wait_for(&au, |s| s["keys"][0]["mode"] == "uc");
        fake.sleep();
        let end = Instant::now() + Duration::from_secs(20);
        while au.status()["state"] != "asleep" && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        let before = sets(&fake);
        assert_eq!(au.shutdown(), Some(false));
        assert_eq!(sets(&fake), before, "nothing is written to a sleeping key");
        assert_eq!(fake.lock().typing[0], KeyTyping::own_code(KeyId::K1));
    }

    #[test]
    fn the_agent_light_stub_sends_nothing() {
        for s in [AgentLight::Idle, AgentLight::Working, AgentLight::NeedsApproval] {
            assert!(agent_light_requests(s).is_empty());
        }
    }

    #[test]
    fn settings_sanitize_to_off_and_no_actions() {
        let d = sanitize(&json!({}));
        assert_eq!(d["enabled"], false);
        assert_eq!(d["actions"]["k3"], json!({"kind":"offline","value":"","app":""}), "no UC action: the key types its offline layer");
        assert_eq!(d["offline"]["knobRight"], json!({"mode":"factory"}));
        // Settings from before the two layers: "none" becomes "offline" once.
        let old = sanitize(&json!({"actions":{"k1":{"kind":"none"}},"profiles":[{"id":"media","name":"Media","actions":{"k2":{"kind":"none"}}}]}));
        assert_eq!(old["actions"]["k1"]["kind"], "offline");
        assert_eq!(old["profiles"][0]["actions"]["k2"]["kind"], "offline");
        assert_eq!(sanitize(&json!({"layers":1,"actions":{"k1":{"kind":"none"}}}))["actions"]["k1"]["kind"], "none", "UC Nothing stays after migration");
        assert!(d["actions"].get("knobRight").is_none(), "knob turns keep the factory scroll");
        let s = sanitize(&json!({"enabled":true,"actions":{"k1":{"kind":"claudeDictation"},"k3":{"kind":"hotkey","value":"Ctrl + Enter"},"k2":{"kind":"shell","value":"x"}}}));
        assert_eq!(s["enabled"], true);
        assert_eq!(s["actions"]["k1"]["kind"], "claudeDictation");
        assert_eq!(s["actions"]["k3"]["value"], "Ctrl + Enter");
        assert_eq!(s["actions"]["k2"]["kind"], "offline", "unknown action kinds are dropped");
        assert!(validate_action("hotkey", "Ctrl + Enter", "").is_ok());
        assert!(validate_action("launch", "C:/x.exe", "").is_err(), "only reviewed kinds");
    }

    #[test]
    fn page_changes_parse_to_allowlisted_values_only() {
        assert_eq!(parse_change(&json!({"setting":"standbyTime","value":600})), Ok(Change::StandbyTime(600)));
        assert!(parse_change(&json!({"setting":"sleepTime","value":1234})).is_err());
        assert_eq!(parse_change(&json!({"setting":"retryJob","key":"knobLeft"})), Ok(Change::Retry(KeyId::KnobLeft)));
        // The key's lists change only through the layers (review 2026-11-17): no direct entry points.
        assert!(parse_change(&json!({"setting":"keyTyping","key":"k3","mode":"uc"})).is_err());
        assert!(parse_change(&json!({"setting":"knobTurn","mode":"uc"})).is_err());
        assert!(parse_change(&json!({"setting":"reboot","value":1})).is_err());
    }

    #[test]
    fn the_connect_reads_never_include_key_typing() {
        assert!(read_all().iter().all(|r| !r.slow_reply()));
        assert!(read_all().iter().all(|r| r.verified_on_hardware()));
    }

    #[test]
    fn enabled_worker_reads_the_fake_key_and_changes_settings() {
        let au = worker();
        assert_eq!(au.status()["state"], "disabled");
        assert!(au.fake_handle().unwrap().received().is_empty(), "nothing before enable");
        au.configure(&json!({"enabled": true}));
        let s = wait_for(&au, |s| s["micNr"].is_object());
        assert_eq!(s["state"], "connected");
        assert_eq!(s["standbyS"], 600);
        assert_eq!(s["battery"]["percent"], 30);
        assert_eq!(s["keys"][2]["mode"], "unknown", "key typing is not read at connect");
        assert!(au.fake_handle().unwrap().received().iter().all(|p| p[2] != 0x50));
        au.read_key(KeyId::K3).unwrap();
        let s = au.status();
        assert_eq!(s["keys"][2]["mode"], "uc", "K3 types its own F22");
        assert_eq!(s["keys"][3]["factory"], "Backspace");
        // K3 held UC's own code with no UC action: its offline typing is seeded as factory, and K3 gets it.
        wait_for(&au, |s| s["keys"][2]["mode"] == "factory");
        au.change(Change::StandbyTime(900)).unwrap();
        // K1's offline layer: with no UC action it is what K1 types while UC runs too, so it is written now.
        au.configure(&json!({"enabled": true, "offline": {"k1": {"mode": "keys", "keys": "Ctrl + D"}}}));
        au.change(Change::SleepTime(0)).unwrap();
        au.change(Change::MicNrLevel(2)).unwrap();
        // 0.37.0: changes made while one is being sent wait their turn (one at a time) and follow shortly.
        let s = wait_for(&au, |s| s["keys"][0]["mode"] == "keys" && s["standbyS"] == 900 && s["micNr"]["level"] == 2 && s["sleepS"] == 0);
        assert_eq!(s["keys"][0]["label"], "Ctrl + D", "learned from the set's echo");
        let sent50: Vec<_> = au.fake_handle().unwrap().received().into_iter().filter(|p| p[2] == 0x50).collect();
        // K3: read, set to factory (seeded; it held F22 without a UC action). K1: read first, then set.
        assert_eq!(sent50.len(), 4, "K3 read and set, K1 read and set");
        assert_eq!(s["sleepS"], 0);
        let fake = au.fake_handle().unwrap();
        assert_eq!(fake.lock().mic, MicNr { level: 2, a: 100, b: 100 }, "the unknown values are kept");
        assert!(fake.forbidden().is_empty());
        au.shutdown();
        assert_eq!(au.status()["state"], "disabled");
    }

    #[test]
    fn hotkeys_are_claimed_only_while_enabled_with_an_action_and_run_it() {
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        au.configure(&json!({"enabled": false, "actions": {"k3": {"kind":"claudeDictation"}}}));
        assert!(desk.claimed().is_empty(), "nothing while disabled");
        au.configure(&json!({"enabled": true, "actions": {"k3": {"kind":"claudeDictation"}, "k2": {"kind":"enter"}}}));
        let mut claimed = desk.claimed();
        claimed.sort();
        assert_eq!(claimed, vec![hotkey::VK_F21, hotkey::VK_F22]);
        // The live layer: K2 and K3 have UC actions, so the key is set to type their own codes.
        let s = wait_for(&au, |s| s["keys"][1]["mode"] == "uc" && s["keys"][2]["mode"] == "uc");
        assert_eq!(s["keys"][2]["mode"], "uc");
        assert_eq!(s["keys"][2]["hotkey"]["state"], "ready");
        assert_eq!(s["keys"][0]["hotkey"]["state"], "off");
        // K3 types F22 (its own code) on the fake key; the fake desktop delivers it.
        au.test_control(&json!({"op":"click","control":"k3"})).unwrap();
        let s = wait_for(&au, |s| s["events"].as_array().unwrap().iter().any(|e| e["action"].is_string()));
        let ev = s["events"].as_array().unwrap().iter().find(|e| e["action"].is_string()).unwrap().clone();
        assert_eq!(ev["control"], "K3");
        assert_eq!(ev["action"], "ran Claude: Start dictation");
        // Claude's message box gets the focus first, then Ctrl + D (never the terminal panel).
        assert_eq!(*desk.ran.lock_or_recover(), vec!["focus claude.exe", "message box claude.exe", "press Ctrl + D"]);
        // K2 now types F21 too: its action runs.
        au.test_control(&json!({"op":"click","control":"k2"})).unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while desk.ran.lock_or_recover().len() < 4 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(desk.ran.lock_or_recover()[3], "press Enter");
        // Choosing "nothing" releases the key; disabling releases all.
        au.configure(&json!({"enabled": true, "actions": {"k2": {"kind":"enter"}}}));
        assert_eq!(desk.claimed(), vec![hotkey::VK_F21]);
        au.configure(&json!({"enabled": false, "actions": {"k2": {"kind":"enter"}}}));
        assert!(desk.claimed().is_empty());
        au.shutdown();
    }

    #[test]
    fn a_key_held_by_another_app_is_reported() {
        let au = worker();
        au.test_control(&json!({"op":"hotkeyBusy","control":"k1"})).unwrap();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind":"enter"}}}));
        let s = au.status();
        assert_eq!(s["keys"][0]["hotkey"]["state"], "failed");
        au.shutdown();
    }

    #[test]
    fn a_stalling_key_pauses_and_reconnect_resumes() {
        let au = worker();
        au.configure(&json!({"enabled": true}));
        wait_for(&au, |s| s["micNr"].is_object());
        au.test_control(&json!({"op":"stallWrites","value":true})).unwrap();
        let err = au.change(Change::StandbyTime(700)).unwrap_err();
        assert!(err.contains("did not accept"), "{err}");
        assert_eq!(wait_for(&au, |s| s["state"] == "paused")["state"], "paused");
        assert!(au.change(Change::StandbyTime(700)).unwrap_err().contains("paused"), "refused at once while paused");
        au.test_control(&json!({"op":"stallWrites","value":false})).unwrap();
        au.reconnect().unwrap();
        au.change(Change::StandbyTime(700)).unwrap();
        assert_eq!(wait_for(&au, |s| s["standbyS"] == 700)["standbyS"], 700);
        au.shutdown();
    }

    /// 0.33.0 (real key 2026-11-17): the settings reads wait for the key to settle after it wakes (twice the
    /// settle time awake), not the moment it wakes.
    #[test]
    fn the_settings_reads_wait_for_the_key_to_settle_after_a_wake() {
        let settle = Duration::from_millis(500);
        let au = Au05::with_config(true, None, SessionConfig { settle, ..quick() });
        au.fake_handle().unwrap().lock().status_period = Some(Duration::from_millis(200));
        let start = Instant::now();
        au.configure(&json!({"enabled": true}));
        let s = wait_for(&au, |s| s["standbyS"].is_number());
        assert!(start.elapsed() >= Duration::from_millis(900), "read after 2 × settle, not at once: {:?}", start.elapsed());
        assert!(s["micNr"].is_object() || wait_for(&au, |s| s["micNr"].is_object())["micNr"].is_object());
        au.shutdown();
    }

    /// 1.2.2 (user report 2026-11-19: "very laggy" right after the key woke; the reads went out while K1 was
    /// being pressed and stalled): while the key is pressed or turned, UC sends it nothing of its own. The reads
    /// wait until it has not been used for 4 × the settle time.
    #[test]
    fn nothing_is_sent_to_the_key_while_it_is_in_use() {
        let settle = Duration::from_millis(150);
        let au = Au05::with_config(true, None, SessionConfig { settle, ..quick() });
        au.fake_handle().unwrap().lock().status_period = Some(Duration::from_millis(100));
        let desk = au.fake_desktop().unwrap().clone();
        au.configure(&json!({"enabled": true, "actions": {"k2": {"kind": "enter"}}}));
        assert!(desk.press(hotkey::VK_F21), "K2 is claimed");
        let using = Instant::now();
        while using.elapsed() < Duration::from_millis(1500) {
            desk.press(hotkey::VK_F21);
            assert!(!au.status()["standbyS"].is_number(), "no settings read while K2 is being pressed");
            std::thread::sleep(Duration::from_millis(80));
        }
        let stopped = Instant::now();
        wait_for(&au, |s| s["standbyS"].is_number());
        assert!(stopped.elapsed() >= Duration::from_millis(550), "read once it was left alone: {:?}", stopped.elapsed());
        au.shutdown();
    }

    /// 0.33.0: the first read gets no reply right after the wake, the others end "Paused": once the pause has
    /// cleared and the key has settled, they are read again, one at a time, and the page shows the values.
    #[test]
    fn settings_reads_that_stalled_are_read_again_after_the_pause() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        fake.lock().drop_replies = 1;
        au.configure(&json!({"enabled": true}));
        assert_eq!(wait_for(&au, |s| s["state"] == "paused")["state"], "paused");
        let s = wait_for(&au, |s| s["micNr"].is_object() && s["standbyS"].is_number() && s["sleepS"].is_number());
        assert!(s["micNr"].is_object(), "read after the pause: {s}");
        assert_eq!(s["standbyS"], fake.lock().standby_s);
        assert!(s["hooksMode"].is_number() && s["light"].is_object(), "{s}");
        assert!(s["readsFailed"].is_null());
        assert!(fake.lock().forbidden.is_empty());
        au.shutdown();
    }

    /// 0.33.0: the extra cycle stalls too: the reads stay "Not read", the page says so with Read again, and no
    /// third attempt is made. Read again reads them.
    #[test]
    fn a_second_stall_leaves_the_settings_not_read_with_read_again() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        fake.lock().drop_replies = 2;
        au.configure(&json!({"enabled": true}));
        let s = wait_for(&au, |s| s["readsFailed"].is_string());
        assert_eq!(s["readsFailed"], READS_FAILED_MESSAGE);
        assert!(s["standbyS"].is_null() && s["micNr"].is_null(), "left not read: {s}");
        // No third attempt (the pause is 200 ms here; wait well past it).
        let sent = fake.lock().received.len();
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(fake.lock().received.len(), sent, "nothing more is sent automatically");
        assert!(au.status()["standbyS"].is_null());
        // Read again (the page's button) once the pause has cleared.
        wait_for(&au, |s| s["state"] == "connected");
        au.read_all().unwrap();
        let s = wait_for(&au, |s| s["standbyS"].is_number() && s["micNr"].is_object());
        assert!(s["standbyS"].is_number(), "{s}");
        assert!(s["readsFailed"].is_null());
        au.shutdown();
    }

    /// 0.33.0: a wake from "probably switched off" reads the settings again (they may have changed).
    #[test]
    fn a_wake_from_switched_off_reads_the_settings_again() {
        // A 1 s awake window: shown as asleep after about 2.3 s, off past the key's power-off time (1 s here).
        let au = Au05::with_config(true, None, SessionConfig { awake_window: Duration::from_secs(1), ..quick() });
        let fake = au.fake_handle().unwrap().clone();
        fake.lock().status_period = Some(Duration::from_millis(200));
        fake.lock().sleep_s = 1;
        au.configure(&json!({"enabled": true}));
        assert!(wait_for(&au, |s| s["sleepS"] == 1)["standbyS"].is_number());
        fake.sleep();
        let s = wait_for(&au, |s| s["state"] == "off");
        assert_eq!(s["state"], "off", "{}", s["message"]);
        // Changed while it was off (on the key itself).
        fake.lock().standby_s = 900;
        fake.wake();
        let s = wait_for(&au, |s| s["standbyS"] == 900);
        assert_eq!(s["standbyS"], 900, "read again after the wake");
        assert_eq!(s["state"], "connected");
        au.shutdown();
    }

    /// A fake key that answers but sends nothing on its own once read (the real key is silent when idle).
    fn quiet_worker(standby_s: u32, strict_ms: u64, touch_ms: u64) -> (Arc<Au05>, FakeHandle) {
        let au = Au05::with_config(true, None, SessionConfig { awake_window: Duration::from_secs(60), ..quick() });
        au.set_gate_timing(Duration::from_millis(strict_ms), Duration::from_millis(touch_ms));
        let fake = au.fake_handle().unwrap().clone();
        fake.lock().status_period = Some(Duration::from_millis(100));
        fake.lock().standby_s = standby_s;
        au.configure(&json!({"enabled": true}));
        assert!(wait_for(&au, |s| s["micNr"].is_object() && s["standbyS"].is_number())["micNr"].is_object());
        fake.lock().status_period = None;
        std::thread::sleep(Duration::from_millis(strict_ms + 400));
        (au, fake)
    }

    /// 0.37.0 (real key 2026-11-17: a brightness change 22 s after a light change ended "Asleep" while the key
    /// was awake): a change is sent while the key's timers say awake, though it sent nothing for a while.
    #[test]
    fn a_change_is_sent_on_the_keys_timers_while_it_is_quiet() {
        let (au, _fake) = quiet_worker(600, 300, 60_000);
        au.change(Change::StandbyTime(700)).unwrap();
        let s = wait_for(&au, |s| s["changes"]["standbyTime"]["state"] == "applied");
        assert_eq!(s["standbyS"], 700);
        assert_eq!(s["changes"]["standbyTime"]["state"], "applied", "read back and the same: {}", s["changes"]);
        assert_eq!(s["strictSending"], false);
        au.shutdown();
    }

    /// 0.37.0: a send on the key's timers that stalls makes the strict rule apply; a change then waits for
    /// the key to be touched ("Touch the Vibe Key to apply") and is sent on the touch.
    #[test]
    fn a_stall_on_the_timers_makes_sending_strict_and_a_touch_applies() {
        let (au, fake) = quiet_worker(600, 300, 60_000);
        fake.lock().stall_writes = true;
        let _ = au.change(Change::StandbyTime(650));
        let s = wait_for(&au, |s| s["strictSending"] == true);
        assert_eq!(s["strictSending"], true, "{}", s["changes"]);
        assert_eq!(s["changes"]["standbyTime"]["state"], "failed");
        fake.lock().stall_writes = false;
        std::thread::sleep(Duration::from_millis(500));
        au.change(Change::LightBrightness(3)).unwrap();
        let s = au.status();
        assert_eq!(s["changes"]["lightBrightness"]["state"], "touch", "held: {}", s["changes"]);
        assert_eq!(s["changes"]["lightBrightness"]["value"], 3);
        // The key is touched: a control event (a fresh message) sends it.
        au.test_control(&json!({"op": "click", "control": "k1"})).unwrap();
        let s = wait_for(&au, |s| s["changes"]["lightBrightness"]["state"] == "applied");
        assert_eq!(s["light"]["brightness"], 3);
        au.shutdown();
    }

    /// 0.37.0: a change for a key that is really asleep (past its standby time) waits for a touch, then is
    /// "Not applied: the key was asleep" and nothing was sent.
    #[test]
    fn a_change_for_a_sleeping_key_waits_then_is_not_applied() {
        let (au, fake) = quiet_worker(1, 200, 600);
        let sent = fake.received().len();
        au.change(Change::LightBrightness(4)).unwrap();
        assert_eq!(au.status()["changes"]["lightBrightness"]["state"], "touch");
        let s = wait_for(&au, |s| s["changes"]["lightBrightness"]["state"] == "failed");
        assert_eq!(s["changes"]["lightBrightness"]["message"], "Not applied: the key was asleep");
        assert_ne!(s["light"]["brightness"], 4, "the shown value goes back");
        assert_eq!(fake.received().len(), sent, "nothing was sent");
        au.shutdown();
    }

    #[test]
    fn a_crash_in_an_action_does_not_stop_the_worker() {
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        au.test_control(&json!({"op":"actionCrash","value":true})).unwrap();
        au.configure(&json!({"enabled": true, "actions": {"k3": {"kind":"enter"}}}));
        wait_for(&au, |s| s["micNr"].is_object());
        au.test_control(&json!({"op":"click","control":"k3"})).unwrap();
        let s = wait_for(&au, |s| s["events"].as_array().unwrap().iter().any(|e| e["action"].is_string()));
        let ev = s["events"].as_array().unwrap().iter().find(|e| e["action"].is_string()).unwrap().clone();
        assert_eq!(ev["action"], "Windows / media: Enter in the focused app: The action crashed");
        au.change(Change::StandbyTime(650)).unwrap();
        assert_eq!(au.status()["state"], "connected");
        au.shutdown();
        assert!(desk.claimed().is_empty(), "quit releases every hotkey");
    }

    #[test]
    fn overlapping_configure_calls_never_start_two_workers() {
        let au = worker();
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let au = au.clone();
                std::thread::spawn(move || au.configure(&json!({"enabled": i % 2 == 0})))
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        au.configure(&json!({"enabled": true}));
        wait_for(&au, |s| s["state"] == "connected");
        au.shutdown();
        assert_eq!(au.status()["state"], "disabled");
        assert!(au.thread.lock_or_recover().is_none());
    }

    #[test]
    fn key_reads_and_changes_go_one_at_a_time() {
        let au = worker();
        au.configure(&json!({"enabled": true}));
        wait_for(&au, |s| s["micNr"].is_object());
        au.fake_handle().unwrap().lock().key_typing_delay = Duration::from_millis(400);
        let first = {
            let au = au.clone();
            std::thread::spawn(move || au.read_key(KeyId::K1))
        };
        std::thread::sleep(Duration::from_millis(100));
        let second = au.read_key(KeyId::K2);
        assert!(second.unwrap_err().contains("Another key"), "a second key read is refused while one runs");
        first.join().unwrap().unwrap();
        assert_eq!(au.fake_handle().unwrap().lock().overlapped, 0);
        au.shutdown();
    }
    fn sets(f: &FakeHandle) -> usize {
        f.received().into_iter().filter(|p| p[2] == 0x50 && p[3] == 0x00).count()
    }
    fn typing_cmds(f: &FakeHandle) -> usize {
        f.received().into_iter().filter(|p| p[2] == 0x50).count()
    }
    fn job(s: &Value, key: &str) -> Value {
        s["keys"]
            .as_array()
            .unwrap()
            .iter()
            .chain(s["turns"].as_array().into_iter().flatten())
            .find(|k| k["key"] == key)
            .map(|k| k["job"].clone())
            .unwrap_or(Value::Null)
    }

    /// Review 2026-11-17 (1): the key stores something else than written (the set's echo differs): one
    /// write, then "failed"; never repeated automatically. Try again allows exactly one more.
    #[test]
    fn an_echo_that_differs_fails_after_one_write() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        au.test_control(&json!({"op": "mangleEcho", "value": true})).unwrap();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
        let s = wait_for(&au, |s| job(s, "k1")["state"] == "failed");
        assert!(job(&s, "k1")["message"].as_str().unwrap().contains("stored something else"), "{s}");
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(sets(&fake), 1, "one write, no automatic repeat");
        au.change(Change::Retry(KeyId::K1)).unwrap();
        wait_for(&au, |s| sets(&fake) == 2 && job(s, "k1")["state"] == "failed");
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(sets(&fake), 2, "Try again is one more write");
        assert_eq!(fake.lock().overlapped, 0);
        au.shutdown();
    }

    /// Review (2, 5): Quit and disable wait for a key-typing command already sent, then store the offline
    /// layer; "stored" is never reported while one is in flight.
    #[test]
    fn quit_and_disable_wait_for_a_key_change_in_flight() {
        for quit in [true, false] {
            let au = worker();
            let fake = au.fake_handle().unwrap().clone();
            au.configure(&json!({"enabled": true}));
            wait_for(&au, |s| s["micNr"].is_object());
            fake.lock().key_typing_delay = Duration::from_millis(400);
            au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
            // The set (after the read) is on its way.
            let s = wait_for(&au, |s| job(s, "k1")["state"] == "sending" && job(s, "k1")["read"] == false);
            assert_eq!(job(&s, "k1")["state"], "sending");
            let stored = if quit {
                au.shutdown()
            } else {
                au.configure(&json!({"enabled": false, "actions": {"k1": {"kind": "claudeDictation"}}}));
                au.take_offline_result()
            };
            assert_eq!(stored, Some(true), "quit={quit}");
            assert_eq!(fake.lock().typing[0], KeyTyping::factory(), "the offline layer is on the key");
            assert_eq!(sets(&fake), 2, "the live set finished, then the offline set");
            assert_eq!(fake.lock().overlapped, 0);
            assert_eq!(au.take_abandoned(), None);
            au.shutdown();
        }
    }

    /// Review (2): out of time with a key-typing command unanswered: recorded, and the next run waits before
    /// its first key-typing command.
    #[test]
    fn an_abandoned_key_change_makes_the_next_run_wait() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        au.set_timing(Duration::from_millis(150), Duration::from_millis(1500));
        au.configure(&json!({"enabled": true}));
        wait_for(&au, |s| s["micNr"].is_object());
        fake.lock().key_typing_delay = Duration::from_millis(600);
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
        wait_for(&au, |s| job(s, "k1")["state"] == "sending");
        assert_eq!(au.shutdown(), Some(false), "out of time: not stored");
        let at = au.take_abandoned().expect("the unanswered command is recorded");

        let next = worker();
        let fake2 = next.fake_handle().unwrap().clone();
        next.set_timing(Duration::from_millis(150), Duration::from_millis(1500));
        let started = Instant::now();
        next.configure(&json!({"enabled": true, "slowAbandonedAt": at, "actions": {"k1": {"kind": "claudeDictation"}}}));
        let s = wait_for(&next, |s| s["micNr"].is_object());
        assert_eq!(s["keySync"]["waiting"], true);
        while started.elapsed() < Duration::from_millis(1100) {
            assert_eq!(typing_cmds(&fake2), 0, "no key-typing command during the wait");
            std::thread::sleep(Duration::from_millis(20));
        }
        let end = Instant::now() + Duration::from_secs(5);
        while typing_cmds(&fake2) == 0 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(started.elapsed() >= Duration::from_millis(1400), "the first one waits for the full time");
        wait_for(&next, |s| s["keys"][0]["mode"] == "uc");
        next.shutdown();
    }

    /// Review (3): after a key-typing command got no answer, nothing more is sent automatically until
    /// Reconnect (or Try again).
    #[test]
    fn a_missed_key_reply_stops_automatic_key_changes() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        au.configure(&json!({"enabled": true}));
        wait_for(&au, |s| s["micNr"].is_object());
        fake.lock().drop_replies = 1;
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
        let s = wait_for(&au, |s| s["keySync"]["blocked"] == true);
        assert_eq!(s["keySync"]["blocked"], true);
        let before = typing_cmds(&fake);
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}, "k2": {"kind": "enter"}}}));
        std::thread::sleep(Duration::from_millis(1000));
        assert_eq!(typing_cmds(&fake), before, "no automatic key-typing command after a missed reply");
        au.reconnect().unwrap();
        let s = wait_for(&au, |s| s["keys"][0]["mode"] == "uc" && s["keys"][1]["mode"] == "uc");
        assert_eq!(s["keySync"]["blocked"], false);
        assert_eq!(fake.lock().overlapped, 0);
        au.shutdown();
    }

    /// Review (4, 6): a start after the migration writes nothing for lists that already hold what UC wants,
    /// and the user's keys seed the offline layer (UC's own codes become factory).
    #[test]
    fn a_start_after_migration_keeps_the_users_keys() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        let saved: Arc<Mutex<Vec<Value>>> = Default::default();
        let sink = saved.clone();
        au.set_persist(Arc::new(move |p| {
            sink.lock_or_recover().push(p);
            Ok(())
        }));
        // The user's K1 types Ctrl + D (set up in Ulanzi Studio). Settings from before the layers.
        fake.lock().typing[0] = KeyTyping::parse("Ctrl + D").unwrap();
        let old = json!({"enabled": true, "actions": {"k1": {"kind": "none"}, "k3": {"kind": "claudeDictation"}}});
        au.configure(&old);
        // K3 (UC action) is read: it already types F22, so nothing is written. K1 is left alone.
        wait_for(&au, |s| s["keys"][2]["mode"] == "uc");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(sets(&fake), 0, "nothing written for lists that already hold what UC wants");
        assert_eq!(typing_cmds(&fake), 1, "only K3 (UC action) was read");
        // "Read from key" on K1: its keys become its offline typing; nothing is written.
        au.read_key(KeyId::K1).unwrap();
        let s = wait_for(&au, |s| s["layer"]["offline"]["k1"]["keys"] == "Ctrl + D");
        assert_eq!(s["layer"]["offline"]["k1"]["keys"], "Ctrl + D", "seeded from the key: {}", s["layer"]);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(sets(&fake), 0, "a read never writes the user's keys away");
        assert_eq!(fake.lock().typing[0], KeyTyping::parse("Ctrl + D").unwrap());
        let patches = saved.lock_or_recover().clone();
        assert!(
            patches.iter().any(|p| p["offline"]["control"] == "k1" && p["offline"]["keys"] == "Ctrl + D" && p["offline"]["seed"] == true),
            "{patches:?}"
        );
        let echo = patches.iter().find(|p| p["keyEcho"]["control"] == "k3").expect("K3's confirmed list is saved").clone();
        // The patches save as the app would.
        let mut au_settings = sanitize(&old);
        for p in &patches {
            apply_patch(&mut au_settings, p).unwrap();
        }
        assert_eq!(au_settings["offline"]["k1"]["keys"], "Ctrl + D");
        assert_eq!(au_settings["keyEcho"]["k3"], echo["keyEcho"]["keys"]);
        // A user's choice is never overwritten by a seed.
        let mut chosen = sanitize(&json!({"offline": {"k2": {"mode": "keys", "keys": "Enter"}}, "offlineSet": {"k2": true}}));
        apply_patch(&mut chosen, &json!({"offline": {"control": "k2", "mode": "factory", "seed": true}})).unwrap();
        assert_eq!(chosen["offline"]["k2"]["keys"], "Enter");
        au.shutdown();
    }

    /// Review (4): the next start with the confirmed lists saved reads and writes nothing.
    #[test]
    fn a_start_with_saved_confirmed_lists_sends_no_key_typing() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        let label = KeyTyping::own_code(KeyId::K3).label(KeyId::K3);
        au.configure(&json!({"enabled": true, "layers": 1, "actions": {"k3": {"kind": "claudeDictation"}}, "keyEcho": {"k3": label}}));
        wait_for(&au, |s| s["micNr"].is_object());
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(typing_cmds(&fake), 0);
        au.shutdown();
    }

    /// Review (7): a key that opens the quick menu needs the knob press on F19. The knob press is not checked
    /// on the real key, so it waits for the user's "Set up", then gets F19.
    #[test]
    fn the_quick_menu_keeps_the_knob_press_on_f19() {
        let l = Layers::from_settings(&json!({"actions": {"k1": {"kind": "quickMenu"}}}));
        let press = LAYER_CONTROLS.iter().position(|(_, id)| *id == KeyId::KnobPress).unwrap();
        assert!(l.uc[press]);
        assert_eq!(l.live[press], KeyTyping::own_code(KeyId::KnobPress));
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        let s = wait_for(&au, |s| job(s, "knobPress")["state"] == "check");
        assert_eq!(job(&s, "knobPress")["state"], "check", "{s}");
        let idx = KeyId::KnobPress.index();
        let press_cmds = |f: &FakeHandle| f.received().into_iter().filter(|p| p[2] == 0x50 && p.get(4).copied().unwrap_or(0) == idx).count();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(press_cmds(&fake), 0, "nothing sent for an unchecked control");
        au.change(Change::Retry(KeyId::KnobPress)).unwrap();
        wait_for(&au, |_| fake.lock().typing[idx as usize] == KeyTyping::own_code(KeyId::KnobPress));
        assert_eq!(fake.lock().typing[idx as usize], KeyTyping::own_code(KeyId::KnobPress));
        au.shutdown();
    }
    /// Verification N1: once exiting, a late settings save (a seed or confirmed list saved through the app,
    /// which re-runs `configure`) never starts the worker again, and the worker saves nothing more.
    #[test]
    fn a_save_during_exit_never_restarts_the_worker() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        let saves = Arc::new(AtomicU64::new(0));
        let (me, count) = (Arc::downgrade(&au), saves.clone());
        // As the app does: a save re-runs configure with the (still enabled) settings, after a while.
        au.set_persist(Arc::new(move |_p| {
            count.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(300));
            if let Some(au) = me.upgrade() {
                au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
            }
            Ok(())
        }));
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
        // K1 is read (a seed save starts) and then set (an echo save starts): quit meanwhile.
        wait_for(&au, |_| saves.load(Ordering::SeqCst) > 0);
        au.shutdown();
        let after = typing_cmds(&fake);
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(au.status()["state"], "disabled", "not started again");
        assert!(au.thread.lock_or_recover().is_none());
        assert_eq!(typing_cmds(&fake), after, "nothing sent after quit");
        au.configure(&json!({"enabled": true}));
        assert_eq!(au.status()["state"], "disabled");
    }

    /// Verification N2: every key-typing command (read or set) waits for the settle time after the key woke
    /// and after its last control event; so does "Read from key".
    #[test]
    fn key_typing_waits_for_the_settle_time() {
        let settle = Duration::from_millis(700);
        let au = Au05::with_config(true, None, SessionConfig { settle, ..quick() });
        let fake = au.fake_handle().unwrap().clone();
        au.configure(&json!({"enabled": true}));
        wait_for(&au, |s| s["state"] == "connected");
        let woke = Instant::now();
        au.read_key(KeyId::K3).unwrap();
        assert!(woke.elapsed() >= Duration::from_millis(600), "sent after the settle time, not at once");
        // A control event restarts it: a UC action chosen right after a key press waits again.
        au.test_control(&json!({"op": "click", "control": "k2"})).unwrap();
        let clicked = Instant::now();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
        let before = typing_cmds(&fake);
        while clicked.elapsed() < Duration::from_millis(500) {
            assert_eq!(typing_cmds(&fake), before, "no key-typing command right after a control event");
            std::thread::sleep(Duration::from_millis(10));
        }
        wait_for(&au, |s| s["keys"][0]["mode"] == "uc");
        au.shutdown();
    }

    /// Verification N5: Reconnect lifts the "no new key-typing command" stop that a quit or session end
    /// out of time leaves behind (when the app keeps running).
    #[test]
    fn reconnect_lifts_the_stop_after_a_store_out_of_time() {
        let au = worker();
        let fake = au.fake_handle().unwrap().clone();
        au.configure(&json!({"enabled": true}));
        wait_for(&au, |s| s["micNr"].is_object());
        fake.lock().key_typing_delay = Duration::from_millis(500);
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}}}));
        wait_for(&au, |s| job(s, "k1")["state"] == "sending");
        assert!(!au.store_offline(Duration::from_millis(100)), "out of time");
        au.set_layer_offline(false);
        wait_for(&au, |s| !s["keySync"]["pending"].as_bool().unwrap_or(true));
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "claudeDictation"}, "k2": {"kind": "enter"}}}));
        let before = typing_cmds(&fake);
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(typing_cmds(&fake), before, "nothing new after the store ran out of time");
        au.reconnect().unwrap();
        wait_for(&au, |s| s["keys"][1]["mode"] == "uc");
        assert_eq!(au.status()["keys"][1]["mode"], "uc");
        au.shutdown();
    }

    /// Verification: "Set up" on an unchecked control is remembered once the set's echo matched.
    #[test]
    fn set_up_is_remembered_after_a_matching_echo() {
        let press = LAYER_CONTROLS.iter().position(|(_, id)| *id == KeyId::KnobPress).unwrap();
        assert!(!Layers::from_settings(&json!({})).verified[press]);
        assert!(Layers::from_settings(&json!({"setUp": {"knobPress": true}})).verified[press]);
        let au = worker();
        let saved: Arc<Mutex<Vec<Value>>> = Default::default();
        let sink = saved.clone();
        au.set_persist(Arc::new(move |p| {
            sink.lock_or_recover().push(p);
            Ok(())
        }));
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        wait_for(&au, |s| job(s, "knobPress")["state"] == "check");
        au.change(Change::Retry(KeyId::KnobPress)).unwrap();
        wait_for(&au, |_| saved.lock_or_recover().iter().any(|p| p["setUp"]["control"] == "knobPress"));
        assert!(saved.lock_or_recover().iter().any(|p| p["setUp"]["control"] == "knobPress"));
        let mut au_settings = sanitize(&json!({}));
        apply_patch(&mut au_settings, &json!({"setUp": {"control": "knobPress"}})).unwrap();
        assert_eq!(au_settings["setUp"]["knobPress"], true);
        au.shutdown();
    }
    fn log_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("uc-au05-{name}-{}-{}", std::process::id(), now_ms()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }
    fn log_text(d: &std::path::Path) -> String {
        std::fs::read_to_string(d.join("device.log")).unwrap_or_default()
    }

    /// Real-key bug on 0.24.0: the knob press with "Open quick menu" kept typing Backspace and showed nothing.
    /// An unchecked control UC needs to write always shows "Needs a check" (also while key changes are
    /// paused), is logged once, and is never written; Set up reads it, then sets it, once.
    #[test]
    fn an_unchecked_control_with_a_uc_action_shows_needs_a_check() {
        let au = worker();
        let dir = log_dir("check");
        au.set_log_dir(dir.clone());
        let fake = au.fake_handle().unwrap().clone();
        let idx = KeyId::KnobPress.index() as usize;
        let press_cmds = |f: &FakeHandle| f.received().into_iter().filter(|p| p[2] == 0x50 && p.get(4).copied().unwrap_or(0) as usize == idx).count();
        // The user's settings: the knob press opens the quick menu; other lists confirmed before.
        let settings = json!({"enabled": true, "layers": 1, "actions": {"knobPress": {"kind": "quickMenu"}, "k1": {"kind": "claudeDictation"}},
            "keyEcho": {"k1": KeyTyping::own_code(KeyId::K1).label(KeyId::K1), "k2": "factory", "knobRight": "factory"}});
        au.configure(&settings);
        let s = wait_for(&au, |s| job(s, "knobPress")["state"] == "check");
        assert_eq!(job(&s, "knobPress")["state"], "check", "{s}");
        assert_eq!(job(&s, "knobPress")["label"], "F19");
        // Paused key changes keep it visible.
        au.status.lock_or_recover().sync.blocked = true;
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(job(&au.status(), "knobPress")["state"], "check", "still shown while key changes are paused");
        assert_eq!(press_cmds(&fake), 0, "nothing sent for the knob press before Set up");
        assert_eq!(log_text(&dir).matches("layer sync: knobPress needs Set up").count(), 1, "logged once");
        // Set up: one read, one set, done; the knob press now types F19.
        au.change(Change::Retry(KeyId::KnobPress)).unwrap();
        let s = wait_for(&au, |s| job(s, "knobPress")["state"] == "done");
        assert_eq!(job(&s, "knobPress")["state"], "done", "{s}");
        assert_eq!(fake.lock().typing[idx], KeyTyping::own_code(KeyId::KnobPress));
        let sent: Vec<_> = fake.received().into_iter().filter(|p| p[2] == 0x50 && p.get(4).copied().unwrap_or(0) as usize == idx).collect();
        assert_eq!(sent.len(), 2, "one read, one set");
        assert_eq!((sent[0][3], sent[1][3]), (0x01, 0x00), "read first, then set");
        // Knob left the same, through the knob turn's UC codes.
        au.configure(&json!({"enabled": true, "layers": 1, "knobTurn": "uc", "actions": {"knobPress": {"kind": "quickMenu"}}}));
        wait_for(&au, |s| job(s, "knobLeft")["state"] == "check");
        assert!(log_text(&dir).contains("layer sync: knobLeft needs Set up"));
        au.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every settings save re-applies the hotkeys: the "hotkey ... claimed" lines are logged only on a change.
    #[test]
    fn hotkey_lines_are_logged_only_when_they_change() {
        let au = worker();
        let dir = log_dir("hotkeys");
        au.set_log_dir(dir.clone());
        let settings = json!({"enabled": true, "actions": {"k3": {"kind": "claudeDictation"}}});
        for _ in 0..5 {
            au.configure(&settings);
        }
        assert_eq!(log_text(&dir).matches("hotkey k3 claimed").count(), 1);
        au.configure(&json!({"enabled": true, "actions": {"k3": {"kind": "enter"}}}));
        assert_eq!(log_text(&dir).matches("hotkey k3 claimed").count(), 2, "a change is logged");
        au.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// User request 2026-11-17: each profile has its own menu (the old single menu is copied to every
    /// profile); the editor edits the current profile's menu; switching profile switches the menu.
    #[test]
    fn each_profile_has_its_own_menu() {
        let a = json!([{"type": "action", "label": "A", "action": {"kind": "playPause"}}]);
        let b = json!([{"type": "action", "label": "B", "action": {"kind": "nextTrack"}}]);
        let mut s = sanitize(&json!({"menu": a}));
        for p in s["profiles"].as_array().unwrap() {
            assert_eq!(p["menu"][0]["label"], "A", "copied to {}", p["id"]);
        }
        apply_patch(&mut s, &json!({"menu": b})).unwrap();
        let menu_of = |s: &Value, id: &str| s["profiles"].as_array().unwrap().iter().find(|p| p["id"] == id).unwrap()["menu"][0]["label"].clone();
        assert_eq!(s["menu"][0]["label"], "B");
        assert_eq!(menu_of(&s, "claude"), "B", "the current profile's menu");
        assert_eq!(menu_of(&s, "chatgpt"), "A", "the others keep theirs");
        apply_patch(&mut s, &json!({"profile": "chatgpt"})).unwrap();
        assert_eq!(s["menu"][0]["label"], "A", "switching profile switches the menu");
        apply_patch(&mut s, &json!({"profile": "claude"})).unwrap();
        assert_eq!(s["menu"][0]["label"], "B");
        apply_patch(&mut s, &json!({"profileEdit": {"op": "add", "name": "Work"}})).unwrap();
        assert_eq!(menu_of(&s, "work"), "B", "a new profile starts with a copy of the current menu");
    }

    /// K1 on a highlighted Claude session in the menu: Claude opens the session, and once it is in front the
    /// dictation runs there (through the message box guard). The menu closes.
    #[test]
    fn k1_dictates_into_the_highlighted_claude_session() {
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        let id = "local_0123abcd-0000-4000-8000-000000000042";
        let list = vec![json!({"desktopId": id, "root": "C:\\UC", "project": "UC", "worktree": null, "title": "Main", "state": "idle", "last": now_ms()})];
        au.set_sessions(Arc::new(move || (list.clone(), true)));
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        desk.claude_in_front.store(true, Ordering::SeqCst);
        desk.press(hotkey::VK_F20); // open
        au.menu_input("next", None).unwrap();
        au.menu_input("next", None).unwrap(); // Claude sessions (third entry of the default menu)
        desk.press(hotkey::VK_F21); // K2: into Claude sessions
        desk.press(hotkey::VK_F20); // K1 on the project: dictate into its main session
        assert_eq!(au.status()["menu"]["view"]["open"], false);
        let end = Instant::now() + Duration::from_secs(5);
        while !desk.ran.lock_or_recover().iter().any(|r| r == "press Ctrl + D") && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ran = desk.ran.lock_or_recover().clone();
        assert_eq!(ran, vec![format!("open claude://claude.ai/epitaxy/{id}"), "focus claude.exe".into(), "message box claude.exe".into(), "press Ctrl + D".into()]);
        au.shutdown();
    }

    /// No wrap-around on any input path (real key report 2026-11-17): the window's own input (keyboard,
    /// `au05_menu` ops), the knob's F23/F24 hotkeys and its private events all stop at the first and last row.
    /// The knob press selects. A session list that changes while the menu is open does not move it.
    #[test]
    fn the_menu_never_wraps_on_any_input_path() {
        let rows = |au: &Au05| au.status()["menu"]["view"]["rows"].as_array().unwrap().len();
        let hl = |au: &Au05| au.status()["menu"]["view"]["highlight"].as_u64().unwrap() as usize;
        // 1) The window (keyboard and clicks go through the same ops).
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        desk.press(hotkey::VK_F20);
        let n = rows(&au);
        for _ in 0..n + 3 {
            au.menu_input("next", None).unwrap();
        }
        assert_eq!(hl(&au), n - 1, "window: stays on the last row");
        for _ in 0..n + 3 {
            au.menu_input("prev", None).unwrap();
        }
        assert_eq!(hl(&au), 0, "window: stays on the first row");
        // The knob press selects (Close row: closes).
        desk.press(hotkey::VK_F19);
        assert_eq!(au.status()["menu"]["view"]["open"], false);
        au.shutdown();
        // 2) The knob's own codes F23 / F24 (knob turn set to Unified Control).
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        au.configure(&json!({"enabled": true, "knobTurn": "uc", "actions": {"k1": {"kind": "quickMenu"}}}));
        desk.press(hotkey::VK_F20);
        let n = rows(&au);
        for _ in 0..n + 3 {
            assert!(desk.press(hotkey::VK_F23));
        }
        assert_eq!(hl(&au), n - 1, "F23: stays on the last row");
        for _ in 0..n + 3 {
            desk.press(hotkey::VK_F24);
        }
        assert_eq!(hl(&au), 0, "F24: stays on the first row");
        au.shutdown();
        // 3) The knob's private events (factory scroll).
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        wait_for(&au, |s| s["micNr"].is_object());
        desk.press(hotkey::VK_F20);
        let n = rows(&au);
        for _ in 0..n + 3 {
            au.test_control(&json!({"op": "click", "control": "knobRight"})).unwrap();
        }
        let s = wait_for(&au, |s| s["menu"]["view"]["highlight"].as_u64() == Some(n as u64 - 1));
        assert_eq!(s["menu"]["view"]["highlight"], n - 1, "knob events: stay on the last row");
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(hl(&au), n - 1);
        for _ in 0..n + 3 {
            au.test_control(&json!({"op": "click", "control": "knobLeft"})).unwrap();
        }
        wait_for(&au, |s| s["menu"]["view"]["highlight"].as_u64() == Some(0));
        assert_eq!(hl(&au), 0, "knob events: stay on the first row");
        au.shutdown();
        // 4) A session list that changes while the menu is open does not move it.
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        let list: Arc<Mutex<Vec<Value>>> = Default::default();
        let src = list.clone();
        au.set_sessions(Arc::new(move || (src.lock_or_recover().clone(), true)));
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        desk.press(hotkey::VK_F20);
        au.menu_input("next", None).unwrap();
        let before = au.status()["menu"]["view"].clone();
        list.lock_or_recover().push(json!({"desktopId": "local_0123abcd-0000-4000-8000-000000000001", "project": "P", "last": now_ms()}));
        au.menu_input("next", None).unwrap();
        au.menu_input("prev", None).unwrap();
        let still = |mut v: Value| {
            v["leftMs"] = Value::Null; // the idle bar's time left moves on
            v
        };
        assert_eq!(still(au.status()["menu"]["view"].clone()), still(before), "frozen while open");
        au.shutdown();
    }

    /// The menu lists Claude's own sessions (with real parents) when its session files are readable; else the
    /// hook list, and the page says so.
    #[test]
    fn the_menu_uses_claudes_session_files_when_readable() {
        let base = std::env::temp_dir().join(format!("uc-au05-claude-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("claude_x").join("claude-code-sessions").join("a").join("o");
        std::fs::create_dir_all(&dir).unwrap();
        let id = "local_0123abcd-0000-4000-8000-000000000007";
        std::fs::write(dir.join(format!("{id}.json")), json!({"sessionId": id, "title": "Orchestrator", "originCwd": "C:/x/UC", "cwd": "C:/x/UC", "isArchived": false, "lastActivityAt": 1}).to_string()).unwrap();
        let au = worker();
        au.set_claude_data(Some(base.clone()));
        au.configure(&json!({"enabled": true}));
        let s = au.status();
        assert_eq!(s["menu"]["sessions"]["source"], "desktop", "{}", s["menu"]["sessions"]);
        assert_eq!(s["menu"]["sessions"]["open"], 1);
        assert!(s["menu"]["sessions"]["groupsNote"].as_str().unwrap().contains("sidebar groups were not found"), "no config file: by folder");
        au.set_claude_data(Some(base.join("missing")));
        std::thread::sleep(super::super::claude_tree::CACHE);
        let s = au.status();
        assert_eq!(s["menu"]["sessions"]["source"], "hooks");
        assert!(s["menu"]["sessions"]["note"].as_str().unwrap().contains("not found"));
        au.shutdown();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// "Reopen where I left off" (default on) through the worker: the menu opens where it was closed; with the
    /// setting off it opens at the top; the setting is saved and shown.
    #[test]
    fn the_menu_reopens_where_it_was_left_unless_turned_off() {
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        assert_eq!(au.status()["menu"]["reopenWhereLeft"], true, "on by default");
        let hl = |au: &Au05| au.status()["menu"]["view"]["highlight"].as_u64();
        desk.press(hotkey::VK_F20); // open
        au.menu_input("next", None).unwrap();
        au.menu_input("next", None).unwrap();
        desk.press(hotkey::VK_F20); // close
        assert_eq!(au.status()["menu"]["view"]["open"], false);
        desk.press(hotkey::VK_F20);
        assert_eq!(hl(&au), Some(3), "reopened on the same entry");
        desk.press(hotkey::VK_F20);
        let mut s = sanitize(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        apply_patch(&mut s, &json!({"reopenWhereLeft": false})).unwrap();
        assert_eq!(s["reopenWhereLeft"], false);
        au.configure(&s);
        desk.press(hotkey::VK_F20);
        assert_eq!(hl(&au), Some(1), "setting off: the first entry");
        assert_eq!(au.status()["menu"]["reopenWhereLeft"], false);
        au.shutdown();
    }

    /// 0.31.0: a knob press "close" saved from the old default becomes "select", once (with a marker); after
    /// that a user who picks "close" keeps it. New settings get the marker without a change.
    #[test]
    fn a_saved_knob_press_close_becomes_select_once() {
        let mut old = json!({"enabled": true, "menuControls": {"k2": "select", "k3": "back", "knobPress": "close"}});
        assert_eq!(sanitize(&old)["menuControls"]["knobPress"], "select", "applies before it is saved too");
        assert!(migrate_menu_controls(&mut old));
        // 1.0: the old defaults K2 select and K3 back also move to the new ones (next menu, close).
        assert_eq!(old["menuControls"], json!({"k2": "nextMenu", "k3": "close", "knobPress": "select"}));
        assert_eq!(old["menuControlsVersion"], MENU_CONTROLS_VERSION);
        assert!(!migrate_menu_controls(&mut old), "only once");
        // The user picks Close again: kept, through sanitising and further patches.
        let mut au = old.clone();
        apply_patch(&mut au, &json!({"menuControls": {"knobPress": "close"}})).unwrap();
        assert_eq!(au["menuControls"]["knobPress"], "close");
        assert!(!migrate_menu_controls(&mut au));
        apply_patch(&mut au, &json!({"enabled": false})).unwrap();
        assert_eq!(sanitize(&au)["menuControls"]["knobPress"], "close", "a chosen Close stays");
        // Another role saved before is left alone; settings without any get only the marker.
        let mut back = json!({"menuControls": {"knobPress": "back"}});
        assert!(migrate_menu_controls(&mut back));
        assert_eq!(back["menuControls"]["knobPress"], "back");
        let mut none = json!({"enabled": true});
        assert!(migrate_menu_controls(&mut none));
        assert_eq!(sanitize(&none)["menuControls"]["knobPress"], "select");
        // A worker configured from old settings: the knob press selects in the open menu.
        let au = worker();
        au.configure(&json!({"enabled": true, "menuControls": {"knobPress": "close"}}));
        assert_eq!(au.status()["menu"]["controls"]["knobPress"], "select", "{}", au.status()["menu"]["controls"]);
        au.shutdown();
    }

    /// While the menu is open: K2 selects, K3 goes back (closes at the top), the knob press set to Close closes
    /// without doing anything (its default is Select). While it is closed they do their normal thing again; K2 / K3 with "Same as on its own"
    /// type their own codes for UC, which types their offline keys for them (K3: Esc).
    #[test]
    fn menu_controls_while_open_and_normal_keys_after_close() {
        let au = worker();
        let desk = au.fake_desktop().unwrap().clone();
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}}));
        let mut claimed = desk.claimed();
        claimed.sort();
        assert_eq!(claimed, vec![hotkey::VK_F19, hotkey::VK_F20, hotkey::VK_F21, hotkey::VK_F22]);
        {
            let l = au.layers.lock_or_recover();
            assert!(l.uc[1] && l.uc[2] && l.uc[3], "K2, K3 and the knob press type their own codes");
            assert_eq!(l.live[2], KeyTyping::own_code(KeyId::K3));
        }
        let ran = |n: usize| {
            let end = Instant::now() + Duration::from_secs(3);
            while desk.ran.lock_or_recover().len() < n && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(10));
            }
            desk.ran.lock_or_recover().clone()
        };
        let open = |au: &Au05| au.status()["menu"]["view"]["open"] == true;
        // Closed: K3 types Esc (its factory typing), as without UC.
        assert!(desk.press(hotkey::VK_F22));
        assert_eq!(ran(1), vec!["press Esc"]);
        // Open; K3 at the top closes; nothing runs.
        desk.press(hotkey::VK_F20);
        assert!(open(&au));
        desk.press(hotkey::VK_F22);
        assert!(!open(&au), "K3 at the top closes");
        // Open; on the Close row the knob press (select) closes without doing anything.
        desk.press(hotkey::VK_F20);
        au.menu_input("prev", None).unwrap();
        desk.press(hotkey::VK_F19);
        assert!(!open(&au));
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(desk.ran.lock_or_recover().len(), 1, "nothing ran");
        // 1.0: K2 shows the next menu (the recent sessions list) and the menu stays open; K3 closes.
        let face = |au: &Au05| au.status()["menu"]["view"]["faceIndex"].as_u64();
        desk.press(hotkey::VK_F20);
        desk.press(hotkey::VK_F21);
        assert!(open(&au));
        assert_eq!(face(&au), Some(1));
        assert_eq!(au.status()["menu"]["view"]["style"], "list");
        desk.press(hotkey::VK_F22);
        assert!(!open(&au));
        // It opens on the menu it was left on; K2 again goes round to the first; the knob press selects the first
        // entry (Claude dictation), which runs after the menu closes.
        desk.press(hotkey::VK_F20);
        assert_eq!(face(&au), Some(1));
        desk.press(hotkey::VK_F21);
        assert_eq!(face(&au), Some(0));
        desk.press(hotkey::VK_F19);
        assert!(!open(&au));
        assert_eq!(ran(4)[1..], ["focus claude.exe", "message box claude.exe", "press Ctrl + D"]);
        // Closed again: K2 types Enter (its factory typing).
        desk.press(hotkey::VK_F21);
        assert_eq!(ran(5)[4], "press Enter");
        // Roles can be changed: K3 "none" is released from the menu and keeps its offline typing on the key.
        au.configure(&json!({"enabled": true, "actions": {"k1": {"kind": "quickMenu"}}, "menuControls": {"k3": "none"}}));
        assert!(!desk.claimed().contains(&hotkey::VK_F22));
        assert!(!au.layers.lock_or_recover().uc[2]);
        au.shutdown();
    }
}
