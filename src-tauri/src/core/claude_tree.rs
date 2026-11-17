//! The complete Claude session list with real parents (user request 2026-11-17), read-only from the Claude
//! desktop app's own session files: `%LOCALAPPDATA%\Claude-Data\claude_<suffix>\claude-code-sessions\
//! <account>\<org>\local_<uuid>.json`, one per session.
//!
//! Only these fields are read and kept: `sessionId`, `title`, `spawnedFrom.sessionId` (the parent),
//! `startChain.firstSessionId` / `depth`, `cwd`, `originCwd`, `worktreeName`, `isArchived`, `lastActivityAt`,
//! `lastTurnEdgeAt`. Everything else in those files (reports, reminders, permission data...) is ignored and
//! never stored or logged. The folder is never written. Bounded: files over 1 MB are skipped, at most 500 are
//! parsed, the result is cached for 5 s. Debug builds and test (fake or quiet) instances read only
//! `UC_CLAUDE_DATA` (a test folder), never the real one.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const MAX_FILE: u64 = 1024 * 1024;
pub const MAX_FILES: usize = 500;
pub const CACHE: Duration = Duration::from_secs(5);
/// A session counts as working when Claude recorded activity this recently (or the hooks say so).
pub const WORKING_MS: u64 = 2 * 60 * 1000;
const MAX_DEPTH: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopSession {
    pub id: String,
    pub title: String,
    pub parent: Option<String>,
    pub first: Option<String>,
    pub depth: Option<u64>,
    pub cwd: String,
    pub origin_cwd: String,
    pub worktree: Option<String>,
    pub archived: bool,
    pub last_activity: u64,
    pub last_turn: Option<u64>,
}

fn text(v: &Value, max: usize) -> String {
    let t: String = v.as_str().unwrap_or("").chars().map(|c| if c.is_control() { ' ' } else { c }).take(max).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One session file, only the listed fields. `None` when it is not a session (no valid `sessionId`).
pub fn parse(bytes: &[u8]) -> Option<DesktopSession> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    let id = v["sessionId"].as_str().filter(|i| super::claude_sessions::valid_desktop_id(i))?.to_string();
    let valid = |x: &Value| x.as_str().filter(|i| super::claude_sessions::valid_desktop_id(i)).map(str::to_string);
    Some(DesktopSession {
        id,
        title: text(&v["title"], 120),
        parent: valid(&v["spawnedFrom"]["sessionId"]),
        first: valid(&v["startChain"]["firstSessionId"]),
        depth: v["startChain"]["depth"].as_u64(),
        cwd: text(&v["cwd"], 1024),
        origin_cwd: text(&v["originCwd"], 1024),
        worktree: Some(text(&v["worktreeName"], 60)).filter(|w| !w.is_empty()),
        archived: v["isArchived"].as_bool().unwrap_or(false),
        last_activity: v["lastActivityAt"].as_u64().unwrap_or(0),
        last_turn: v["lastTurnEdgeAt"].as_u64(),
    })
}

/// The `Claude-Data` folder: `%LOCALAPPDATA%\Claude-Data`; debug builds and test instances use only
/// `UC_CLAUDE_DATA`.
pub fn base_dir(test: bool) -> Option<PathBuf> {
    if test || cfg!(debug_assertions) {
        return std::env::var_os("UC_CLAUDE_DATA").map(PathBuf::from);
    }
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Claude-Data"))
}

fn dirs(p: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(p)
        .map(|r| r.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// Read every session file under `base` (bounded). `Err` when the folder or the format is missing.
pub fn read_all(base: &Path) -> Result<Vec<DesktopSession>, String> {
    let mut files = Vec::new();
    for app in dirs(base).into_iter().filter(|d| d.file_name().is_some_and(|n| n.to_string_lossy().starts_with("claude_"))) {
        for account in dirs(&app.join("claude-code-sessions")) {
            for org in dirs(&account) {
                for e in std::fs::read_dir(&org).into_iter().flatten().flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.starts_with("local_") && name.ends_with(".json") && e.file_type().is_ok_and(|t| t.is_file()) {
                        files.push((e.path(), e.metadata().map(|m| m.len()).unwrap_or(u64::MAX)));
                    }
                }
            }
        }
    }
    if files.is_empty() {
        return Err("Claude's session files were not found".into());
    }
    let mut out = Vec::new();
    for (path, _) in files.into_iter().filter(|(_, l)| *l <= MAX_FILE).take(MAX_FILES) {
        if let Some(s) = std::fs::read(&path).ok().and_then(|b| parse(&b)) {
            out.push(s);
        }
    }
    if out.is_empty() {
        return Err("Claude's session files have a format Unified Control does not know".into());
    }
    Ok(out)
}

static CACHED: Mutex<Option<(Instant, PathBuf, Result<Vec<DesktopSession>, String>)>> = Mutex::new(None);

/// [`read_all`], cached for 5 s.
pub fn cached(base: &Path) -> Result<Vec<DesktopSession>, String> {
    let mut c = CACHED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, p, r)) = c.as_ref() {
        if p == base && at.elapsed() < CACHE {
            return r.clone();
        }
    }
    let r = read_all(base);
    *c = Some((Instant::now(), base.to_path_buf(), r.clone()));
    r
}

fn folder_name(p: &str) -> String {
    let n = p.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("").to_string();
    if n.is_empty() { "Other".into() } else { n }
}

/// The menu tree: projects (the `originCwd` folder) at the top, newest activity first; under each, the sessions
/// without a (visible) parent, and under every session its spawned sessions, any depth. Archived sessions are
/// hidden. A session with children is a sub-menu whose first entry opens the session itself; it carries its
/// session, so K1 can dictate into it. `working` is the hooks' live list (desktop ids reported working).
pub fn tree(sessions: &[DesktopSession], working: &[String], then: &Value, now_ms: u64) -> Vec<Value> {
    let visible: Vec<&DesktopSession> = sessions.iter().filter(|s| !s.archived).collect();
    let known = |id: &str| visible.iter().any(|s| s.id == id);
    let busy = |s: &DesktopSession| working.contains(&s.id) || now_ms.saturating_sub(s.last_activity) < WORKING_MS;
    // Newest activity of a session and everything below it.
    fn newest(s: &DesktopSession, all: &[&DesktopSession], depth: usize) -> u64 {
        let own = s.last_activity.max(s.last_turn.unwrap_or(0));
        if depth >= MAX_DEPTH {
            return own;
        }
        all.iter().filter(|c| c.parent.as_deref() == Some(&s.id)).map(|c| newest(c, all, depth + 1)).fold(own, u64::max)
    }
    fn node(s: &DesktopSession, all: &[&DesktopSession], depth: usize, ctx: &dyn Fn(&DesktopSession) -> Value) -> Value {
        let mut kids: Vec<&&DesktopSession> = all.iter().filter(|c| c.parent.as_deref() == Some(&s.id) && c.id != s.id).collect();
        kids.sort_by_key(|c| std::cmp::Reverse(newest(c, all, depth + 1)));
        let me = ctx(s);
        if kids.is_empty() || depth >= MAX_DEPTH {
            return me;
        }
        let mut entries = vec![json!({"type": "claudeSession", "label": format!("Open: {}", me["label"].as_str().unwrap_or("")),
                                      "session": s.id, "then": me["then"], "detail": "this session"})];
        entries.extend(kids.into_iter().map(|c| node(c, all, depth + 1, ctx)));
        // Working when it or anything below it works.
        let working = me["working"] == true || entries.iter().any(|e| e["working"] == true);
        json!({"type": "submenu", "label": me["label"], "session": s.id, "working": working, "detail": me["detail"], "entries": entries})
    }
    let row = |s: &DesktopSession| {
        let working = busy(s);
        let since = now_ms.saturating_sub(s.last_activity);
        let label = if !s.title.is_empty() { s.title.clone() } else { s.worktree.clone().unwrap_or_else(|| folder_name(&s.cwd)) };
        let mut detail = String::new();
        if working {
            detail.push_str("● working · ");
        }
        if let Some(w) = &s.worktree {
            detail.push_str(&format!("{w} · "));
        }
        detail.push_str(&super::au05_menu::age_text(since));
        json!({"type": "claudeSession", "label": label, "session": s.id, "then": then, "working": working, "detail": detail})
    };
    // Roots: no parent, or a parent that is hidden (archived) or missing.
    let roots: Vec<&DesktopSession> = visible.iter().copied().filter(|s| s.parent.as_deref().is_none_or(|p| !known(p))).collect();
    let mut projects: Vec<(String, u64)> = Vec::new();
    for r in &roots {
        let p = folder_name(if r.origin_cwd.is_empty() { &r.cwd } else { &r.origin_cwd });
        let n = newest(r, &visible, 0);
        match projects.iter_mut().find(|(q, _)| q.eq_ignore_ascii_case(&p)) {
            Some(e) => e.1 = e.1.max(n),
            None => projects.push((p, n)),
        }
    }
    projects.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    projects
        .into_iter()
        .map(|(p, _)| {
            let mut mine: Vec<&DesktopSession> = roots
                .iter()
                .copied()
                .filter(|r| folder_name(if r.origin_cwd.is_empty() { &r.cwd } else { &r.origin_cwd }).eq_ignore_ascii_case(&p))
                .collect();
            mine.sort_by_key(|s| std::cmp::Reverse(newest(s, &visible, 0)));
            let entries: Vec<Value> = mine.iter().map(|s| node(s, &visible, 0, &row)).collect();
            let working = entries.iter().any(|e| e["working"] == true);
            json!({"type": "submenu", "label": p, "project": true, "session": mine.first().map(|s| s.id.clone()),
                   "working": working, "detail": if working { "● working" } else { "" }, "entries": entries})
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u32) -> String {
        format!("local_0123abcd-0000-4000-8000-{n:012}")
    }
    fn file(n: u32, parent: Option<u32>, origin: &str, archived: bool, last: u64) -> String {
        json!({"sessionId": id(n), "title": format!("S{n}"), "cwd": format!("C:\\x\\{origin}"), "originCwd": format!("C:\\x\\{origin}"),
               "isArchived": archived, "lastActivityAt": last, "spawnedFrom": parent.map(|p| json!({"sessionId": id(p), "taskId": "t", "title": "secret"})),
               "lastTurnReport": "secret report", "pendingSystemReminder": "secret", "sessionPermissionUpdates": [1, 2]})
        .to_string()
    }

    /// Fixture folder: parents to depth 3, an archived session hidden (its child becomes a root), malformed and
    /// non-session files skipped, extra fields ignored, files over 1 MB skipped.
    #[test]
    fn the_tree_follows_real_parents_and_reads_only_listed_fields() {
        let base = std::env::temp_dir().join(format!("uc-claude-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("claude_abc").join("claude-code-sessions").join("acct").join("org");
        std::fs::create_dir_all(&dir).unwrap();
        let w = |n: u32, body: String| std::fs::write(dir.join(format!("{}.json", id(n))), body).unwrap();
        w(1, file(1, None, "UnifiedControl", false, 100));
        w(2, file(2, Some(1), "UnifiedControl", false, 300));
        w(3, file(3, Some(2), "UnifiedControl", false, 200));
        w(4, file(4, Some(3), "UnifiedControl", false, 150)); // depth 3
        w(5, file(5, None, "vault", true, 900)); // archived
        w(6, file(6, Some(5), "vault", false, 50)); // its parent is hidden: a root
        w(7, file(7, None, "CoolerScreen", false, 10));
        w(8, "{not json".into());
        std::fs::write(dir.join("notes.json"), "{}").unwrap();
        std::fs::write(dir.join(format!("{}.json", id(9))), format!("{{\"sessionId\":\"{}\",\"x\":\"{}\"}}", id(9), "y".repeat(1_100_000))).unwrap();
        let all = read_all(&base).unwrap();
        assert_eq!(all.len(), 7, "malformed, non-session and oversized files are skipped");
        let s2 = all.iter().find(|s| s.id == id(2)).unwrap();
        assert_eq!(s2.parent.as_deref(), Some(id(1).as_str()));
        assert!(format!("{s2:?}").find("secret").is_none(), "only the listed fields are kept");

        let t = tree(&all, &[], &json!("none"), 1_000_000);
        let labels: Vec<&str> = t.iter().map(|p| p["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["UnifiedControl", "vault", "CoolerScreen"], "projects by newest activity");
        // UnifiedControl: S1 has S2 (which has S3, which has S4).
        let s1 = &t[0]["entries"][0];
        assert_eq!((s1["type"].as_str(), s1["session"].as_str()), (Some("submenu"), Some(id(1).as_str())));
        assert_eq!(s1["entries"][0]["session"], id(1), "the first entry opens the session itself");
        let s2n = &s1["entries"][1];
        assert_eq!(s2n["session"], id(2));
        let s3n = &s2n["entries"][1];
        assert_eq!(s3n["session"], id(3));
        assert_eq!(s3n["entries"][1]["session"], id(4), "depth 3");
        assert_eq!(s3n["entries"][1]["type"], "claudeSession", "a leaf");
        // vault: the archived S5 is hidden; S6 is a root.
        assert_eq!(t[1]["entries"].as_array().unwrap().len(), 1);
        assert_eq!(t[1]["entries"][0]["session"], id(6));
        assert!(!json!(t).to_string().contains(&id(5)), "archived sessions are hidden");
        // Working: recent activity, or the hooks say so.
        let t = tree(&all, &[id(7)], &json!("none"), 300 + WORKING_MS - 1);
        assert_eq!(t[0]["working"], true);
        assert_eq!(t.iter().find(|p| p["label"] == "CoolerScreen").unwrap()["entries"][0]["working"], true);
        // A missing folder is reported (the menu then uses the hook list).
        assert!(read_all(&base.join("none")).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
