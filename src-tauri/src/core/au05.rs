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
pub const OFF_MESSAGE: &str = "The Vibe Key is switched off or asleep. Hold the power button on its right side for 2–3 s";

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
/// The read burst after the key wakes is capped by total time; reads still queued then are dropped.
const READ_BURST_LIMIT: Duration = Duration::from_secs(10);

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
        profiles.push(json!({"id": id, "name": if name.is_empty() { id.to_string() } else { name },
                             "actions": sanitize_actions(&p["actions"])}));
    }
    if profiles.is_empty() {
        profiles = menu::default_profiles(&actions).as_array().cloned().unwrap_or_default();
    }
    let current = v["profile"]
        .as_str()
        .filter(|id| profiles.iter().any(|p| p["id"] == *id))
        .map(str::to_string)
        .unwrap_or_else(|| profiles[0]["id"].as_str().unwrap_or("").to_string());
    for p in profiles.iter_mut() {
        if p["id"] == current.as_str() {
            p["actions"] = actions.clone();
        }
    }
    let ids: Vec<String> = profiles.iter().filter_map(|p| p["id"].as_str().map(str::to_string)).collect();
    let entries = if v["menu"].is_array() { v["menu"].clone() } else { menu::default_menu() };
    json!({
        "enabled": v["enabled"].as_bool().unwrap_or(false),
        "actions": actions,
        "profiles": profiles,
        "profile": current,
        "target": v["target"].as_str().filter(|t| TARGETS.contains(t)).unwrap_or("claude"),
        "menu": menu::sanitize_menu(&entries, &ids),
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
    })
}

/// Switch to another profile: its key actions apply at once (no device write). A profile without a menu key
/// keeps the current menu key, so the menu can always switch back.
fn switch_profile(au: &mut Value, id: &str) -> Result<(), String> {
    let current = au["profile"].as_str().unwrap_or("").to_string();
    let actions = au["actions"].clone();
    let profiles = au["profiles"].as_array_mut().ok_or("No profiles")?;
    let mut next = profiles.iter().find(|p| p["id"] == id).ok_or("Choose a saved profile")?["actions"].clone();
    for p in profiles.iter_mut() {
        if p["id"] == current.as_str() {
            p["actions"] = actions.clone();
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
                // A new profile starts as a copy of the current key actions.
                let new_id = profile_id(&name, &profiles);
                let actions = au["actions"].clone();
                if let Some(ps) = au["profiles"].as_array_mut() {
                    ps.push(json!({"id": new_id, "name": name, "actions": actions}));
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
        au["menu"] = m.clone();
        lines.push(format!("page menu saved ({} entries)", m.as_array().map(|a| a.len()).unwrap_or(0)));
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
struct Known {
    battery: Option<(u16, u8)>,
    /// The status message's flags byte (class 0x7B, byte 4). Its bits are not understood yet; changes are
    /// logged so charging and full can be mapped on the real key.
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
        let (list, connected) = self.sessions.lock_or_recover().clone().map(|f| f()).unwrap_or_default();
        menu::expand(&settings["menu"], &list, connected, now_ms())
    }

    pub fn input(self: &Arc<Self>, input: Input, source: &str) {
        let settings = self.settings();
        let shown = self.shown_menu(&settings);
        let (outcome, was_open, view) = {
            let mut st = self.state.lock_or_recover();
            let was = st.open;
            let o = st.step(&shown, input, Instant::now());
            (o, was, st.view(&settings, &shown))
        };
        let open = view["open"] == true;
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
                            hotkey::dictate_send(&*d, hotkey::DictateTiming::default()).map(|seen| {
                                log.line(&format!("dictate & send: {seen}"));
                            })
                        } else {
                            d.run(&action)
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
                    std::thread::sleep(Duration::from_millis(1200));
                    me.run(Action::Entry(e), None, false);
                }
            }
        });
    }

    /// A claimed hotkey arrived (on the hotkey thread: return quickly).
    pub fn hotkey(self: &Arc<Self>, vk: u16) {
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
        if open && key == KeyId::KnobPress {
            self.input(Input::Choose, "knob press");
            return;
        }
        let action = saved_action(&settings["actions"], settings_key(key));
        match action {
            Action::QuickMenu => self.input(Input::Toggle, key.control().name()),
            Action::Nothing => {}
            action => {
                if open {
                    self.input(Input::Close, "another key");
                }
                self.run(action, Some(key), false);
            }
        }
    }

    pub fn status(&self) -> Value {
        let settings = self.settings();
        let shown = self.shown_menu(&settings);
        let view = self.state.lock_or_recover().view(&settings, &shown);
        json!({
            "view": view,
            "entries": settings["menu"],
            "profiles": settings["profiles"].as_array().map(|ps| ps.iter().map(|p| json!({"id": p["id"], "name": p["name"]})).collect::<Vec<_>>()),
            "profile": settings["profile"],
            "target": settings["target"],
            "targets": TARGETS.iter().map(|t| json!({"id": t, "name": menu::target_name(t)})).collect::<Vec<_>>(),
            "knobTurn": settings["knobTurn"],
            "knobTurnAction": settings["knobTurnAction"],
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
            config_lock: Mutex::new(()),
            config,
            log,
            menu: menu.clone(),
            layers: Arc::new(Mutex::new(Layers::from_settings(&default_settings()))),
            offline_result: Mutex::new(None),
            timing: Mutex::new((Duration::from_secs(8), Duration::from_secs(20))),
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
            "toggle" => Some(Input::Toggle),
            "pick" => Some(Input::Pick(index.ok_or("Choose a row")?)),
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
        // The knob press (F19) chooses in the menu, even when it has no action of its own.
        if enabled && any_menu && !wanted.contains(&hotkey::VK_F19) {
            wanted.push(hotkey::VK_F19);
        }
        // The knob types F23/F24: they move the menu, or run the knob turn action (UC's own scrolling).
        if enabled && settings["knobTurn"] == "uc" {
            wanted.extend([hotkey::VK_F23, hotkey::VK_F24]);
        }
        let results = d.set_hotkeys(&wanted);
        for (key, id) in ACTION_KEYS {
            let vk = own_vk(id);
            let what = match saved_action(&actions, key) {
                Action::Nothing if any_menu && id == KeyId::KnobPress => "chooses in the quick menu".to_string(),
                a => a.label(),
            };
            let line = match results.get(&vk) {
                Some(Ok(())) => format!("hotkey {key} claimed ({what})"),
                Some(Err(e)) => format!("hotkey {key} not claimed: {e}"),
                None => continue,
            };
            self.log.line(&line);
        }
        for (vk, name) in [(hotkey::VK_F23, "knob right F23"), (hotkey::VK_F24, "knob left F24")] {
            match results.get(&vk) {
                Some(Ok(())) => self.log.line(&format!("hotkey {name} claimed")),
                Some(Err(e)) => self.log.line(&format!("hotkey {name} not claimed: {e}")),
                None => {}
            }
        }
        if wanted.is_empty() {
            self.log.line("hotkeys released");
        }
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
            "battery": k.battery.map(|(mv, pct)| json!({"millivolts": mv, "percent": pct, "flags": k.battery_flags,
                "at": k.battery_at, "charging": Value::Null, "full": pct >= 100})),
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
                "lightMode": Request::SetLightMode(0).verified_on_hardware(),
                "lightBrightness": Request::SetLightBrightness(0).verified_on_hardware(),
                "lightRecord": Request::SetLightRecord { record: 0, field: LightField::Type, value: 0 }.verified_on_hardware(),
                "hooksMode": Request::SetHooksMode(0).verified_on_hardware(),
                "micNr": Request::SetMicNr(MicNr { level: 0, a: 0, b: 0 }).verified_on_hardware(),
            },
            "diagnostics": {"sent": s.sent, "stalls": s.stalls, "generation": s.generation},
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
}

enum Purpose {
    Read,
    Set(Sender<Result<(), String>>),
    /// A key-typing job (see `KeyJob`).
    Key(KeyId),
    /// A key-typing read job (before writing a list that is not known).
    KeyRead(KeyId),
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
            if s.sync.blocked || s.sync.no_new {
                continue;
            }
            let check = !l.verified[i] && !s.sync.allowed[i];
            let job = |read: bool, typing: KeyTyping, state: &'static str, message: String| {
                Some(KeyJob { typing, state, message, next: None, read })
            };
            match known {
                None => {
                    // The key confirmed this very list before (saved): no read, no write.
                    if s.sync.read_tried[idx] || l.echo[i].as_ref() == Some(&want) {
                        continue;
                    }
                    // Only what UC would write is read: a control with a UC action in the live layer, one whose
                    // layer changed in this run, or one whose saved list differs from what UC wants.
                    if !(l.dirty[i] || (!l.target_offline && l.uc[i]) || l.echo[i].is_some()) {
                        continue;
                    }
                    if check {
                        s.jobs[idx] = job(false, want, "check", "Not checked on the real key yet: press Set up".into());
                        continue;
                    }
                    s.sync.read_tried[idx] = true;
                    self.log.line(&format!("layer sync: read {name} first"));
                    s.jobs[idx] = job(true, want, "waiting", String::new());
                }
                Some(held) => {
                    if !l.seeded[i] || held == want || s.sync.attempted[idx].as_ref() == Some(&want) {
                        continue;
                    }
                    if check {
                        s.jobs[idx] = job(false, want, "check", "Not checked on the real key yet: press Set up".into());
                        continue;
                    }
                    self.log.line(&format!("layer sync: {name} -> {} ({})", want.label(*id), if l.target_offline { "offline" } else { "live" }));
                    s.sync.attempted[idx] = Some(want.clone());
                    s.jobs[idx] = job(false, want, "waiting", String::new());
                }
            }
        }
    }

    /// Send the next waiting key job when the key is awake, no other key-typing command is pending, and no
    /// guard holds key typing back.
    /// The settle time has passed since the key woke and since its last control event (the driver applies it to
    /// sets only; UC applies it to key-typing reads too).
    fn settled(&self) -> bool {
        self.quiet_from.lock_or_recover().is_none_or(|t| t.elapsed() >= self.config.settle)
    }

    fn start_jobs(&self, session: &mut Session<Box<dyn Transport>>, tickets: &mut HashMap<u64, Purpose>) {
        if session.state() != SessionState::Awake || session.has_pending(Request::slow_reply) || !self.settled() {
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
        let (mut last_msg, mut seen_before, mut was_off): (Option<Instant>, u64, bool) = (None, 0, false);
        let mut tickets: HashMap<u64, Purpose> = HashMap::new();
        let mut read_done = false;
        let mut read_burst: Option<(Instant, Vec<u64>)> = None;
        let mut last_state = None;
        // A "Read from key" waiting for the settle time (one at a time).
        let mut deferred_read: Option<(KeyId, Sender<Result<(), String>>, Instant)> = None;
        let mut was_awake = false;
        self.set_state("waiting", "Connected. Press a key or turn the knob to wake the Vibe Key");
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
                        read_done = false;
                        let mut s = self.status.lock_or_recover();
                        s.last_error = None;
                        // Reconnect also lifts the guard after a failed key change (one more attempt each).
                        s.sync.blocked = false;
                        s.sync.no_new = false;
                        s.sync.attempted = Default::default();
                        s.sync.read_tried = Default::default();
                    }
                    Ok(Cmd::Change(c, reply)) => self.submit_change(&mut session, &mut tickets, c, reply),
                    Ok(Cmd::ReadAll(reply)) => {
                        self.log.line("page read all");
                        let mut result = Ok(());
                        for r in read_all_settings() {
                            let t = TICKETS.fetch_add(1, Ordering::Relaxed);
                            match session.submit(t, r) {
                                Ok(()) => {
                                    tickets.insert(t, Purpose::Read);
                                }
                                Err(f) => {
                                    result = Err(f.message());
                                    break;
                                }
                            }
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
            // The settle time starts when the key wakes (control events restart it in `handle`).
            let awake = session.state() == SessionState::Awake;
            if awake && !was_awake {
                *self.quiet_from.lock_or_recover() = Some(Instant::now());
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
            self.sync_layers();
            self.start_jobs(&mut session, &mut tickets);
            let state = session.state();
            if state == SessionState::Awake && !read_done {
                read_done = true;
                let mut burst = Vec::new();
                for r in read_all() {
                    let t = TICKETS.fetch_add(1, Ordering::Relaxed);
                    if session.submit(t, r).is_ok() {
                        tickets.insert(t, Purpose::Read);
                        burst.push(t);
                    }
                }
                read_burst = Some((Instant::now(), burst));
            }
            // Cap the read burst by total time: reads still queued after the limit are dropped.
            if let Some((started, burst)) = &read_burst {
                if started.elapsed() >= READ_BURST_LIMIT {
                    for o in session.cancel_queued(|t, _| burst.contains(&t)) {
                        self.handle(o, &mut tickets);
                    }
                    read_burst = None;
                }
            }
            if outputs_seen > seen_before {
                last_msg = Some(Instant::now());
                seen_before = outputs_seen;
            }
            // Switched off (2026-11-17, real key: after its power-off time the key turns itself off and only its
            // power button brings it back): no message since the connection opened for longer than the
            // standby time + 2 minutes, or none for longer than the power-off time.
            let off = {
                let k = &self.status.lock_or_recover().known;
                let quiet = last_msg.unwrap_or(opened_at).elapsed();
                let standby = Duration::from_secs(u64::from(k.standby_s.unwrap_or(600)) + 120);
                let power_off = k.sleep_s.filter(|s| *s > 0).map(|s| Duration::from_secs(u64::from(s)));
                matches!(state, SessionState::WaitingForKey | SessionState::Asleep)
                    && ((last_msg.is_none() && quiet > standby) || power_off.is_some_and(|p| quiet > p))
            };
            if off && !was_off {
                was_off = true;
                self.set_state("off", OFF_MESSAGE);
            }
            if !off && was_off {
                was_off = false;
                last_state = None; // show the session's own state again
            }
            if !off && last_state != Some(state) {
                last_state = Some(state);
                let (code, msg) = match state {
                    SessionState::WaitingForKey => ("waiting", "Connected. Press a key or turn the knob to wake the Vibe Key"),
                    SessionState::Awake => ("connected", "Connected"),
                    SessionState::Asleep => ("asleep", "The Vibe Key is asleep. Press a key to wake it"),
                    SessionState::Paused => ("paused", "A reply was slow. Sending is paused for 30 seconds"),
                    SessionState::Suspended => ("suspended", "Sending stopped after repeated slow replies. Use Reconnect"),
                };
                self.set_state(code, msg);
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
    ) {
        let known = self.status.lock_or_recover().known.clone();
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
                self.log.line(&format!("queued {name}"));
                tickets.insert(t, Purpose::Set(reply));
                if let Some(r) = read_back {
                    let rt = TICKETS.fetch_add(1, Ordering::Relaxed);
                    if session.submit(rt, r).is_ok() {
                        tickets.insert(rt, Purpose::Read);
                    }
                }
            }
            Err(f) => {
                self.log.line(&format!("refused {name}: {}", f.message()));
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
            Request::GetStandbyTime | Request::SetStandbyTime(_) => k.standby_s = Some(protocol::seconds(r)),
            Request::GetSleepTime | Request::SetSleepTime(_) => k.sleep_s = Some(protocol::seconds(r)),
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

    /// The reported "stuck" choice (2026-11-17): choosing a UC action must never wait for the key. The action
    /// is saved at once (before the key's list is read), the key change waits visibly while the key is
    /// asleep, and the page always ends in a clear state: done, or the error.
    #[test]
    fn choosing_a_uc_action_never_waits_for_the_key() {
        let au = worker();
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
        assert!(s["keys"][0]["job"]["message"].as_str().unwrap().contains("did not answer"));
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
        let s = wait_for(&au, |s| s["keys"][0]["mode"] == "keys" && s["standbyS"] == 900);
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
        assert_eq!(*desk.ran.lock_or_recover(), vec!["focus claude.exe, press Ctrl + D".to_string()]);
        // K2 now types F21 too: its action runs.
        au.test_control(&json!({"op":"click","control":"k2"})).unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while desk.ran.lock_or_recover().len() < 2 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(desk.ran.lock_or_recover()[1], "press Enter");
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
}
