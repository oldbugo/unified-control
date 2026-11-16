//! Claude session targeting through Claude Code hooks (orchestrator design; the user chose hooks, 2026-11-17).
//!
//! - Opt-in: the Vibe Key page's "Connect Claude sessions" adds UC's own hook entries to the user's Claude
//!   settings (`%USERPROFILE%\.claude\settings.json`): merged, never overwritten, backed up first, and shown
//!   before they are added. "Disconnect" removes exactly UC's entries. No admin rights.
//! - Each hook runs `UnifiedControl.exe --claude-hook <event>` directly (exec form: no shell, no quoting). The
//!   command sends only {event, desktop session id, Claude Code session id, folder, time, and for
//!   UserPromptSubmit the first 80 characters of the prompt as a label} to the running UC, prints nothing and
//!   always exits 0 at once, so Claude is never slowed or blocked. Nothing else from the hook's environment
//!   is read (it holds tokens and the user's email).
//! - UC keeps the session list in `claude-sessions.json` in its app folder; a session leaves the list at
//!   SessionEnd or after 24 h without activity. The quick menu shows it as "Claude sessions", grouped by
//!   project; choosing one opens `claude://claude.ai/epitaxy/<desktop id>`.
//! - Tests never touch the real Claude settings: fake and quiet instances need `UC_CLAUDE_HOME` (a temp folder).
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const EVENTS: [&str; 4] = ["SessionStart", "UserPromptSubmit", "Stop", "SessionEnd"];
pub const HOOK_FLAG: &str = "--claude-hook";
pub const IDLE_EXPIRY_MS: u64 = 24 * 3600 * 1000;
pub const MAX_SESSIONS: usize = 200;
pub const LABEL_MAX: usize = 80;
/// Seconds Claude Code waits for a hook (SessionEnd hooks share a short budget unless one is set).
pub const HOOK_TIMEOUT_S: u64 = 5;

fn hex_uuid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() })
}
/// A desktop session id: `local_<uuid>`.
pub fn valid_desktop_id(id: &str) -> bool {
    id.strip_prefix("local_").is_some_and(hex_uuid)
}
pub fn session_link(desktop_id: &str) -> Option<String> {
    valid_desktop_id(desktop_id).then(|| format!("claude://claude.ai/epitaxy/{desktop_id}"))
}

fn clean(s: &str, max: usize) -> String {
    let t: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).take(max).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What `--claude-hook` sends: only these fields. `None` = send nothing (unknown event, or not a desktop
/// session: without a desktop id the session cannot be opened).
pub fn hook_payload(event: &str, stdin: &Value, desktop_id: Option<&str>, code_id: Option<&str>, now_ms: u64) -> Option<Value> {
    if !EVENTS.contains(&event) {
        return None;
    }
    let desktop = desktop_id.filter(|d| valid_desktop_id(d))?;
    let code = code_id.or(stdin["session_id"].as_str()).filter(|c| hex_uuid(c)).unwrap_or("");
    let cwd = stdin["cwd"].as_str().filter(|c| c.len() <= 1024 && !c.contains('\0')).unwrap_or("");
    let mut v = json!({"event": event, "desktopId": desktop, "codeSessionId": code, "cwd": cwd, "at": now_ms});
    if event == "UserPromptSubmit" {
        let label = clean(stdin["prompt"].as_str().unwrap_or(""), LABEL_MAX);
        if !label.is_empty() {
            v["label"] = json!(label);
        }
    }
    Some(v)
}

/// The project a folder belongs to: its last path component ("UnifiedControl").
pub fn project_name(cwd: &str) -> String {
    cwd.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().filter(|s| !s.is_empty()).unwrap_or("Other").to_string()
}

/// The session list (in memory and `claude-sessions.json`).
pub struct Sessions {
    path: PathBuf,
    list: Mutex<Vec<Value>>,
    last_event: Mutex<Option<u64>>,
}

impl Sessions {
    pub fn load(path: PathBuf) -> Self {
        let list = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
            .and_then(|v| v["sessions"].as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s["desktopId"].as_str().is_some_and(valid_desktop_id))
            .take(MAX_SESSIONS)
            .collect();
        Self { path, list: Mutex::new(list), last_event: Mutex::new(None) }
    }

    fn save(&self, list: &[Value]) {
        let tmp = self.path.with_extension("json.tmp");
        if let Ok(t) = serde_json::to_string_pretty(&json!({"sessions": list})) {
            if std::fs::write(&tmp, t).is_ok() && std::fs::rename(&tmp, &self.path).is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }

    /// One hook event (checked again: the pipe is same-user only, but the payload is still untrusted).
    pub fn record(&self, v: &Value, now_ms: u64) -> Result<(), String> {
        let event = v["event"].as_str().filter(|e| EVENTS.contains(e)).ok_or("Unknown Claude hook event")?;
        let id = v["desktopId"].as_str().filter(|d| valid_desktop_id(d)).ok_or("Not a desktop session")?;
        let cwd = v["cwd"].as_str().filter(|c| c.len() <= 1024).unwrap_or("");
        let code = v["codeSessionId"].as_str().filter(|c| hex_uuid(c)).unwrap_or("");
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        *self.last_event.lock().unwrap_or_else(|e| e.into_inner()) = Some(now_ms);
        list.retain(|s| now_ms.saturating_sub(s["last"].as_u64().unwrap_or(0)) < IDLE_EXPIRY_MS);
        if event == "SessionEnd" {
            list.retain(|s| s["desktopId"] != id);
        } else {
            let pos = match list.iter().position(|s| s["desktopId"] == id) {
                Some(p) => p,
                None => {
                    list.push(json!({"desktopId": id, "label": "", "started": now_ms}));
                    list.len() - 1
                }
            };
            let s = &mut list[pos];
            s["codeSessionId"] = json!(code);
            if !cwd.is_empty() {
                s["cwd"] = json!(cwd);
                s["project"] = json!(project_name(cwd));
            }
            s["last"] = json!(now_ms);
            s["state"] = json!(if event == "UserPromptSubmit" { "working" } else { "idle" });
            // The label is the first prompt only.
            if s["label"].as_str().unwrap_or("").is_empty() {
                if let Some(l) = v["label"].as_str() {
                    s["label"] = json!(clean(l, LABEL_MAX));
                }
            }
            if list.len() > MAX_SESSIONS {
                list.sort_by_key(|s| std::cmp::Reverse(s["last"].as_u64().unwrap_or(0)));
                list.truncate(MAX_SESSIONS);
            }
        }
        let snapshot = list.clone();
        drop(list);
        self.save(&snapshot);
        Ok(())
    }

    /// Current sessions, most recent first (expired ones left out).
    pub fn list(&self, now_ms: u64) -> Vec<Value> {
        let mut list: Vec<Value> = self
            .list
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|s| now_ms.saturating_sub(s["last"].as_u64().unwrap_or(0)) < IDLE_EXPIRY_MS)
            .cloned()
            .collect();
        list.sort_by_key(|s| std::cmp::Reverse(s["last"].as_u64().unwrap_or(0)));
        list
    }
    pub fn last_event(&self) -> Option<u64> {
        *self.last_event.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn clear(&self) {
        self.list.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.save(&[]);
    }
}

// ---------- UC's entries in Claude's settings ----------

/// One hook group: `{"hooks": [{"type": "command", "command": <exe>, "args": ["--claude-hook", <event>], "timeout": 5}]}`.
pub fn our_group(exe: &str, event: &str) -> Value {
    json!({"hooks": [{"type": "command", "command": exe, "args": [HOOK_FLAG, event], "timeout": HOOK_TIMEOUT_S}]})
}

/// Exactly what Connect adds (for the page's preview).
pub fn plan(exe: &str) -> Value {
    let mut hooks = serde_json::Map::new();
    for e in EVENTS {
        hooks.insert(e.to_string(), json!([our_group(exe, e)]));
    }
    json!({"hooks": hooks})
}

/// UC's own hook: its program is a Unified Control exe and its first argument is `--claude-hook`.
fn is_ours(hook: &Value) -> bool {
    let exe = hook["command"].as_str().unwrap_or("").to_ascii_lowercase();
    let name = exe.rsplit(['\\', '/']).next().unwrap_or("");
    (name == "unifiedcontrol.exe" || name == "unified-control.exe") && hook["args"][0] == HOOK_FLAG
}

/// Remove UC's hooks (and only them). Returns how many were removed.
pub fn disconnect(settings: &mut Value) -> usize {
    let mut removed = 0;
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else { return 0 };
    for event in EVENTS {
        let Some(groups) = hooks.get_mut(event).and_then(Value::as_array_mut) else { continue };
        let before = removed;
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                let n = list.len();
                list.retain(|h| !is_ours(h));
                removed += n - list.len();
            }
        }
        if removed > before {
            groups.retain(|g| g["hooks"].as_array().is_none_or(|l| !l.is_empty()));
            if groups.is_empty() {
                hooks.remove(event);
            }
        }
    }
    if removed > 0 && hooks.is_empty() {
        if let Some(o) = settings.as_object_mut() {
            o.remove("hooks");
        }
    }
    removed
}

/// Add UC's hooks for this exe (replacing older UC entries; everything else is kept as it is).
pub fn connect(settings: &mut Value, exe: &str) -> Result<(), String> {
    if !settings.is_object() {
        return Err("Your Claude settings file is not a settings object; Unified Control changed nothing".into());
    }
    if settings.get("hooks").is_some_and(|h| !h.is_object()) {
        return Err("The hooks in your Claude settings have an unexpected shape; Unified Control changed nothing".into());
    }
    for e in EVENTS {
        if settings["hooks"].get(e).is_some_and(|v| !v.is_array()) {
            return Err(format!("The {e} hooks in your Claude settings have an unexpected shape; Unified Control changed nothing"));
        }
    }
    disconnect(settings);
    let hooks = settings.as_object_mut().unwrap().entry("hooks").or_insert_with(|| json!({}));
    for e in EVENTS {
        let list = hooks.as_object_mut().unwrap().entry(e).or_insert_with(|| json!([]));
        list.as_array_mut().unwrap().push(our_group(exe, e));
    }
    Ok(())
}

/// "connected" (all four, this exe), "outdated" (UC entries for another exe path, or only some), or "off".
pub fn status(settings: &Value, exe: &str) -> &'static str {
    let mut ours = 0;
    let mut current = 0;
    for e in EVENTS {
        let found: Vec<&Value> = settings["hooks"][e]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
            .filter(|h| is_ours(h))
            .collect();
        if !found.is_empty() {
            ours += 1;
        }
        if found.iter().any(|h| h["command"].as_str().is_some_and(|c| c.eq_ignore_ascii_case(exe)) && h["args"][1] == e) {
            current += 1;
        }
    }
    match (ours, current) {
        (0, _) => "off",
        (_, 4) => "connected",
        _ => "outdated",
    }
}

/// Why Claude would not run UC's hooks, if it would not: managed (organisation) settings first, then the
/// user's own `disableAllHooks`.
pub fn hooks_blocked(user: &Value, managed: &[Value]) -> Option<String> {
    for m in managed {
        if m["allowManagedHooksOnly"] == true {
            return Some("Your organisation's Claude settings allow only managed hooks (allowManagedHooksOnly), so Claude would not run Unified Control's hooks.".into());
        }
        if m["disableAllHooks"] == true {
            return Some("Your organisation's Claude settings turn off all hooks (disableAllHooks).".into());
        }
        let strict = &m["strictPluginOnlyCustomization"];
        if *strict == true || strict.as_array().is_some_and(|a| a.iter().any(|x| x == "hooks")) {
            return Some("Your organisation's Claude settings allow hooks only from plugins (strictPluginOnlyCustomization).".into());
        }
    }
    if user["disableAllHooks"] == true {
        return Some("Hooks are turned off in your own Claude settings (disableAllHooks is true).".into());
    }
    None
}

/// Read Claude's settings file. A missing file is an empty object; an unreadable one is an error (UC then
/// changes nothing).
pub fn read_settings(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(_) => Err("Your Claude settings file could not be read; Unified Control changed nothing".into()),
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}'))
            .ok()
            .filter(Value::is_object)
            .ok_or_else(|| "Your Claude settings file is not valid JSON; Unified Control changed nothing".into()),
    }
}

/// Back up the current file (if any) next to it, then write the new one (temp file and rename).
/// Returns the backup's path.
pub fn write_settings(path: &Path, settings: &Value, now_ms: u64) -> Result<Option<PathBuf>, String> {
    let dir = path.parent().ok_or("No Claude settings folder")?;
    std::fs::create_dir_all(dir).map_err(|_| "The Claude settings folder could not be created")?;
    let backup = if path.exists() {
        let b = dir.join(format!("settings.json.uc-backup-{now_ms}"));
        std::fs::copy(path, &b).map_err(|_| "The backup of your Claude settings could not be made; nothing was changed")?;
        Some(b)
    } else {
        None
    };
    let text = serde_json::to_string_pretty(settings).map_err(|_| "The Claude settings could not be written")?;
    let tmp = dir.join("settings.json.uc-tmp");
    std::fs::write(&tmp, text + "\n").map_err(|_| "The Claude settings could not be written")?;
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err("The Claude settings could not be replaced; the old file is unchanged".into());
    }
    Ok(backup)
}

/// The organisation's Claude settings on Windows: `C:\Program Files\ClaudeCode\managed-settings.json`, its
/// `managed-settings.d` folder, and the `Settings` policy value under HKLM and HKCU
/// `SOFTWARE\Policies\ClaudeCode`.
pub fn managed_settings() -> Vec<Value> {
    let mut out = Vec::new();
    let dir = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Program Files")).join("ClaudeCode");
    let parse = |t: String| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok();
    if let Some(v) = std::fs::read_to_string(dir.join("managed-settings.json")).ok().and_then(parse) {
        out.push(v);
    }
    if let Ok(rd) = std::fs::read_dir(dir.join("managed-settings.d")) {
        for e in rd.flatten().take(64) {
            if e.path().extension().is_some_and(|x| x == "json") {
                if let Some(v) = std::fs::read_to_string(e.path()).ok().and_then(parse) {
                    out.push(v);
                }
            }
        }
    }
    #[cfg(windows)]
    for root in [windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE, windows_sys::Win32::System::Registry::HKEY_CURRENT_USER] {
        if let Some(v) = policy_value(root).and_then(parse) {
            out.push(v);
        }
    }
    out
}

#[cfg(windows)]
fn policy_value(root: windows_sys::Win32::System::Registry::HKEY) -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (key, name) = (wide(r"SOFTWARE\Policies\ClaudeCode"), wide("Settings"));
    let mut size: u32 = 0;
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
    let r = unsafe { RegGetValueW(root, key.as_ptr(), name.as_ptr(), flags, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) };
    if r != 0 || size == 0 || size > 1 << 20 {
        return None;
    }
    let mut buf = vec![0u16; size as usize / 2 + 1];
    let r = unsafe {
        RegGetValueW(root, key.as_ptr(), name.as_ptr(), flags, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    if r != 0 {
        return None;
    }
    let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..n]))
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "local_0123abcd-0000-4000-8000-00000000abcd";
    const CODE: &str = "11111111-2222-4333-8444-555555555555";

    #[test]
    fn hooks_merge_and_remove_only_ucs_entries_and_sessions_follow_events() {
        let exe = r"C:\Users\x\AppData\Local\Unified Control\UnifiedControl.exe";
        let mut s = json!({"model": "opus", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify.exe"}]}]}});
        connect(&mut s, exe).unwrap();
        assert_eq!(status(&s, exe), "connected");
        assert_eq!(s["model"], "opus", "other settings are kept");
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 2, "the user's own Stop hook is kept");
        connect(&mut s, exe).unwrap();
        assert_eq!(s["hooks"]["SessionStart"].as_array().unwrap().len(), 1, "connecting twice adds nothing");
        assert_eq!(status(&s, r"C:\Other\UnifiedControl.exe"), "outdated");
        assert_eq!(disconnect(&mut s), 4);
        assert_eq!(s, json!({"model": "opus", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify.exe"}]}]}}));
        assert_eq!(status(&s, exe), "off");
        assert!(connect(&mut json!({"hooks": []}), exe).is_err(), "an odd shape changes nothing");
        assert!(hooks_blocked(&json!({}), &[json!({"allowManagedHooksOnly": true})]).is_some());
        assert!(hooks_blocked(&json!({"disableAllHooks": true}), &[]).is_some());
        assert!(hooks_blocked(&json!({}), &[json!({"strictPluginOnlyCustomization": ["hooks"]})]).is_some());
        assert!(hooks_blocked(&json!({}), &[]).is_none());

        // The hook sends only its fields; a prompt becomes an 80-character label; no desktop id, nothing sent.
        let stdin = json!({"session_id": CODE, "cwd": r"C:\Users\x\Documents\UnifiedControl", "prompt": "Fix\nthe cooler ".repeat(20), "transcript_path": "secret"});
        let p = hook_payload("UserPromptSubmit", &stdin, Some(ID), None, 5).unwrap();
        assert_eq!(p.as_object().unwrap().len(), 6);
        assert!(p.get("transcript_path").is_none());
        assert!(p["label"].as_str().unwrap().chars().count() <= LABEL_MAX && !p["label"].as_str().unwrap().contains('\n'));
        assert_eq!(hook_payload("UserPromptSubmit", &stdin, None, None, 5), None);
        assert_eq!(hook_payload("PreToolUse", &stdin, Some(ID), None, 5), None);

        let dir = std::env::temp_dir().join(format!("uc-claude-sessions-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = Sessions::load(dir.join("claude-sessions.json"));
        list.record(&hook_payload("SessionStart", &stdin, Some(ID), None, 1).unwrap(), 1).unwrap();
        list.record(&p, 2).unwrap();
        let now = list.list(3);
        assert_eq!(now[0]["project"], "UnifiedControl");
        assert_eq!(now[0]["state"], "working");
        assert!(now[0]["label"].as_str().unwrap().starts_with("Fix the cooler"));
        assert_eq!(Sessions::load(dir.join("claude-sessions.json")).list(3).len(), 1, "saved in the app folder");
        assert!(list.list(2 + IDLE_EXPIRY_MS).is_empty(), "24 h without activity");
        list.record(&hook_payload("SessionEnd", &stdin, Some(ID), None, 4).unwrap(), 4).unwrap();
        assert!(list.list(5).is_empty(), "SessionEnd removes it");
        assert_eq!(session_link(ID).unwrap(), format!("claude://claude.ai/epitaxy/{ID}"));
        assert!(session_link("local_../../x").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
