//! Vibe Key quick menu (user request 2026-11-17, "a bit similar to what Logitech have").
//!
//! A key with the action "Open quick menu" opens a small menu window at a fixed place on a screen (not at the
//! mouse). Knob turns move the highlight, the knob press chooses, the menu key again (or about 8 s without
//! input) closes it. Esc, the arrow keys, Enter and mouse clicks work in the window too.
//!
//! Everything here is data and logic only: the menu model in settings.json (`au05.menu`, `au05.profiles`,
//! `au05.profile`, `au05.target`), its checks, and the menu's state machine. The window itself is behind
//! [`MenuView`]; tests and quiet instances have no view, so nothing ever appears on the desktop.
use super::au05_hotkey::{self as hotkey, TARGETS};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The menu closes after this long without input.
pub const CLOSE_AFTER: Duration = Duration::from_secs(8);
pub const MAX_DEPTH: usize = 3;
pub const MAX_ENTRIES: usize = 64;
pub const MAX_LABEL: usize = 60;
pub const MAX_PROFILES: usize = 12;
/// Entry types. "claudeSessions" is the live list of Claude sessions (from Claude Code hooks, see
/// `claude_sessions`), expanded when the menu is shown; "claudeSession" opens one session.
pub const ENTRY_TYPES: [&str; 6] = ["action", "profile", "target", "submenu", "claudeSessions", "claudeSession"];

/// Shows the menu window. Implemented by the app shell (a Tauri window); never installed in tests, fake or
/// quiet instances.
pub trait MenuView: Send + Sync {
    /// Show or update the menu. `first` is true when it has just opened (place it and take focus).
    fn show(&self, view: &Value, first: bool);
    fn hide(&self);
}

pub fn target_name(id: &str) -> &'static str {
    hotkey::app(id).map(|a| a.name).unwrap_or("Claude")
}

fn action(kind: &str) -> Value {
    json!({"kind": kind, "value": "", "app": ""})
}
fn act(label: &str, kind: &str) -> Value {
    json!({"type": "action", "label": label, "action": action(kind)})
}

/// The shipped menu (orchestrator brief 2026-11-17).
pub fn default_menu() -> Value {
    json!([
        act("Claude dictation", "claudeDictation"),
        act("Claude new session", "claudeNewSession"),
        {"type": "claudeSessions", "label": "Claude sessions", "then": "none"},
        act("ChatGPT dictation", "chatgptDictation"),
        {"type": "submenu", "label": "Switch profile", "entries": [
            {"type": "profile", "label": "Claude", "profile": "claude"},
            {"type": "profile", "label": "ChatGPT", "profile": "chatgpt"},
            {"type": "profile", "label": "Media", "profile": "media"},
        ]},
        {"type": "submenu", "label": "Target app", "entries": [
            {"type": "target", "label": "Claude", "target": "claude"},
            {"type": "target", "label": "ChatGPT", "target": "chatgpt"},
        ]},
        act("Play / pause", "playPause"),
        act("Screenshot", "screenshot"),
    ])
}

/// The shipped profiles. The first ("Claude") takes the user's current key actions, so nothing changes on
/// an update; the others apply only when the user switches to them.
pub fn default_profiles(current_actions: &Value) -> Value {
    let set = |k1: &str, k2: &str| {
        json!({"k1": action(k1), "k2": action(k2), "k3": action("none"), "knobPress": action("none")})
    };
    json!([
        {"id": "claude", "name": "Claude", "actions": current_actions},
        {"id": "chatgpt", "name": "ChatGPT", "actions": set("chatgptDictation", "chatgptSend")},
        {"id": "media", "name": "Media", "actions": set("playPause", "nextTrack")},
    ])
}

pub fn clean_label(v: &Value) -> String {
    let s: String = v.as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(MAX_LABEL).collect();
    s.trim().to_string()
}

/// A profile id: a-z, 0-9 and '-', at most 24 characters.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 24 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_session(id: &str) -> bool {
    super::claude_sessions::valid_desktop_id(id)
}
/// The action run after a Claude session opens (for example dictation): a catalogue id or "none".
fn then_action(v: &Value) -> &str {
    v.as_str().filter(|k| hotkey::entry(k).is_some()).unwrap_or("none")
}

pub fn age_text(ms: u64) -> String {
    age(ms)
}
fn age(ms: u64) -> String {
    match ms / 60_000 {
        0 => "now".into(),
        m if m < 60 => format!("{m} min"),
        m => format!("{} h", m / 60),
    }
}

/// The menu with each "Claude sessions" entry replaced by a sub-menu: the sessions grouped by project (one
/// project: the sessions directly), most recent first, at most 30. `sessions` from `claude_sessions`.
/// `desktop`: the Claude desktop app's own session list (`claude_tree`), used when it could be read; else the
/// hook list.
pub fn expand(menu: &Value, sessions: &[Value], connected: bool, now_ms: u64, desktop: Option<&super::claude_tree::Desktop>) -> Value {
    let Some(list) = menu.as_array() else { return json!([]) };
    Value::Array(
        list.iter()
            .map(|e| match e["type"].as_str() {
                Some("claudeSessions") if desktop.is_some() => {
                    let working: Vec<String> = sessions
                        .iter()
                        .filter(|s| s["state"] == "working")
                        .filter_map(|s| s["desktopId"].as_str().map(str::to_string))
                        .collect();
                    let d = desktop.expect("checked");
                    let mut entries = super::claude_tree::tree(&d.sessions, d.groups.as_ref().ok(), &working, &e["then"], now_ms);
                    if entries.is_empty() {
                        entries.push(json!({"type": "note", "label": "No open Claude sessions"}));
                    }
                    json!({"type": "submenu", "label": e["label"], "entries": entries})
                }
                Some("claudeSessions") => {
                    let entries = if sessions.is_empty() {
                        let note = if connected { "No Claude sessions yet" } else { "Connect Claude sessions on the Vibe Key page" };
                        vec![json!({"type": "note", "label": note})]
                    } else {
                        session_tree(sessions, &e["then"], now_ms)
                    };
                    json!({"type": "submenu", "label": e["label"], "entries": entries})
                }
                Some("submenu") => {
                    let mut x = e.clone();
                    x["entries"] = expand(&e["entries"], sessions, connected, now_ms, desktop);
                    x
                }
                _ => e.clone(),
            })
            .collect(),
    )
}

/// The Claude sessions as a tree (2026-11-17): level 1 the projects (repositories, by folder name, most recent
/// activity first); under each, the session in the main checkout first, then the worktree sessions, most
/// recent first. A project node carries its main (else most recent) session, so K1 can dictate into it. A
/// node is generic (`entries`), so a real parent link can nest sessions later. At most 30 sessions.
pub fn session_tree(sessions: &[Value], then: &Value, now_ms: u64) -> Vec<Value> {
    let shown: Vec<&Value> = sessions.iter().take(30).collect();
    let key = |s: &Value| s["root"].as_str().or(s["project"].as_str()).unwrap_or("Other").to_lowercase();
    let mut roots: Vec<String> = Vec::new();
    for s in &shown {
        if !roots.contains(&key(s)) {
            roots.push(key(s));
        }
    }
    let row = |s: &Value| {
        // Its title (named in Claude), else its first prompt, else its worktree or project folder.
        let label = s["title"].as_str().filter(|l| !l.is_empty())
            .or(s["label"].as_str().filter(|l| !l.is_empty()))
            .or(s["worktree"].as_str().filter(|l| !l.is_empty()))
            .or(s["project"].as_str().filter(|l| !l.is_empty()))
            .unwrap_or("Untitled session");
        let working = s["state"] == "working";
        let since = now_ms.saturating_sub(s["last"].as_u64().unwrap_or(now_ms));
        let place = s["worktree"].as_str().map(|w| format!("{w} · ")).unwrap_or_else(|| "main · ".into());
        json!({"type": "claudeSession", "label": label, "session": s["desktopId"], "then": then, "working": working,
               "detail": format!("{}{place}{}", if working { "● working · " } else { "" }, age(since))})
    };
    roots
        .iter()
        .map(|r| {
            let mine: Vec<&Value> = shown.iter().copied().filter(|s| key(s) == *r).collect();
            let mut ordered: Vec<&Value> = mine.iter().copied().filter(|s| s["worktree"].as_str().is_none()).collect();
            ordered.extend(mine.iter().copied().filter(|s| s["worktree"].as_str().is_some()));
            let main = ordered.first().map(|s| s["desktopId"].clone()).unwrap_or(Value::Null);
            let working = ordered.iter().any(|s| s["state"] == "working");
            let name = mine.first().and_then(|s| s["project"].as_str()).unwrap_or("Other");
            json!({"type": "submenu", "label": name, "project": true, "session": main, "working": working,
                   "detail": if working { "● working" } else { "" }, "entries": ordered.iter().map(|s| row(s)).collect::<Vec<_>>()})
        })
        .collect()
}

/// Keep only valid entries (unknown types, unknown actions, missing profiles, too deep or too many are
/// dropped). `profiles` is the list of profile ids.
pub fn sanitize_menu(v: &Value, profiles: &[String]) -> Value {
    let mut count = 0;
    Value::Array(clean_level(v, profiles, 1, &mut count))
}

fn clean_level(v: &Value, profiles: &[String], depth: usize, count: &mut usize) -> Vec<Value> {
    let mut out = Vec::new();
    for e in v.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        if *count >= MAX_ENTRIES {
            break;
        }
        let label = clean_label(&e["label"]);
        let cleaned = match e["type"].as_str().unwrap_or("") {
            "action" => {
                let a = &e["action"];
                let kind = a["kind"].as_str().unwrap_or("");
                let value = a["value"].as_str().unwrap_or("");
                let app = a["app"].as_str().unwrap_or("");
                // The menu cannot open itself; "nothing" is not an entry.
                if kind == hotkey::QUICK_MENU || kind == "none" || hotkey::validate(kind, value, app).is_err() {
                    continue;
                }
                let custom = kind == "hotkey";
                let fallback = hotkey::entry(kind).map(|x| format!("{} {}", x.group, x.label)).unwrap_or_else(|| value.to_string());
                json!({"type": "action", "label": if label.is_empty() { fallback } else { label },
                       "action": {"kind": kind, "value": if custom { value } else { "" }, "app": if custom { app } else { "" }}})
            }
            "profile" => {
                let id = e["profile"].as_str().unwrap_or("");
                if !profiles.iter().any(|p| p == id) {
                    continue;
                }
                json!({"type": "profile", "label": if label.is_empty() { id.to_string() } else { label }, "profile": id})
            }
            "target" => {
                let t = e["target"].as_str().unwrap_or("");
                if !TARGETS.contains(&t) {
                    continue;
                }
                json!({"type": "target", "label": if label.is_empty() { target_name(t).to_string() } else { label }, "target": t})
            }
            "submenu" => {
                if depth >= MAX_DEPTH {
                    continue;
                }
                *count += 1;
                let entries = clean_level(&e["entries"], profiles, depth + 1, count);
                out.push(json!({"type": "submenu", "label": if label.is_empty() { "Sub-menu".to_string() } else { label }, "entries": entries}));
                continue;
            }
            "claudeSessions" => {
                json!({"type": "claudeSessions", "label": if label.is_empty() { "Claude sessions".to_string() } else { label },
                       "then": then_action(&e["then"])})
            }
            "claudeSession" => {
                let id = e["session"].as_str().unwrap_or("");
                json!({"type": "claudeSession", "label": if label.is_empty() { "Claude session".to_string() } else { label },
                       "session": if valid_session(id) { id } else { "" }, "then": then_action(&e["then"])})
            }
            _ => continue,
        };
        *count += 1;
        out.push(cleaned);
    }
    out
}

/// Controls that can have a role while the menu is open, and the roles (user request 2026-11-17).
/// Knob press selects (user feedback 2026-11-17: "It is natural to keep the thumb there").
pub const CONTROL_KEYS: [(&str, &str); 3] = [("k2", "select"), ("k3", "back"), ("knobPress", "select")];
pub const ROLES: [&str; 4] = ["select", "back", "close", "none"];

/// The menu roles, defaults filled in: K2 select, K3 back, knob press select.
pub fn sanitize_controls(v: &Value) -> Value {
    Value::Object(
        CONTROL_KEYS
            .iter()
            .map(|(k, def)| (k.to_string(), json!(v[k].as_str().filter(|r| ROLES.contains(r)).unwrap_or(def))))
            .collect(),
    )
}
/// The role of a control while the menu is open ("none" for K1 and unknown controls).
pub fn role<'a>(controls: &'a Value, key: &str) -> &'a str {
    let def = CONTROL_KEYS.iter().find(|(k, _)| *k == key).map(|(_, r)| *r).unwrap_or("none");
    controls[key].as_str().filter(|r| ROLES.contains(r)).unwrap_or(def)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// The menu key: open, or close when open.
    Toggle,
    Next,
    Prev,
    Choose,
    /// Up one level (the "Back" row, Backspace).
    Back,
    /// K1 on a highlighted session or project: open it and run this catalogue action (dictation).
    Dictate(String),
    Close,
    /// A mouse click on a row.
    Pick(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Nothing,
    Opened,
    Moved,
    Closed,
    /// Run this saved action (`{kind, value, app}`) after the menu has closed.
    Run(Value, String),
    Profile(String, String),
    Target(String, String),
    /// Open this Claude session (desktop id), then run the follow-up action (a catalogue id or "none").
    Session(String, String),
    /// Chosen but not available (stays open with a note).
    Unavailable(String),
}

/// A stable id for a menu entry (the list changes between opens, so rows are not enough): a sidebar group, a
/// Claude session (a session's sub-menu and the session itself share it), a project folder, a profile...
pub fn entry_key(e: &Value) -> String {
    let s = |k: &str| e[k].as_str().unwrap_or("").to_string();
    match e["type"].as_str().unwrap_or("") {
        "claudeSession" => format!("s:{}", s("session")),
        "submenu" if e["group"].is_string() => format!("g:{}", s("group")),
        "submenu" if e["project"] == true => format!("p:{}", s("label").to_lowercase()),
        "submenu" if e["session"].is_string() => format!("s:{}", s("session")),
        "profile" => format!("profile:{}", s("profile")),
        "target" => format!("target:{}", s("target")),
        "action" => format!("a:{}:{}", e["action"]["kind"].as_str().unwrap_or(""), e["action"]["value"].as_str().unwrap_or("")),
        t => format!("{t}:{}", s("label")),
    }
}

/// Where the menu was when it closed (user request 2026-11-17, "remember where we were standing"): the
/// sub-menus by id (and row, for when one is gone), and the highlighted entry.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Spot {
    path: Vec<(String, usize)>,
    row: usize,
    key: Option<String>,
}

/// The menu's state machine. Rows: row 0 is "Close" at the top level and "Back" in a sub-menu (fixed, user
/// request 2026-11-17); row i is entry i-1.
#[derive(Clone, Debug, Default)]
pub struct MenuState {
    pub open: bool,
    path: Vec<usize>,
    pub highlight: usize,
    last_input: Option<Instant>,
    pub note: Option<String>,
    /// "Reopen where I left off" (setting `reopenWhereLeft`, default on).
    pub remember: bool,
    /// Whose menu this is (the profile): the memory is per profile menu.
    pub scope: String,
    /// The last spot per profile menu, in memory only (not kept over an app restart).
    memory: HashMap<String, Spot>,
    closing: bool,
}

impl MenuState {
    /// The entries of the current level. A path that no longer fits the menu (it was edited) goes back to
    /// the top.
    fn level(&mut self, menu: &Value) -> Vec<Value> {
        let mut cur = menu.as_array().cloned().unwrap_or_default();
        for (depth, &i) in self.path.clone().iter().enumerate() {
            match cur.get(i).filter(|e| e["type"] == "submenu") {
                Some(e) => cur = e["entries"].as_array().cloned().unwrap_or_default(),
                None => {
                    self.path.truncate(depth);
                    self.highlight = 0;
                    return self.level(menu);
                }
            }
        }
        cur
    }
    fn offset(&self) -> usize {
        1
    }
    fn rows(&mut self, menu: &Value) -> usize {
        self.level(menu).len() + self.offset()
    }
    /// The path and highlight are kept until `step` has remembered them.
    fn close(&mut self) -> Outcome {
        self.open = false;
        self.closing = true;
        self.note = None;
        Outcome::Closed
    }
    /// Remember where the menu closed (Close highlighted at the top: nothing to come back to).
    fn remember_spot(&mut self, menu: &Value) {
        if !self.remember || (self.path.is_empty() && self.highlight == 0) {
            self.memory.remove(&self.scope);
            return;
        }
        let mut cur = menu.as_array().cloned().unwrap_or_default();
        let mut path = Vec::new();
        for &i in &self.path {
            let Some(e) = cur.get(i).cloned() else { break };
            path.push((entry_key(&e), i));
            cur = e["entries"].as_array().cloned().unwrap_or_default();
        }
        let key = self.highlight.checked_sub(1).and_then(|i| cur.get(i)).map(entry_key);
        self.memory.insert(self.scope.clone(), Spot { path, row: self.highlight, key });
    }
    /// Open at the remembered spot: each sub-menu found by its id; where one is gone (or no longer a
    /// sub-menu), the nearest surviving level with that row (clamped); the highlighted entry by its id, else
    /// its row (clamped). Nothing remembered: the first entry at the top.
    fn restore(&mut self, menu: &Value) {
        self.path.clear();
        self.highlight = usize::from(menu.as_array().is_some_and(|a| !a.is_empty()));
        let Some(spot) = self.memory.get(&self.scope).cloned().filter(|_| self.remember) else { return };
        let mut cur = menu.as_array().cloned().unwrap_or_default();
        let clamp = |row: usize, len: usize| row.min(len);
        for (key, i) in &spot.path {
            match cur.iter().position(|e| entry_key(e) == *key) {
                Some(at) if cur[at]["type"] == "submenu" => {
                    self.path.push(at);
                    cur = cur[at]["entries"].as_array().cloned().unwrap_or_default();
                }
                Some(at) => {
                    // A session that had sub-sessions and has none now: highlight it.
                    self.highlight = at + 1;
                    return;
                }
                None => {
                    self.highlight = clamp(i + 1, cur.len());
                    return;
                }
            }
        }
        self.highlight = match spot.key.and_then(|k| cur.iter().position(|e| entry_key(e) == k)) {
            Some(at) => at + 1,
            None => clamp(spot.row, cur.len()),
        };
    }

    pub fn step(&mut self, menu: &Value, input: Input, now: Instant) -> Outcome {
        let o = self.step_inner(menu, input, now);
        if self.closing {
            self.closing = false;
            self.remember_spot(menu);
            self.path.clear();
            self.highlight = 0;
        }
        o
    }
    /// True when the menu has been open without input for `CLOSE_AFTER`.
    pub fn expired(&self, now: Instant) -> bool {
        self.open && self.last_input.is_some_and(|t| now.duration_since(t) >= CLOSE_AFTER)
    }

    fn step_inner(&mut self, menu: &Value, input: Input, now: Instant) -> Outcome {
        if !self.open {
            if input != Input::Toggle {
                return Outcome::Nothing;
            }
            self.open = true;
            // Where it was left (setting on), else the first entry (after Close).
            self.restore(menu);
            self.note = None;
            self.last_input = Some(now);
            return Outcome::Opened;
        }
        self.last_input = Some(now);
        let rows = self.rows(menu).max(1);
        self.highlight = self.highlight.min(rows - 1);
        match input {
            Input::Toggle | Input::Close => self.close(),
            // No wrap-around (2026-11-17): at the first or last row, further turns stay put.
            Input::Next => {
                self.highlight = (self.highlight + 1).min(rows - 1);
                self.note = None;
                Outcome::Moved
            }
            Input::Prev => {
                self.highlight = self.highlight.saturating_sub(1);
                self.note = None;
                Outcome::Moved
            }
            Input::Dictate(then) => match self.highlighted_session(menu) {
                Some(id) => {
                    self.close();
                    Outcome::Session(id, then)
                }
                None => Outcome::Nothing,
            },
            Input::Back => match self.path.pop() {
                None => self.close(),
                Some(i) => {
                    self.highlight = i + self.offset();
                    self.note = None;
                    Outcome::Moved
                }
            },
            Input::Pick(i) => {
                if i >= rows {
                    return Outcome::Nothing;
                }
                self.highlight = i;
                self.step_inner(menu, Input::Choose, now)
            }
            Input::Choose => {
                // Row 0: Close (top level) or Back (sub-menu).
                if self.highlight == 0 {
                    return self.step_inner(menu, Input::Back, now);
                }
                let level = self.level(menu);
                let Some(e) = level.get(self.highlight - self.offset()).cloned() else {
                    return Outcome::Nothing; // an empty menu
                };
                let label = e["label"].as_str().unwrap_or("").to_string();
                match e["type"].as_str().unwrap_or("") {
                    "submenu" => {
                        self.path.push(self.highlight - self.offset());
                        self.highlight = if e["entries"].as_array().is_some_and(|a| !a.is_empty()) { 1 } else { 0 };
                        self.note = None;
                        Outcome::Moved
                    }
                    "action" => {
                        self.close();
                        Outcome::Run(e["action"].clone(), label)
                    }
                    "profile" => {
                        self.close();
                        Outcome::Profile(e["profile"].as_str().unwrap_or("").to_string(), label)
                    }
                    "target" => {
                        self.close();
                        Outcome::Target(e["target"].as_str().unwrap_or("").to_string(), label)
                    }
                    "claudeSession" if super::claude_sessions::valid_desktop_id(e["session"].as_str().unwrap_or("")) => {
                        self.close();
                        Outcome::Session(e["session"].as_str().unwrap_or("").to_string(), e["then"].as_str().unwrap_or("none").to_string())
                    }
                    _ => {
                        let msg = if e["type"] == "note" { label } else { "This Claude session is no longer available".to_string() };
                        self.note = Some(msg.clone());
                        Outcome::Unavailable(msg)
                    }
                }
            }
        }
    }

    /// The Claude session behind the highlighted row: a session, or a project's main session.
    pub fn highlighted_session(&mut self, menu: &Value) -> Option<String> {
        if !self.open || self.highlight == 0 {
            return None;
        }
        let level = self.level(menu);
        let id = level.get(self.highlight - 1)?["session"].as_str()?.to_string();
        super::claude_sessions::valid_desktop_id(&id).then_some(id)
    }

    /// What the window shows. `settings` is the sanitized `au05` settings (for the header); `menu` is the
    /// menu as shown (with the Claude sessions expanded).
    pub fn view(&mut self, settings: &Value, menu: &Value) -> Value {
        let level = self.level(menu);
        let profile = settings["profile"].as_str().unwrap_or("");
        let target = settings["target"].as_str().unwrap_or("claude");
        let profile_name = settings["profiles"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["id"] == profile))
            .and_then(|p| p["name"].as_str())
            .unwrap_or("")
            .to_string();
        let mut rows = Vec::new();
        if self.path.is_empty() {
            rows.push(json!({"label": "Close", "type": "close"}));
        } else {
            rows.push(json!({"label": "Back", "type": "back"}));
        }
        for e in &level {
            let t = e["type"].as_str().unwrap_or("");
            let detail = match t {
                "action" => {
                    let a = &e["action"];
                    match hotkey::entry(a["kind"].as_str().unwrap_or("")) {
                        Some(x) if x.status != "verified" => "unverified".to_string(),
                        _ => String::new(),
                    }
                }
                "claudeSession" | "submenu" => e["detail"].as_str().unwrap_or("").to_string(),
                _ => String::new(),
            };
            let current = (t == "profile" && e["profile"] == profile) || (t == "target" && e["target"] == target);
            rows.push(json!({"label": e["label"], "type": t, "detail": detail, "current": current, "working": e["working"] == true}));
        }
        let mut title = vec!["Quick menu".to_string()];
        let mut cur = menu.as_array().cloned().unwrap_or_default();
        for &i in &self.path {
            if let Some(e) = cur.get(i).cloned() {
                title.push(e["label"].as_str().unwrap_or("").to_string());
                cur = e["entries"].as_array().cloned().unwrap_or_default();
            }
        }
        json!({
            "open": self.open,
            "title": title,
            "rows": rows,
            "highlight": self.highlight,
            "profile": profile_name,
            "target": target_name(target),
            "note": self.note,
            "closeAfterS": CLOSE_AFTER.as_secs(),
            // The hints at the bottom: what each control does while the menu is open.
            "hints": hints(&settings["menuControls"]),
        })
    }
}

/// "K2 select · K3 back · Knob press select" (from the roles), with the knob turn first.
pub fn hints(controls: &Value) -> String {
    let mut parts = vec!["Turn: move".to_string()];
    for (k, _) in CONTROL_KEYS {
        let r = role(controls, k);
        if r != "none" {
            let name = if k == "knobPress" { "Knob press" } else if k == "k2" { "K2" } else { "K3" };
            parts.push(format!("{name} {r}"));
        }
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The menu's state machine (the one test the brief asks for): open, move with wrap-around, into a
    /// sub-menu and back, choose, toggle closed, time out, and the reserved session entry.
    /// "Reopen where I left off" on the fake session tree: the menu opens at the sub-menu path and entry it
    /// was closed on, found by id though the list changed; a gone entry falls back to its nearest surviving
    /// level with the row clamped; per profile menu; off: the top.
    #[test]
    fn the_menu_reopens_where_it_was_left() {
        use super::super::claude_tree::{tree, DesktopSession, Groups, Scope};
        let id = |n: u32| format!("local_0123abcd-0000-4000-8000-{n:012}");
        let s = |n: u32, parent: Option<u32>, last: u64| DesktopSession {
            id: id(n), title: format!("S{n}"), parent: parent.map(id), first: None, depth: None, cwd: "C:/x/UC".into(),
            origin_cwd: "C:/x/UC".into(), worktree: None, archived: false, last_activity: last, last_turn: None, scope: "a/o".into(),
        };
        let groups = |assign: &[(u32, &str)]| Groups {
            scopes: [("a/o".to_string(), Scope {
                groups: vec![("cg-1".into(), "Unified Control".into()), ("cg-2".into(), "Nocturne".into())],
                assignments: assign.iter().map(|(n, g)| (id(*n), g.to_string())).collect(),
            })].into_iter().collect(),
        };
        let menu_of = |sessions: &[DesktopSession], g: &Groups| {
            json!([{"type": "action", "label": "Dictation", "action": {"kind": "claudeDictation"}},
                   {"type": "submenu", "label": "Claude sessions", "entries": tree(sessions, Some(g), &[], &json!("none"), 1_000_000_000)}])
        };
        // Unified Control: S1 (S2, S3 below it); Nocturne: S4.
        let mut all = vec![s(1, None, 100), s(2, Some(1), 300), s(3, Some(1), 200), s(4, None, 50)];
        let g = groups(&[(1, "cg-1"), (4, "cg-2")]);
        let menu = menu_of(&all, &g);
        let t0 = Instant::now();
        let mut m = MenuState { remember: true, scope: "p1".into(), ..Default::default() };
        let walk = |m: &mut MenuState, menu: &Value| {
            m.step(menu, Input::Toggle, t0);
            m.step(menu, Input::Next, t0); // Claude sessions
            m.step(menu, Input::Choose, t0); // > Claude sessions: Unified Control highlighted
            m.step(menu, Input::Choose, t0); // > Unified Control: S1 highlighted
            m.step(menu, Input::Choose, t0); // > S1: row 1 is "Open: S1"
            m.step(menu, Input::Next, t0); // S2
        };
        walk(&mut m, &menu);
        let title = |m: &mut MenuState, menu: &Value| m.view(&json!({}), menu)["title"].clone();
        assert_eq!(title(&mut m, &menu), json!(["Quick menu", "Claude sessions", "Unified Control", "S1"]));
        assert_eq!(m.highlighted_session(&menu), Some(id(2)));
        m.step(&menu, Input::Toggle, t0);
        // Reopen: same path and session, though a new session is now listed first there.
        all.push(s(5, Some(1), 900));
        let menu = menu_of(&all, &g);
        assert_eq!(m.step(&menu, Input::Toggle, t0), Outcome::Opened);
        assert_eq!(title(&mut m, &menu), json!(["Quick menu", "Claude sessions", "Unified Control", "S1"]));
        assert_eq!(m.highlighted_session(&menu), Some(id(2)), "by id, not by row");
        assert_eq!(m.highlight, 3);
        // Back still walks up; Close is row 0 at the top.
        m.step(&menu, Input::Back, t0);
        m.step(&menu, Input::Back, t0);
        m.step(&menu, Input::Back, t0);
        assert_eq!(title(&mut m, &menu), json!(["Quick menu"]));
        assert_eq!(m.view(&json!({}), &menu)["rows"][0]["type"], "close");
        assert_eq!(m.highlight, 2, "on Claude sessions");
        // Dictating into a session closes the menu; the next open returns to it.
        m.step(&menu, Input::Choose, t0);
        m.step(&menu, Input::Choose, t0);
        m.step(&menu, Input::Choose, t0);
        m.step(&menu, Input::Next, t0);
        m.step(&menu, Input::Next, t0); // S2 (after Open: S1 and S5)
        assert_eq!(m.step(&menu, Input::Dictate("claudeDictation".into()), t0), Outcome::Session(id(2), "claudeDictation".into()));
        m.step(&menu, Input::Toggle, t0);
        assert_eq!(m.highlighted_session(&menu), Some(id(2)));
        m.step(&menu, Input::Toggle, t0);
        // S2 is gone: inside S1, the same row clamped to the list.
        all.retain(|x| x.id != id(2));
        let menu = menu_of(&all, &g);
        m.step(&menu, Input::Toggle, t0);
        assert_eq!(title(&mut m, &menu), json!(["Quick menu", "Claude sessions", "Unified Control", "S1"]));
        assert_eq!(m.highlight, 3, "rows: Back, Open: S1, S5, S3: row 3 is S3");
        m.step(&menu, Input::Toggle, t0);
        // S1 lost its sub-sessions: it is a single session now, highlighted in its group.
        all.retain(|x| x.parent.is_none());
        let menu = menu_of(&all, &g);
        m.step(&menu, Input::Toggle, t0);
        assert_eq!(title(&mut m, &menu), json!(["Quick menu", "Claude sessions", "Unified Control"]));
        assert_eq!(m.highlighted_session(&menu), Some(id(1)));
        m.step(&menu, Input::Choose, t0); // opens S1: closes, remembered
        // The group is gone (S1 moved to Nocturne): the nearest surviving level, its row clamped.
        let g2 = groups(&[(1, "cg-2"), (4, "cg-2")]);
        let menu = menu_of(&all, &g2);
        m.step(&menu, Input::Toggle, t0);
        assert_eq!(title(&mut m, &menu), json!(["Quick menu", "Claude sessions"]));
        assert_eq!(m.highlight, 1, "Unified Control was row 1; Nocturne is there now");
        // Another profile menu has its own memory (none yet: the top).
        m.step(&menu, Input::Toggle, t0);
        m.scope = "p2".into();
        m.step(&menu, Input::Toggle, t0);
        assert_eq!((title(&mut m, &menu), m.highlight), (json!(["Quick menu"]), 1));
        m.step(&menu, Input::Next, t0);
        m.step(&menu, Input::Toggle, t0);
        m.scope = "p1".into();
        m.step(&menu, Input::Toggle, t0);
        assert_eq!(title(&mut m, &menu), json!(["Quick menu", "Claude sessions"]), "profile 1's spot is kept");
        m.step(&menu, Input::Toggle, t0);
        m.scope = "p2".into();
        m.step(&menu, Input::Toggle, t0);
        assert_eq!((title(&mut m, &menu), m.highlight), (json!(["Quick menu"]), 2), "profile 2's spot");
        m.step(&menu, Input::Toggle, t0);
        // Setting off: the top, first entry.
        m.remember = false;
        m.scope = "p1".into();
        m.step(&menu, Input::Toggle, t0);
        assert_eq!((title(&mut m, &menu), m.highlight), (json!(["Quick menu"]), 1));
        // Closed on Close at the top: nothing to come back to.
        m.remember = true;
        m.step(&menu, Input::Prev, t0);
        m.step(&menu, Input::Choose, t0);
        m.step(&menu, Input::Toggle, t0);
        assert_eq!(m.highlight, 1);
    }

    #[test]
    fn the_quick_menu_opens_moves_chooses_and_closes() {
        let profiles = vec!["claude".to_string(), "chatgpt".to_string(), "media".to_string()];
        let menu = sanitize_menu(&default_menu(), &profiles);
        assert_eq!(menu.as_array().unwrap().len(), 8);
        let t0 = Instant::now();
        let mut m = MenuState::default();
        assert_eq!(m.step(&menu, Input::Next, t0), Outcome::Nothing, "closed: knob turns do nothing");
        assert_eq!(m.step(&menu, Input::Toggle, t0), Outcome::Opened);
        assert_eq!(m.highlight, 1, "the first entry, after Close");
        let v = m.view(&json!({}), &menu);
        assert_eq!((v["rows"][0]["type"].as_str(), v["rows"][0]["label"].as_str()), (Some("close"), Some("Close")), "Close is always first");
        assert_eq!(m.step(&menu, Input::Prev, t0), Outcome::Moved);
        assert_eq!(m.highlight, 0);
        assert_eq!(m.step(&menu, Input::Prev, t0), Outcome::Moved);
        assert_eq!(m.highlight, 0, "no wrap-around: stays on the first row");
        // Choosing Close closes.
        assert_eq!(m.step(&menu, Input::Choose, t0), Outcome::Closed);
        m.step(&menu, Input::Toggle, t0);
        // Into "Switch profile": row 0 is Back, the first profile is highlighted.
        m.highlight = 5;
        assert_eq!(m.step(&menu, Input::Choose, t0), Outcome::Moved);
        assert_eq!(m.highlight, 1);
        let settings = json!({"menu": menu, "profile": "claude", "target": "claude",
                              "profiles": [{"id": "claude", "name": "Claude"}]});
        let v = m.view(&settings, &menu);
        assert_eq!(v["title"], json!(["Quick menu", "Switch profile"]));
        assert_eq!(v["rows"][0]["type"], "back");
        assert_eq!(v["rows"][1]["current"], true, "the current profile is marked");
        assert_eq!(v["profile"], "Claude");
        // Back returns to the sub-menu's row; choosing again and picking ChatGPT switches the profile.
        assert_eq!(m.step(&menu, Input::Pick(0), t0), Outcome::Moved);
        assert_eq!(m.highlight, 5);
        m.step(&menu, Input::Choose, t0);
        assert_eq!(m.step(&menu, Input::Pick(2), t0), Outcome::Profile("chatgpt".into(), "ChatGPT".into()));
        assert!(!m.open, "a choice closes the menu");
        // An action runs after closing; the menu key closes an open menu.
        m.step(&menu, Input::Toggle, t0);
        match m.step(&menu, Input::Choose, t0) {
            Outcome::Run(a, label) => {
                assert_eq!(a["kind"], "claudeDictation");
                assert_eq!(label, "Claude dictation");
            }
            o => panic!("{o:?}"),
        }
        m.step(&menu, Input::Toggle, t0);
        assert_eq!(m.step(&menu, Input::Toggle, t0), Outcome::Closed);
        // About 8 s without input closes it; any input restarts the time.
        m.step(&menu, Input::Toggle, t0);
        m.step(&menu, Input::Next, t0 + Duration::from_secs(5));
        assert!(!m.expired(t0 + Duration::from_secs(12)));
        assert!(m.expired(t0 + Duration::from_secs(13)));
        // "Claude sessions" lists the sessions by project; choosing one opens it, then runs the follow-up.
        let id = "local_0123abcd-0000-4000-8000-00000000abcd";
        let sessions = vec![
            json!({"desktopId": id, "label": "Fix the cooler", "project": "UnifiedControl", "state": "working", "last": 1}),
            json!({"desktopId": "local_0123abcd-0000-4000-8000-00000000abce", "label": "", "project": "vault", "last": 1}),
        ];
        let lst = json!([{"type": "claudeSessions", "label": "Claude sessions", "then": "claudeDictation"}]);
        let shown = expand(&lst, &sessions, true, 1, None);
        assert_eq!(shown[0]["entries"][0]["label"], "UnifiedControl", "grouped by project");
        let mut s = MenuState::default();
        s.step(&shown, Input::Toggle, t0);
        s.step(&shown, Input::Choose, t0); // into Claude sessions
        s.step(&shown, Input::Choose, t0); // into UnifiedControl
        assert_eq!(s.step(&shown, Input::Choose, t0), Outcome::Session(id.into(), "claudeDictation".into()));
        // Not connected: a note that stays open; Back at the top closes.
        let none = expand(&lst, &[], false, 1, None);
        let mut s = MenuState::default();
        s.step(&none, Input::Toggle, t0);
        s.step(&none, Input::Choose, t0);
        assert!(matches!(s.step(&none, Input::Choose, t0), Outcome::Unavailable(m) if m.starts_with("Connect")));
        assert!(s.open);
        s.step(&none, Input::Back, t0);
        assert_eq!(s.step(&none, Input::Back, t0), Outcome::Closed);
        // Sanitising drops what the menu cannot run.
        let bad = json!([{"type": "action", "action": {"kind": "quickMenu"}}, {"type": "action", "action": {"kind": "shell"}},
                         {"type": "profile", "profile": "gone"}, {"type": "target", "target": "notepad"},
                         {"type": "submenu", "entries": [{"type": "submenu", "entries": [{"type": "submenu", "entries": []}]}]}]);
        let clean = sanitize_menu(&bad, &profiles);
        assert_eq!(clean.as_array().unwrap().len(), 1, "only the sub-menu is kept");
        assert!(clean[0]["entries"][0]["entries"].as_array().unwrap().is_empty(), "at most three levels");
        // Menu controls: defaults K2 select, K3 back, knob press select; invalid values fall back.
        let c = sanitize_controls(&json!({"k2": "close", "k3": "explode"}));
        assert_eq!(c, json!({"k2": "close", "k3": "back", "knobPress": "select"}));
        assert_eq!(role(&json!({}), "k1"), "none");
        assert_eq!(hints(&sanitize_controls(&json!({}))), "Turn: move · K2 select · K3 back · Knob press select");
    }

    /// Session tree: projects by recent activity; in a project the main checkout's session first, then the
    /// worktrees; a project carries its main session; a working dot. No wrap-around; K1 on a session or a
    /// project dictates into it, elsewhere it does nothing here.
    #[test]
    fn sessions_form_a_project_tree_and_k1_dictates_into_the_highlighted_one() {
        let id = |n: u8| format!("local_0123abcd-0000-4000-8000-0000000000{n:02}");
        let s = |n: u8, root: &str, wt: Option<&str>, title: &str, working: bool| {
            json!({"desktopId": id(n), "root": format!("C:\\{root}"), "project": root, "worktree": wt, "title": title,
                   "state": if working { "working" } else { "idle" }, "last": 1})
        };
        // Most recent first (as the session list gives them).
        let sessions = vec![
            s(1, "UnifiedControl", Some("wt-a"), "Vibe key fixes", true),
            s(2, "vault", None, "", false),
            s(3, "UnifiedControl", None, "Orchestrator", false),
            s(4, "UnifiedControl", Some("wt-b"), "", false),
        ];
        let tree = session_tree(&sessions, &json!("none"), 1);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0]["label"], "UnifiedControl", "the most recently active project first");
        assert_eq!(tree[0]["session"], id(3), "a project carries its main-checkout session");
        assert_eq!(tree[0]["working"], true);
        let labels: Vec<&str> = tree[0]["entries"].as_array().unwrap().iter().map(|e| e["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["Orchestrator", "Vibe key fixes", "wt-b"], "main first, then worktrees; title, else the worktree folder");
        assert!(tree[0]["entries"][1]["detail"].as_str().unwrap().starts_with("● working"));
        assert_eq!(tree[1]["entries"][0]["label"], "vault", "no title or prompt: the project folder");

        let menu = json!([{"type": "submenu", "label": "Claude sessions", "entries": tree}, {"type": "action", "label": "X", "action": {"kind": "playPause"}}]);
        let t0 = Instant::now();
        let mut m = MenuState::default();
        m.step(&menu, Input::Toggle, t0);
        // No wrap-around.
        m.step(&menu, Input::Next, t0);
        m.step(&menu, Input::Next, t0);
        assert_eq!(m.highlight, 2, "stays on the last row");
        m.step(&menu, Input::Prev, t0);
        m.step(&menu, Input::Prev, t0);
        m.step(&menu, Input::Prev, t0);
        assert_eq!(m.highlight, 0, "stays on the first row");
        assert_eq!(m.step(&menu, Input::Dictate("claudeDictation".into()), t0), Outcome::Nothing, "Close is not a session");
        // Into Claude sessions; K1 on the project dictates into its main session.
        m.highlight = 1;
        m.step(&menu, Input::Choose, t0);
        assert_eq!(m.highlighted_session(&menu), Some(id(3)));
        // Into the project; K2 (Choose) on a session opens it; K1 dictates into it.
        m.step(&menu, Input::Choose, t0);
        m.step(&menu, Input::Next, t0);
        assert_eq!(m.step(&menu, Input::Dictate("claudeDictateSend".into()), t0), Outcome::Session(id(1), "claudeDictateSend".into()));
        assert!(!m.open, "the menu closes");
        let v = { let mut m = MenuState::default(); m.step(&menu, Input::Toggle, t0); m.step(&menu, Input::Choose, t0); m.view(&json!({}), &menu) };
        assert_eq!(v["rows"][1]["working"], true, "the window gets the working dot");
    }
}
