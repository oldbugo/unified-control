//! Claude session targeting through Claude Code hooks (orchestrator design; the user chose hooks, 2026-11-17).
//!
//! - Opt-in: the Vibe Key page's "Connect Claude sessions" adds UC's own hook entries to the user's Claude
//!   settings (`%USERPROFILE%\.claude\settings.json`): merged, never overwritten, backed up first, shown before
//!   they are added. "Disconnect" removes exactly UC's entries. No admin rights.
//! - Each hook runs the request-only copy `UC-ClaudeHook-v1.exe` (made by the installer next to
//!   UnifiedControl.exe) directly, with `--claude-hook <event> --uc-hook-v1` (exec form: no shell, no quoting).
//!   A program with that name only ever runs the hook, whatever its arguments, so a hook can never start or
//!   raise the app (an older or virtualized UnifiedControl.exe could). The hook sends only {event, desktop
//!   session id, Claude Code session id, folder, time, and for UserPromptSubmit the first 80 characters of the
//!   prompt as a label} to the running UC, prints nothing and always exits 0 at once. Nothing else from the
//!   hook's environment is read (it holds tokens and the user's email).
//! - The uninstaller runs `UnifiedControl.exe --claude-disconnect` first, so no hook outlives the app.
//! - UC keeps the session list in `claude-sessions.json` in its app folder. A session is bound to the Claude
//!   Code session that first reported it (child sessions can share the desktop id); it leaves the list at
//!   that session's SessionEnd or after 24 h without activity.
//! - Claude's settings file is changed only through [`update_file`]: nothing is written when nothing changes;
//!   a backup comes first (UC keeps its last three); a change made meanwhile by someone else aborts the write;
//!   a symlinked file is written at its target.
//! - Tests never touch the real Claude settings: debug builds, fake and quiet instances use only
//!   `UC_CLAUDE_HOME` (a temp folder).
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const EVENTS: [&str; 4] = ["SessionStart", "UserPromptSubmit", "Stop", "SessionEnd"];
pub const HOOK_FLAG: &str = "--claude-hook";
/// Marks UC's own hook entries (with the proxy's exact name).
pub const HOOK_MARKER: &str = "--uc-hook-v1";
/// The request-only copy the hooks run. Made by the installer (`installer-hooks.nsh`).
pub const PROXY_NAME: &str = "UC-ClaudeHook-v1.exe";
pub const DISCONNECT_FLAG: &str = "--claude-disconnect";
pub const IDLE_EXPIRY_MS: u64 = 24 * 3600 * 1000;
pub const MAX_SESSIONS: usize = 200;
pub const LABEL_MAX: usize = 80;
pub const PROJECT_MAX: usize = 60;
pub const BACKUPS_KEPT: usize = 3;
const BACKUP_PREFIX: &str = "settings.json.uc-backup-";
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

/// Is this program the hook proxy (by its file name)? Then it only ever runs the hook.
pub fn is_proxy(exe: &Path) -> bool {
    exe.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(PROXY_NAME))
}

/// What `--claude-hook` sends: only these fields. `None` = send nothing (unknown event, or not a desktop
/// session: without a desktop id the session cannot be opened).
pub fn hook_payload(event: &str, stdin: &Value, desktop_id: Option<&str>, code_id: Option<&str>, now_ms: u64) -> Option<Value> {
    if !EVENTS.contains(&event) {
        return None;
    }
    let desktop = desktop_id.filter(|d| valid_desktop_id(d))?;
    let code = code_id.or(stdin["session_id"].as_str()).filter(|c| hex_uuid(c)).unwrap_or("");
    let cwd: String = stdin["cwd"].as_str().filter(|c| c.len() <= 1024).unwrap_or("").chars().filter(|c| !c.is_control()).collect();
    let mut v = json!({"event": event, "desktopId": desktop, "codeSessionId": code, "cwd": cwd, "at": now_ms});
    if event == "UserPromptSubmit" {
        let label = clean(stdin["prompt"].as_str().unwrap_or(""), LABEL_MAX);
        if !label.is_empty() {
            v["label"] = json!(label);
        }
    }
    Some(v)
}

/// The project a folder belongs to: its last path component ("UnifiedControl"), at most 60 characters.
pub fn project_name(cwd: &str) -> String {
    let name = cwd.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().map(|s| clean(s, PROJECT_MAX)).unwrap_or_default();
    if name.is_empty() { "Other".into() } else { name }
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
    /// A session is bound to the Claude Code session id that first reported it; events from another code id
    /// (a child session sharing the desktop id) change nothing.
    pub fn record(&self, v: &Value, now_ms: u64) -> Result<(), String> {
        let event = v["event"].as_str().filter(|e| EVENTS.contains(e)).ok_or("Unknown Claude hook event")?;
        let id = v["desktopId"].as_str().filter(|d| valid_desktop_id(d)).ok_or("Not a desktop session")?;
        let cwd: String = v["cwd"].as_str().filter(|c| c.len() <= 1024).unwrap_or("").chars().filter(|c| !c.is_control()).collect();
        let code = v["codeSessionId"].as_str().filter(|c| hex_uuid(c)).unwrap_or("");
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        *self.last_event.lock().unwrap_or_else(|e| e.into_inner()) = Some(now_ms);
        list.retain(|s| now_ms.saturating_sub(s["last"].as_u64().unwrap_or(0)) < IDLE_EXPIRY_MS);
        let pos = list.iter().position(|s| s["desktopId"] == id);
        let bound = pos.and_then(|p| list[p]["codeSessionId"].as_str().map(str::to_string)).unwrap_or_default();
        let same = bound.is_empty() || code.is_empty() || bound == code;
        if !same {
            return Ok(()); // another Claude Code session under the same desktop session
        }
        if event == "SessionEnd" {
            if pos.is_some() {
                list.retain(|s| s["desktopId"] != id);
            }
        } else {
            let pos = match pos {
                Some(p) => p,
                None => {
                    list.push(json!({"desktopId": id, "label": "", "started": now_ms, "codeSessionId": ""}));
                    list.len() - 1
                }
            };
            let s = &mut list[pos];
            if bound.is_empty() && !code.is_empty() {
                s["codeSessionId"] = json!(code);
            }
            if !cwd.is_empty() {
                s["cwd"] = json!(cwd);
                s["project"] = json!(project_name(&cwd));
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

// ---------- where things are ----------

/// Claude's settings folder. Debug builds and test (fake or quiet) instances use only `UC_CLAUDE_HOME`, never
/// the real folder.
pub fn claude_home(test: bool) -> Result<PathBuf, String> {
    let override_home = std::env::var_os("UC_CLAUDE_HOME").filter(|h| !h.is_empty());
    if test || cfg!(debug_assertions) {
        return override_home
            .map(PathBuf::from)
            .ok_or_else(|| "Test and debug builds never change your Claude settings (UC_CLAUDE_HOME is not set)".into());
    }
    let profile = std::env::var_os("USERPROFILE").ok_or("Your Windows user folder is unknown")?;
    Ok(PathBuf::from(profile).join(".claude"))
}

/// The hook proxy next to this program, if it exists.
pub fn proxy_path() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|_| "The Unified Control program path is unknown")?;
    let p = exe.with_file_name(PROXY_NAME);
    if !p.is_file() {
        return Err(format!("{PROXY_NAME} is missing next to Unified Control. Reinstall Unified Control to connect Claude sessions"));
    }
    let s = p.to_string_lossy().to_string();
    Ok(s.strip_prefix(r"\\?\").map(String::from).unwrap_or(s))
}

// ---------- UC's entries in Claude's settings ----------

/// One hook group: `{"hooks": [{"type": "command", "command": <proxy>, "args": ["--claude-hook", <event>,
/// "--uc-hook-v1"], "timeout": 5}]}`.
pub fn our_group(exe: &str, event: &str) -> Value {
    json!({"hooks": [{"type": "command", "command": exe, "args": [HOOK_FLAG, event, HOOK_MARKER], "timeout": HOOK_TIMEOUT_S}]})
}

/// Exactly what Connect adds (for the page's preview).
pub fn plan(exe: &str) -> Value {
    let mut hooks = serde_json::Map::new();
    for e in EVENTS {
        hooks.insert(e.to_string(), json!([our_group(exe, e)]));
    }
    json!({"hooks": hooks})
}

/// UC's own hook: the program is exactly the proxy and the marker argument is there.
fn is_ours(hook: &Value) -> bool {
    let exe = hook["command"].as_str().unwrap_or("");
    let name = exe.rsplit(['\\', '/']).next().unwrap_or("");
    name.eq_ignore_ascii_case(PROXY_NAME) && hook["args"].as_array().is_some_and(|a| a.iter().any(|x| x == HOOK_MARKER))
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

/// Add UC's hooks for this proxy path (replacing older UC entries; everything else is kept as it is).
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
    // Already exactly connected: leave the file alone.
    if status(settings, exe) == "connected" {
        return Ok(());
    }
    disconnect(settings);
    let hooks = settings.as_object_mut().unwrap().entry("hooks").or_insert_with(|| json!({}));
    for e in EVENTS {
        let list = hooks.as_object_mut().unwrap().entry(e).or_insert_with(|| json!([]));
        list.as_array_mut().unwrap().push(our_group(exe, e));
    }
    Ok(())
}

/// "connected" (all four, this proxy, one each), "outdated" (UC entries for another path, or only some), or "off".
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
        ours += found.len();
        if found.len() == 1 && found[0]["command"].as_str().is_some_and(|c| c.eq_ignore_ascii_case(exe)) && found[0]["args"][1] == e {
            current += 1;
        }
    }
    match (ours, current) {
        (0, _) => "off",
        (4, 4) => "connected",
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

fn parse_settings(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Your Claude settings file is not valid text; Unified Control changed nothing")?;
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(text)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| "Your Claude settings file is not valid JSON; Unified Control changed nothing".into())
}

/// Read Claude's settings. A missing or empty file is an empty object; an unreadable or invalid one is an
/// error (UC then changes nothing).
pub fn read_settings(path: &Path) -> Result<Value, String> {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(_) => Err("Your Claude settings file could not be read; Unified Control changed nothing".into()),
        Ok(b) => parse_settings(&b),
    }
}

/// The result of [`update_file`].
#[derive(Debug, PartialEq)]
pub struct Updated {
    pub changed: bool,
    pub backup: Option<PathBuf>,
}

/// Change Claude's settings file safely: read it, apply `change`, and only if the content changed: back it up
/// (keeping UC's last three backups), write a temp file, check that nobody changed the file meanwhile, and
/// replace it. A symlinked file is written at its target. `before_replace` is for tests.
pub fn update_file(
    path: &Path,
    now_ms: u64,
    change: impl FnOnce(&mut Value) -> Result<(), String>,
    before_replace: &dyn Fn(),
) -> Result<Updated, String> {
    // A symlink is followed, so the link itself stays a link.
    let target = match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => std::fs::canonicalize(path).map_err(|_| "Your Claude settings link points nowhere; Unified Control changed nothing")?,
        _ => path.to_path_buf(),
    };
    let original = match std::fs::read(&target) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err("Your Claude settings file could not be read; Unified Control changed nothing".into()),
    };
    let before = match &original {
        Some(b) => parse_settings(b)?,
        None => json!({}),
    };
    let mut after = before.clone();
    change(&mut after)?;
    if after == before {
        return Ok(Updated { changed: false, backup: None });
    }
    let dir = target.parent().ok_or("No Claude settings folder")?;
    std::fs::create_dir_all(dir).map_err(|_| "The Claude settings folder could not be created")?;
    let backup = match &original {
        Some(b) => {
            let p = dir.join(format!("{BACKUP_PREFIX}{now_ms}"));
            std::fs::write(&p, b).map_err(|_| "The backup of your Claude settings could not be made; nothing was changed")?;
            prune_backups(dir);
            Some(p)
        }
        None => None,
    };
    let text = serde_json::to_string_pretty(&after).map_err(|_| "The Claude settings could not be written")? + "\n";
    let tmp = dir.join("settings.json.uc-tmp");
    std::fs::write(&tmp, text).map_err(|_| "The Claude settings could not be written; the old file is unchanged")?;
    before_replace();
    let now = match std::fs::read(&target) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => None,
    };
    if now != original {
        let _ = std::fs::remove_file(&tmp);
        return Err("Your Claude settings changed while Unified Control was updating them; nothing was changed. Try again".into());
    }
    if std::fs::rename(&tmp, &target).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err("The Claude settings could not be replaced (the file may be in use); the old file is unchanged".into());
    }
    Ok(Updated { changed: true, backup })
}

/// Keep only UC's last three backups (by their time stamp).
fn prune_backups(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut found: Vec<(u64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_prefix(BACKUP_PREFIX).and_then(|n| n.parse::<u64>().ok()).map(|n| (n, e.path()))
        })
        .collect();
    found.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    for (_, p) in found.into_iter().skip(BACKUPS_KEPT) {
        let _ = std::fs::remove_file(p);
    }
}

/// `--claude-disconnect` (the uninstaller): remove UC's entries from Claude's settings. Returns how many.
pub fn disconnect_file(path: &Path, now_ms: u64) -> Result<usize, String> {
    let mut removed = 0;
    update_file(path, now_ms, |v| { removed = disconnect(v); Ok(()) }, &|| {})?;
    Ok(removed)
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
    const CHILD: &str = "99999999-2222-4333-8444-555555555555";
    const EXE: &str = r"C:\Users\x\AppData\Local\Unified Control\UC-ClaudeHook-v1.exe";

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("uc-claude-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn hooks_merge_and_remove_only_ucs_entries_and_sessions_follow_events() {
        let mut s = json!({"model": "opus", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify.exe"}]}]}});
        connect(&mut s, EXE).unwrap();
        assert_eq!(status(&s, EXE), "connected");
        assert_eq!(s["hooks"]["SessionStart"][0]["hooks"][0]["args"], json!(["--claude-hook", "SessionStart", "--uc-hook-v1"]));
        assert_eq!(s["model"], "opus", "other settings are kept");
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 2, "the user's own Stop hook is kept");
        let once = s.clone();
        connect(&mut s, EXE).unwrap();
        assert_eq!(s, once, "connecting twice changes nothing");
        assert_eq!(status(&s, r"C:\Other\UC-ClaudeHook-v1.exe"), "outdated");
        // Only the exact proxy with the marker is UC's: a UnifiedControl.exe hook or one without the marker stays.
        let mut other = json!({"hooks": {"Stop": [{"hooks": [
            {"type": "command", "command": r"C:\x\UnifiedControl.exe", "args": ["--claude-hook", "Stop"]},
            {"type": "command", "command": r"C:\x\UC-ClaudeHook-v1.exe", "args": ["--claude-hook", "Stop"]}]}]}});
        assert_eq!(disconnect(&mut other), 0);
        assert_eq!(disconnect(&mut s), 4);
        assert_eq!(s, json!({"model": "opus", "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify.exe"}]}]}}));
        assert_eq!(status(&s, EXE), "off");
        assert!(connect(&mut json!({"hooks": []}), EXE).is_err(), "an odd shape changes nothing");
        assert!(hooks_blocked(&json!({}), &[json!({"allowManagedHooksOnly": true})]).is_some());
        assert!(hooks_blocked(&json!({"disableAllHooks": true}), &[]).is_some());
        assert!(hooks_blocked(&json!({}), &[json!({"strictPluginOnlyCustomization": ["hooks"]})]).is_some());
        assert!(hooks_blocked(&json!({}), &[]).is_none());
        assert!(is_proxy(Path::new(EXE)) && !is_proxy(Path::new(r"C:\x\UnifiedControl.exe")));

        // The hook sends only its fields; a prompt becomes an 80-character label; no desktop id, nothing sent.
        let stdin = json!({"session_id": CODE, "cwd": "C:\\Users\\x\\Documents\\Unified\u{7}Control", "prompt": "Fix\nthe cooler ".repeat(20), "transcript_path": "secret"});
        let p = hook_payload("UserPromptSubmit", &stdin, Some(ID), None, 5).unwrap();
        assert_eq!(p.as_object().unwrap().len(), 6);
        assert!(p.get("transcript_path").is_none());
        assert_eq!(p["cwd"], r"C:\Users\x\Documents\UnifiedControl", "control characters are dropped");
        assert!(p["label"].as_str().unwrap().chars().count() <= LABEL_MAX && !p["label"].as_str().unwrap().contains('\n'));
        assert_eq!(hook_payload("UserPromptSubmit", &stdin, None, None, 5), None);
        assert_eq!(hook_payload("PreToolUse", &stdin, Some(ID), None, 5), None);
        assert!(project_name(&format!("C:\\{}", "x".repeat(200))).chars().count() <= PROJECT_MAX);

        let dir = temp("sessions");
        let list = Sessions::load(dir.join("claude-sessions.json"));
        list.record(&hook_payload("SessionStart", &stdin, Some(ID), None, 1).unwrap(), 1).unwrap();
        list.record(&p, 2).unwrap();
        let now = list.list(3);
        assert_eq!(now[0]["project"], "UnifiedControl");
        assert_eq!(now[0]["state"], "working");
        assert!(now[0]["label"].as_str().unwrap().starts_with("Fix the cooler"));
        assert_eq!(Sessions::load(dir.join("claude-sessions.json")).list(3).len(), 1, "saved in the app folder");
        assert!(list.list(2 + IDLE_EXPIRY_MS).is_empty(), "24 h without activity");
        // A child session sharing the desktop id changes nothing, and its SessionEnd does not remove the parent.
        let child_stop = hook_payload("Stop", &json!({}), Some(ID), Some(CHILD), 4).unwrap();
        list.record(&child_stop, 4).unwrap();
        assert_eq!(list.list(5)[0]["state"], "working");
        list.record(&hook_payload("SessionEnd", &json!({}), Some(ID), Some(CHILD), 4).unwrap(), 4).unwrap();
        assert_eq!(list.list(5).len(), 1);
        list.record(&hook_payload("SessionEnd", &stdin, Some(ID), None, 4).unwrap(), 4).unwrap();
        assert!(list.list(5).is_empty(), "its own SessionEnd removes it");
        assert_eq!(session_link(ID).unwrap(), format!("claude://claude.ai/epitaxy/{ID}"));
        assert!(session_link("local_../../x").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Claude's settings file: never written when nothing changes, backed up first, untouched when invalid,
    /// unchanged when someone else edits it meanwhile or the replace fails; BOM, missing and empty files.
    #[test]
    fn the_claude_settings_file_is_changed_safely() {
        let dir = temp("file");
        let path = dir.join("settings.json");
        let add = |v: &mut Value| connect(v, EXE);
        let backups = || std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(BACKUP_PREFIX)).count();

        // Missing file: created, no backup.
        let r = update_file(&path, 1, add, &|| {}).unwrap();
        assert_eq!(r, Updated { changed: true, backup: None });
        assert_eq!(status(&read_settings(&path).unwrap(), EXE), "connected");
        // Already connected: nothing written, no backup.
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(update_file(&path, 2, add, &|| {}).unwrap(), Updated { changed: false, backup: None });
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(backups(), 0);

        // Invalid JSON: refused, file untouched.
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(update_file(&path, 3, add, &|| {}).unwrap_err().contains("not valid JSON"));
        assert_eq!(std::fs::read(&path).unwrap(), b"{ not json");
        assert!(disconnect_file(&path, 3).is_err());

        // BOM and an empty file are read; the backup holds the old bytes before the write.
        let bom = "\u{feff}{\"model\":\"opus\"}".as_bytes().to_vec();
        std::fs::write(&path, &bom).unwrap();
        let r = update_file(&path, 4, add, &|| {}).unwrap();
        assert_eq!(std::fs::read(r.backup.unwrap()).unwrap(), bom);
        assert_eq!(read_settings(&path).unwrap()["model"], "opus");
        std::fs::write(&path, b"  ").unwrap();
        assert!(update_file(&path, 5, add, &|| {}).unwrap().changed);

        // Someone else edits the file meanwhile: aborted, their version kept.
        std::fs::write(&path, b"{}").unwrap();
        let err = update_file(&path, 6, add, &|| std::fs::write(&path, b"{\"theirs\":1}").unwrap()).unwrap_err();
        assert!(err.contains("changed while"));
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"theirs\":1}");
        assert!(!dir.join("settings.json.uc-tmp").exists());

        // The replace fails (the file is held open without delete sharing): the old file stays.
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::write(&path, b"{}").unwrap();
            let lock = std::fs::OpenOptions::new().read(true).share_mode(1).open(&path).unwrap();
            assert!(update_file(&path, 7, add, &|| {}).unwrap_err().contains("could not be replaced"));
            drop(lock);
            assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        }

        // Only UC's last three backups are kept.
        for t in 10..16 {
            std::fs::write(&path, format!("{{\"n\":{t}}}")).unwrap();
            update_file(&path, t, add, &|| {}).unwrap();
        }
        assert_eq!(backups(), BACKUPS_KEPT);
        assert!(dir.join(format!("{BACKUP_PREFIX}15")).exists());
        assert_eq!(disconnect_file(&path, 20).unwrap(), 4);
        assert_eq!(read_settings(&path).unwrap(), json!({"n": 15}));
        let _ = std::fs::remove_dir_all(dir);
    }
}
