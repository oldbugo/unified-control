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
//!
//! The user's projects are their sidebar groups in the Claude Code tab (user request 2026-11-17), read-only from
//! `claude_<suffix>\claude_desktop_config.json`: only `preferences.epitaxyPrefs["dframe-group-scopes"]
//! ["<account>/<org>"].groups` and `.assignments`. The file also holds connector (MCP) settings and other
//! preferences: nothing else is read into Unified Control (unknown fields are skipped while parsing), kept,
//! logged or shown. Files over 2 MB are skipped; it is re-read with the session files (5 s cache).
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const MAX_FILE: u64 = 1024 * 1024;
pub const MAX_FILES: usize = 500;
pub const CACHE: Duration = Duration::from_secs(5);
/// A session counts as working when Claude recorded activity this recently (or the hooks say so).
pub const WORKING_MS: u64 = 2 * 60 * 1000;
const MAX_DEPTH: usize = 8;
pub const MAX_CONFIG: u64 = 2 * 1024 * 1024;
pub const OTHER: &str = "Other sessions";

/// What the menu reads from the Claude desktop app: its sessions, and its sidebar groups when readable.
#[derive(Clone, Debug)]
pub struct Desktop {
    pub sessions: Vec<DesktopSession>,
    pub groups: Result<Groups, String>,
}

/// The sidebar groups, per `<account>/<org>` scope.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    pub scopes: HashMap<String, Scope>,
}

/// One scope: the groups in sidebar order (id, name) and which group each session is in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    pub groups: Vec<(String, String)>,
    pub assignments: HashMap<String, String>,
}

// Only these keys are taken from the config file; serde skips everything else without keeping it.
#[derive(Deserialize)]
struct ConfigFile {
    preferences: Option<PrefsFile>,
}
#[derive(Deserialize)]
struct PrefsFile {
    #[serde(rename = "epitaxyPrefs")]
    epitaxy: Option<EpitaxyFile>,
}
#[derive(Deserialize)]
struct EpitaxyFile {
    #[serde(rename = "dframe-group-scopes")]
    scopes: Option<HashMap<String, ScopeFile>>,
}
#[derive(Deserialize)]
struct ScopeFile {
    #[serde(default)]
    groups: Value,
    #[serde(default)]
    assignments: Value,
}

/// The sidebar groups from one config file (only `groups` and `assignments`). `None` when the key is missing.
pub fn parse_groups(bytes: &[u8]) -> Option<Groups> {
    let f: ConfigFile = serde_json::from_slice(bytes).ok()?;
    let scopes = f.preferences?.epitaxy?.scopes?;
    let mut out = Groups::default();
    for (key, sc) in scopes {
        let groups: Vec<(String, String)> = sc
            .groups
            .as_array()
            .into_iter()
            .flatten()
            .filter(|g| g["mode"].as_str().is_none_or(|m| m == "code"))
            .filter_map(|g| Some((g["id"].as_str()?.to_string(), text(&g["name"], 80))))
            .filter(|(id, name)| !id.is_empty() && !name.is_empty())
            .collect();
        let assignments = sc
            .assignments
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| {
                let id = k.strip_prefix("code:")?;
                let group = v.as_str()?;
                super::claude_sessions::valid_desktop_id(id).then(|| (id.to_string(), group.to_string()))
            })
            .collect();
        out.scopes.insert(key, Scope { groups, assignments });
    }
    Some(out)
}

/// Read the sidebar groups of every `claude_*` folder under `base`. `Err` when none could be read.
pub fn read_groups(base: &Path) -> Result<Groups, String> {
    let mut out = Groups::default();
    let mut found = false;
    for app in dirs(base).into_iter().filter(|d| d.file_name().is_some_and(|n| n.to_string_lossy().starts_with("claude_"))) {
        let f = app.join("claude_desktop_config.json");
        if std::fs::metadata(&f).map(|m| m.is_file() && m.len() <= MAX_CONFIG).unwrap_or(false) {
            if let Some(g) = std::fs::read(&f).ok().and_then(|b| parse_groups(&b)) {
                found = true;
                out.scopes.extend(g.scopes);
            }
        }
    }
    if found { Ok(out) } else { Err("Claude's sidebar groups were not found".into()) }
}

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
    /// `<account>/<org>`: the folder the file is in (matches a sidebar group scope).
    pub scope: String,
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
        scope: String::new(),
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
                        let scope = format!("{}/{}", account.file_name().unwrap_or_default().to_string_lossy(), org.file_name().unwrap_or_default().to_string_lossy());
                        files.push((e.path(), e.metadata().map(|m| m.len()).unwrap_or(u64::MAX), scope));
                    }
                }
            }
        }
    }
    if files.is_empty() {
        return Err("Claude's session files were not found".into());
    }
    let mut out = Vec::new();
    for (path, _, scope) in files.into_iter().filter(|(_, l, _)| *l <= MAX_FILE).take(MAX_FILES) {
        if let Some(mut s) = std::fs::read(&path).ok().and_then(|b| parse(&b)) {
            s.scope = scope;
            out.push(s);
        }
    }
    if out.is_empty() {
        return Err("Claude's session files have a format Unified Control does not know".into());
    }
    Ok(out)
}

/// The sessions ([`read_all`]) and the sidebar groups ([`read_groups`]).
pub fn read_desktop(base: &Path) -> Result<Desktop, String> {
    let sessions = read_all(base)?;
    Ok(Desktop { sessions, groups: read_groups(base) })
}

static CACHED: Mutex<Option<(Instant, PathBuf, Result<Desktop, String>)>> = Mutex::new(None);

/// [`read_desktop`], cached for 5 s.
pub fn cached(base: &Path) -> Result<Desktop, String> {
    let mut c = CACHED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, p, r)) = c.as_ref() {
        if p == base && at.elapsed() < CACHE {
            return r.clone();
        }
    }
    let r = read_desktop(base);
    *c = Some((Instant::now(), base.to_path_buf(), r.clone()));
    r
}

fn folder_name(p: &str) -> String {
    let n = p.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("").to_string();
    if n.is_empty() { "Other".into() } else { n }
}

/// How the menu orders and folds sessions (1.0, user request 2026-11-18: "an exposed view of the most recent
/// ones that we have spoken to ... alongside the rest of the list"; things not used for a while "collapse"
/// into an entry that shows them all).
#[derive(Clone, Debug, Default)]
pub struct Recent {
    /// Desktop id -> when the user last sent it a message (the hooks' UserPromptSubmit).
    pub spoke: HashMap<String, u64>,
    /// Entries not used within this long go into a "More" entry at the end of their level. `None`: never.
    pub fold_ms: Option<u64>,
}

/// At least this many entries stay visible at each level before anything is folded.
pub const KEEP_SHOWN: usize = 3;
pub const MORE: &str = "More";

impl Recent {
    /// When the user last used a session: their last message if the hooks saw one, else Claude's own time.
    pub fn at(&self, s: &DesktopSession) -> u64 {
        match self.spoke.get(&s.id) {
            Some(t) => *t,
            None => s.last_activity.max(s.last_turn.unwrap_or(0)),
        }
    }
}

/// Fold one level (sorted newest first; every entry carries `at`): entries used within `fold_ms`, working
/// ones and the first [`KEEP_SHOWN`] stay; the rest go, in the same order, into a last "More" sub-menu.
/// Nothing is folded when it would hide only one entry. `fixed` leading entries (a session's "Open: ...") are
/// never folded or counted.
pub fn fold(entries: Vec<Value>, fixed: usize, r: &Recent, now_ms: u64) -> Vec<Value> {
    let Some(window) = r.fold_ms else { return entries };
    let (head, rest) = entries.split_at(fixed.min(entries.len()));
    let mut shown = Vec::new();
    let mut folded = Vec::new();
    for e in rest {
        let fresh = now_ms.saturating_sub(e["at"].as_u64().unwrap_or(0)) < window;
        if fresh || e["working"] == true || shown.len() < KEEP_SHOWN {
            shown.push(e.clone());
        } else {
            folded.push(e.clone());
        }
    }
    if folded.len() < 2 {
        return entries;
    }
    let newest = folded.iter().filter_map(|e| e["at"].as_u64()).max().unwrap_or(0);
    let mut out = head.to_vec();
    out.extend(shown);
    out.push(json!({"type": "submenu", "more": true, "label": format!("{MORE} ({})", folded.len()),
                    "detail": format!("not used for {}", super::au05_menu::age_text(now_ms.saturating_sub(newest))),
                    "at": newest, "entries": folded}));
    out
}

/// The menu tree, newest first at every level (1.0). With the sidebar groups (`groups`): the groups are the
/// projects, each with its sessions; a session goes under its assigned group, a sub-session without one under
/// its parent's group (still as its parent's child); sessions in no group go in an "Other sessions" entry, by
/// folder. Without the groups: the projects are the `originCwd` folders. Inside each, the sessions without a
/// (visible) parent there, and under every session its spawned sessions, any depth. Every level is ordered by
/// when the user last used it (a project or a session counts its newest session below it), and what was not
/// used within `recent.fold_ms` is folded into a last "More" entry ([`fold`]). Archived sessions are hidden.
/// A session with children is a sub-menu whose first entry opens the session itself; every node carries a
/// session, so K1 can dictate into it. `working` is the hooks' live list (desktop ids reported working).
/// Empty groups are left out.
pub fn tree(sessions: &[DesktopSession], groups: Option<&Groups>, working: &[String], then: &Value, now_ms: u64) -> Vec<Value> {
    tree_with(sessions, groups, working, then, now_ms, &Recent::default())
}

pub fn tree_with(sessions: &[DesktopSession], groups: Option<&Groups>, working: &[String], then: &Value, now_ms: u64, recent: &Recent) -> Vec<Value> {
    let visible: Vec<&DesktopSession> = sessions.iter().filter(|s| !s.archived).collect();
    let row = |s: &DesktopSession| {
        let working = working.contains(&s.id) || now_ms.saturating_sub(s.last_activity) < WORKING_MS;
        let at = recent.at(s);
        let since = now_ms.saturating_sub(at);
        let label = if !s.title.is_empty() { s.title.clone() } else { s.worktree.clone().unwrap_or_else(|| folder_name(&s.cwd)) };
        let mut detail = String::new();
        if working {
            detail.push_str("● working · ");
        }
        if let Some(w) = &s.worktree {
            detail.push_str(&format!("{w} · "));
        }
        detail.push_str(&super::au05_menu::age_text(since));
        json!({"type": "claudeSession", "label": label, "session": s.id, "then": then, "working": working, "detail": detail, "at": at})
    };
    let cx = Cx { row: &row, recent, now_ms };
    let Some(groups) = groups else {
        return fold(folders(&visible, &cx), 0, recent, now_ms);
    };
    // Each session's group: its own assignment, else its parent's (through archived parents too).
    let group_of = |s: &DesktopSession| -> Option<(String, String)> {
        let mut cur = s;
        for _ in 0..=MAX_DEPTH {
            if let Some(sc) = groups.scopes.get(&cur.scope) {
                if let Some(g) = sc.assignments.get(&cur.id).filter(|g| sc.groups.iter().any(|(id, _)| id == *g)) {
                    return Some((cur.scope.clone(), g.clone()));
                }
            }
            cur = sessions.iter().find(|p| Some(&p.id) == cur.parent.as_ref() && p.id != cur.id)?;
        }
        None
    };
    let tagged: Vec<(&DesktopSession, Option<(String, String)>)> = visible.iter().map(|s| (*s, group_of(s))).collect();
    let mut scopes: Vec<&String> = groups.scopes.keys().collect();
    scopes.sort();
    let mut out = Vec::new();
    for scope in scopes {
        for (gid, name) in &groups.scopes[scope].groups {
            let mine: Vec<&DesktopSession> =
                tagged.iter().filter(|(_, g)| g.as_ref().is_some_and(|(sc, id)| sc == scope && id == gid)).map(|(s, _)| *s).collect();
            if !mine.is_empty() {
                let mut p = project(name, &mine, &cx);
                p["group"] = json!(gid); // a stable id for the menu's memory
                out.push(p);
            }
        }
    }
    let rest: Vec<&DesktopSession> = tagged.iter().filter(|(_, g)| g.is_none()).map(|(s, _)| *s).collect();
    if !rest.is_empty() {
        let entries = fold(folders(&rest, &cx), 0, recent, now_ms);
        let working = entries.iter().any(|e| e["working"] == true);
        let at = entries.iter().filter_map(|e| e["at"].as_u64()).max().unwrap_or(0);
        out.push(json!({"type": "submenu", "label": OTHER, "project": true, "working": working, "at": at,
                        "detail": if working { "● working" } else { "" }, "entries": entries}));
    }
    // Newest first (a stable sort keeps the sidebar order between equals).
    out.sort_by_key(|e| std::cmp::Reverse(e["at"].as_u64().unwrap_or(0)));
    fold(out, 0, recent, now_ms)
}

struct Cx<'a> {
    row: &'a dyn Fn(&DesktopSession) -> Value,
    recent: &'a Recent,
    now_ms: u64,
}

/// Every open Claude session as one flat list, the one the user last spoke to first (1.0, the "Recent
/// sessions" list menu). Each row names its project (as the menu groups them) in its detail.
pub fn recent_list(sessions: &[DesktopSession], groups: Option<&Groups>, working: &[String], now_ms: u64, recent: &Recent) -> Vec<Value> {
    let r = Recent { fold_ms: None, ..recent.clone() };
    let t = tree_with(sessions, groups, working, &json!("none"), now_ms, &r);
    let mut rows: Vec<Value> = Vec::new();
    fn walk(e: &Value, project: &str, rows: &mut Vec<Value>) {
        if e["type"] == "claudeSession" && !e["label"].as_str().unwrap_or("").starts_with("Open: ") {
            if !rows.iter().any(|x| x["session"] == e["session"]) {
                let mut x = e.clone();
                let rest = e["detail"].as_str().unwrap_or("");
                x["project"] = json!(project);
                x["detail"] = json!(if project.is_empty() { rest.to_string() } else { format!("{project} · {rest}") });
                rows.push(x);
            }
        }
        if e["type"] == "submenu" && e["project"] != true && e["session"].is_string() {
            // A session with sub-sessions: the session itself, then its children.
            let me = e["entries"].as_array().and_then(|a| a.first()).cloned();
            if let Some(mut m) = me {
                m["label"] = e["label"].clone();
                m["detail"] = e["detail"].clone();
                walk(&m, project, rows);
            }
        }
        for c in e["entries"].as_array().into_iter().flatten() {
            walk(c, project, rows);
        }
    }
    for p in &t {
        if p["label"] == OTHER {
            for f in p["entries"].as_array().into_iter().flatten() {
                walk(f, f["label"].as_str().unwrap_or(""), &mut rows);
            }
        } else {
            walk(p, p["label"].as_str().unwrap_or(""), &mut rows);
        }
    }
    rows.sort_by_key(|e| std::cmp::Reverse(e["at"].as_u64().unwrap_or(0)));
    rows
}

/// The live summary for the page's Claude sessions card (0.36.0): open sessions, projects (as the menu groups
/// them), how many work now, and the newest few with their project. Titles only; nothing else from the files.
pub fn summary(d: &Desktop, working: &[String], now_ms: u64, recent: usize) -> Value {
    let t = tree(&d.sessions, d.groups.as_ref().ok(), working, &json!("none"), now_ms);
    // Each session with the project it is listed under ("Other sessions" counts each folder).
    let mut found: Vec<(String, String, bool)> = Vec::new();
    fn walk(e: &Value, project: &str, out: &mut Vec<(String, String, bool)>) {
        if let Some(id) = e["session"].as_str() {
            if !out.iter().any(|(i, _, _)| i == id) && e["project"] != true {
                out.push((id.to_string(), project.to_string(), e["working"] == true));
            }
        }
        for c in e["entries"].as_array().into_iter().flatten() {
            walk(c, project, out);
        }
    }
    let mut projects = 0;
    for p in &t {
        if p["label"] == OTHER {
            for f in p["entries"].as_array().into_iter().flatten() {
                projects += 1;
                walk(f, f["label"].as_str().unwrap_or(""), &mut found);
            }
        } else {
            projects += 1;
            walk(p, p["label"].as_str().unwrap_or(""), &mut found);
        }
    }
    let mut rows: Vec<(&DesktopSession, &String, bool)> = found
        .iter()
        .filter_map(|(id, p, w)| d.sessions.iter().find(|s| &s.id == id).map(|s| (s, p, *w)))
        .collect();
    rows.sort_by_key(|(s, _, _)| std::cmp::Reverse(s.last_activity.max(s.last_turn.unwrap_or(0))));
    json!({
        "open": d.sessions.iter().filter(|s| !s.archived).count(),
        "projects": projects,
        "working": found.iter().filter(|(_, _, w)| *w).count(),
        "recent": rows.iter().take(recent).map(|(s, p, w)| json!({
            "title": if s.title.is_empty() { s.worktree.clone().unwrap_or_else(|| folder_name(&s.cwd)) } else { s.title.clone() },
            "project": p, "working": w, "at": s.last_activity.max(s.last_turn.unwrap_or(0)),
        })).collect::<Vec<_>>(),
    })
}

/// When the user last used a session or anything below it (within `all`).
fn newest(s: &DesktopSession, all: &[&DesktopSession], depth: usize, r: &Recent) -> u64 {
    let own = r.at(s);
    if depth >= MAX_DEPTH {
        return own;
    }
    all.iter().filter(|c| c.parent.as_deref() == Some(&s.id) && c.id != s.id).map(|c| newest(c, all, depth + 1, r)).fold(own, u64::max)
}

fn node(s: &DesktopSession, all: &[&DesktopSession], depth: usize, cx: &Cx) -> Value {
    let mut kids: Vec<&&DesktopSession> = all.iter().filter(|c| c.parent.as_deref() == Some(&s.id) && c.id != s.id).collect();
    kids.sort_by_key(|c| std::cmp::Reverse(newest(c, all, depth + 1, cx.recent)));
    let mut me = (cx.row)(s);
    me["at"] = json!(newest(s, all, depth, cx.recent));
    if kids.is_empty() || depth >= MAX_DEPTH {
        return me;
    }
    let mut entries = vec![json!({"type": "claudeSession", "label": format!("Open: {}", me["label"].as_str().unwrap_or("")),
                                  "session": s.id, "then": me["then"], "detail": "this session", "at": cx.recent.at(s)})];
    entries.extend(kids.into_iter().map(|c| node(c, all, depth + 1, cx)));
    let entries = fold(entries, 1, cx.recent, cx.now_ms);
    // Working when it or anything below it works.
    let working = me["working"] == true || entries.iter().any(|e| e["working"] == true);
    json!({"type": "submenu", "label": me["label"], "session": s.id, "working": working, "detail": me["detail"],
           "at": me["at"], "entries": entries})
}

/// A project (a sidebar group or a folder): its sessions without a parent among `all`, newest first, as trees.
/// It carries its newest session (K1 dictates there) and that session's time.
fn project(label: &str, all: &[&DesktopSession], cx: &Cx) -> Value {
    let mut roots: Vec<&DesktopSession> =
        all.iter().copied().filter(|s| s.parent.as_deref().is_none_or(|p| !all.iter().any(|x| x.id == p))).collect();
    roots.sort_by_key(|s| std::cmp::Reverse(newest(s, all, 0, cx.recent)));
    let at = roots.first().map(|s| newest(s, all, 0, cx.recent)).unwrap_or(0);
    let entries = fold(roots.iter().map(|s| node(s, all, 0, cx)).collect(), 0, cx.recent, cx.now_ms);
    let working = entries.iter().any(|e| e["working"] == true);
    json!({"type": "submenu", "label": label, "project": true, "session": roots.first().map(|s| s.id.clone()),
           "working": working, "detail": if working { "● working" } else { "" }, "at": at, "entries": entries})
}

/// The sessions by folder (`originCwd`, else `cwd`), newest first. A sub-session goes with its tree's root.
fn folders(all: &[&DesktopSession], cx: &Cx) -> Vec<Value> {
    let place = |s: &DesktopSession| folder_name(if s.origin_cwd.is_empty() { &s.cwd } else { &s.origin_cwd });
    let root_of = |s: &DesktopSession| {
        let mut cur = s;
        for _ in 0..MAX_DEPTH {
            match all.iter().find(|p| Some(&p.id) == cur.parent.as_ref() && p.id != cur.id) {
                Some(p) => cur = p,
                None => break,
            }
        }
        place(cur)
    };
    let mut names: Vec<(String, u64)> = Vec::new();
    for s in all {
        let p = root_of(s);
        let n = cx.recent.at(s);
        match names.iter_mut().find(|(q, _)| q.eq_ignore_ascii_case(&p)) {
            Some(e) => e.1 = e.1.max(n),
            None => names.push((p, n)),
        }
    }
    names.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    names
        .into_iter()
        .map(|(p, _)| {
            let mine: Vec<&DesktopSession> = all.iter().copied().filter(|s| root_of(s).eq_ignore_ascii_case(&p)).collect();
            project(&p, &mine, cx)
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

        let t = tree(&all, None, &[], &json!("none"), 1_000_000);
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
        let t = tree(&all, None, &[id(7)], &json!("none"), 300 + WORKING_MS - 1);
        assert_eq!(t[0]["working"], true);
        assert_eq!(t.iter().find(|p| p["label"] == "CoolerScreen").unwrap()["entries"][0]["working"], true);
        // The page's live summary: counts and the newest sessions with their project.
        let sum = summary(&read_desktop(&base).unwrap(), &[], 1_000_000, 2);
        assert_eq!(sum["recent"].as_array().unwrap().len(), 2);
        assert!(sum["open"].as_u64().unwrap() >= 2 && sum["projects"].as_u64().unwrap() >= 1, "{sum}");
        assert!(sum["recent"][0]["at"].as_u64() >= sum["recent"][1]["at"].as_u64(), "newest first");
        // A missing folder is reported (the menu then uses the hook list).
        assert!(read_all(&base.join("none")).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The sidebar groups (fake config file): groups in sidebar order, a session under its assigned group, a
    /// sub-session without its own assignment under its parent's group (as its child), one with its own group
    /// there, the rest in "Other sessions" by folder. Only `groups` and `assignments` are kept; a missing key,
    /// a broken or an oversized file falls back to folders.
    #[test]
    fn sidebar_groups_are_the_projects() {
        let base = std::env::temp_dir().join(format!("uc-claude-groups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let app = base.join("claude_abc");
        let dir = app.join("claude-code-sessions").join("acct").join("org");
        std::fs::create_dir_all(&dir).unwrap();
        let w = |n: u32, body: String| std::fs::write(dir.join(format!("{}.json", id(n))), body).unwrap();
        w(1, file(1, None, "CoolerScreen", false, 100)); // assigned to "Unified Control"
        w(2, file(2, Some(1), "CoolerScreen", false, 300)); // no assignment: under S1
        w(3, file(3, Some(2), "CoolerScreen", false, 200)); // depth 2, inherits through S2
        w(4, file(4, Some(1), "CoolerScreen", false, 150)); // its own group: a root in "Nocturne"
        w(5, file(5, None, "vault", false, 900)); // no group: Other sessions / vault
        w(6, file(6, None, "Nocturne", true, 50)); // archived, assigned
        w(7, file(7, Some(6), "Nocturne", false, 40)); // its archived parent's group
        let config = |groups: Value| {
            json!({"mcpServers": {"x": {"command": "secret-command", "env": {"TOKEN": "secret"}}},
                   "preferences": {"theme": "secret", "epitaxyPrefs": {"other": "secret", "dframe-group-scopes": groups}}})
            .to_string()
        };
        let scopes = json!({
            "acct/org": {
                "groups": [{"id": "cg-1", "name": "Unified Control", "mode": "code"}, {"id": "cg-empty", "name": "Gaze Browser", "mode": "code"},
                           {"id": "cg-2", "name": "Project Nocturne", "mode": "code"}],
                "assignments": {format!("code:{}", id(1)): "cg-1", format!("code:{}", id(4)): "cg-2", format!("code:{}", id(6)): "cg-2",
                                format!("code:{}", id(5)): "cg-gone"},
                "order": ["secret"]
            },
            "other/scope": {"groups": [{"id": "cg-9", "name": "Elsewhere", "mode": "code"}], "assignments": {}}
        });
        std::fs::write(app.join("claude_desktop_config.json"), config(scopes)).unwrap();
        let d = read_desktop(&base).unwrap();
        let g = d.groups.clone().unwrap();
        assert!(!format!("{g:?}").contains("secret"), "only groups and assignments are kept");
        assert_eq!(g.scopes["acct/org"].groups[0], ("cg-1".to_string(), "Unified Control".to_string()));
        assert_eq!(d.sessions.iter().find(|s| s.id == id(1)).unwrap().scope, "acct/org", "the session's folder scope");

        let t = tree(&d.sessions, Some(&g), &[], &json!("none"), 1_000_000);
        let labels: Vec<&str> = t.iter().map(|p| p["label"].as_str().unwrap()).collect();
        assert_eq!(labels, [OTHER, "Unified Control", "Project Nocturne"], "newest first (1.0); empty groups left out");
        // Unified Control: S1 > S2 > S3 (S4 is in its own group).
        let uc = &t[1];
        assert_eq!(uc["session"], id(1), "a group carries its newest session");
        assert_eq!(uc["entries"].as_array().unwrap().len(), 1);
        let s1 = &uc["entries"][0];
        assert_eq!(s1["entries"][0]["session"], id(1), "Open: the session itself");
        assert_eq!(s1["entries"][1]["session"], id(2), "a sub-session without a group stays under its parent");
        assert_eq!(s1["entries"][1]["entries"][1]["session"], id(3), "and so does its child");
        assert_eq!(s1["entries"].as_array().unwrap().len(), 2, "S4 is not here");
        // Project Nocturne: S4 (own assignment) and S7 (its archived parent's group).
        let noc: Vec<&str> = t[2]["entries"].as_array().unwrap().iter().map(|e| e["session"].as_str().unwrap()).collect();
        assert_eq!(noc, [id(4).as_str(), id(7).as_str()]);
        // Other sessions: S5 (assigned to a group that does not exist) by folder.
        assert_eq!(t[0]["entries"][0]["label"], "vault");
        assert_eq!(t[0]["entries"][0]["entries"][0]["session"], id(5));
        assert!(!json!(t).to_string().contains(&id(6)), "archived sessions stay hidden");

        // Fallbacks: no key, a broken file, an oversized file: folders, with a note.
        for body in [json!({"preferences": {"epitaxyPrefs": {}}}).to_string(), "{broken".into(), format!("{{\"x\":\"{}\"}}", "y".repeat(2_200_000))] {
            std::fs::write(app.join("claude_desktop_config.json"), body).unwrap();
            let d = read_desktop(&base).unwrap();
            assert!(d.groups.as_ref().unwrap_err().contains("not found"));
            let t = tree(&d.sessions, d.groups.as_ref().ok(), &[], &json!("none"), 1_000_000);
            let labels: Vec<&str> = t.iter().map(|p| p["label"].as_str().unwrap()).collect();
            assert_eq!(labels, ["vault", "CoolerScreen", "Nocturne"], "folders, a sub-session with its root");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    fn session(n: u32, parent: Option<u32>, origin: &str, last: u64) -> DesktopSession {
        parse(file(n, parent, origin, false, last).as_bytes()).unwrap()
    }

    /// 1.0 (user request 2026-11-18): every level is ordered by when the user last spoke to it (the hooks'
    /// time wins over Claude's own), and what was not used within the fold time goes into a last "More"
    /// entry, keeping at least three shown, working ones always shown, never folding a single entry.
    #[test]
    fn recent_first_and_old_ones_fold_into_more() {
        const H: u64 = 3600 * 1000;
        let now = 100 * H;
        // Project A: five sessions of different ages; A2 has two sub-sessions. Project B: one old session that
        // the user spoke to a minute ago (the hooks know it), so B comes first.
        let mut all = vec![
            session(1, None, "A", now - H),
            session(2, None, "A", now - 2 * H),
            session(3, None, "A", now - 30 * H),
            session(4, None, "A", now - 40 * H),
            session(5, None, "A", now - 50 * H),
            session(6, None, "B", now - 60 * H),
            session(7, Some(2), "A", now - 20 * H),
            session(8, Some(2), "A", now - 21 * H),
        ];
        all.push(session(9, None, "C", now - 90 * H));
        all.push(session(10, None, "D", now - 91 * H));
        let recent = Recent { spoke: HashMap::from([(id(6), now - 60_000)]), fold_ms: Some(8 * H) };
        let t = tree_with(&all, None, &[], &json!("none"), now, &recent);
        let labels: Vec<&str> = t.iter().map(|p| p["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["B", "A", "C", "D"], "four projects, none folded: three are kept and one alone is never folded");
        // A: S1, S2 (fresh), S3 (the third kept), then More with S4 and S5, oldest last.
        let a: Vec<String> = t[1]["entries"].as_array().unwrap().iter().map(|e| e["label"].as_str().unwrap().to_string()).collect();
        assert_eq!(a, ["S1", "S2", "S3", "More (2)"]);
        let more = &t[1]["entries"][3];
        assert_eq!((more["type"].as_str(), more["more"].as_bool()), (Some("submenu"), Some(true)));
        let inside: Vec<&str> = more["entries"].as_array().unwrap().iter().map(|e| e["session"].as_str().unwrap()).collect();
        assert_eq!(inside, [id(4).as_str(), id(5).as_str()], "all there when opened, in order");
        assert_eq!(super::super::au05_menu::entry_key(more), "more", "a stable id for the menu's memory");
        // S2's sub-sessions: "Open" stays first, two children are not folded (fewer than three).
        let s2 = &t[1]["entries"][1];
        assert_eq!(s2["entries"][0]["session"], id(2));
        assert_eq!(s2["entries"].as_array().unwrap().len(), 3);
        // A working session is never folded.
        let t = tree_with(&all, None, &[id(5)], &json!("none"), now, &recent);
        let a: Vec<&str> = t[1]["entries"].as_array().unwrap().iter().map(|e| e["label"].as_str().unwrap()).collect();
        assert_eq!(a, ["S1", "S2", "S3", "S4", "S5"], "S5 works, so only S4 would fold: nothing is folded");
        // Never fold: everything shown, still newest first.
        let t = tree_with(&all, None, &[], &json!("none"), now, &Recent { fold_ms: None, ..recent.clone() });
        assert_eq!(t[1]["entries"].as_array().unwrap().len(), 5);
        assert_eq!(t[0]["label"], "B");
    }
}
