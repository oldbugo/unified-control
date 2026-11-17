//! Vibe Key actions through global hotkeys (orchestrator design, proven on the real key 2026-11-16).
//!
//! Not every AU05 control sends a private event (K2, K3 and the knob press never did). Instead each key types
//! its own unused code (K1 F20, K2 F21, K3 F22, knob press F19) and UC claims that code with
//! `RegisterHotKey(NULL, id, MOD_NOREPEAT, vk)`. Windows then swallows the key press and posts WM_HOTKEY to
//! UC's hotkey thread, whatever app has focus.
//!
//! Everything that touches the user's desktop (hotkeys, focus, synthetic keys) is behind [`Desktop`].
//! Tests and fake instances (`UC_FAKE_DEVICE`) use [`FakeDesktop`], which registers nothing, moves no focus
//! and sends no input.
use super::dock_runtime::PendingAction;
use super::sync::MutexExt;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const VK_F19: u16 = 0x82;
pub const VK_F20: u16 = 0x83;
pub const VK_F21: u16 = 0x84;
pub const VK_F22: u16 = 0x85;
/// Knob turns, when the knob is set to type them (not set by UC yet: needs a check on the real key).
pub const VK_F23: u16 = 0x86;
pub const VK_F24: u16 = 0x87;

/// An app UC can bring forward before it presses the keys.
#[derive(Debug, PartialEq, Eq)]
pub struct App {
    pub id: &'static str,
    pub name: &'static str,
    /// Executable file name, lower case.
    pub exe: &'static str,
    /// The window title starts with this.
    pub title: &'static str,
    /// True when finding and focusing this window was checked on the user's PC.
    pub verified: bool,
}
pub const APPS: [App; 2] = [
    // Checked by the K3 hotkey prototype on 2026-11-16 (user-confirmed: dictation started).
    App { id: "claude", name: "Claude", exe: "claude.exe", title: "Claude", verified: true },
    // Not checked: no source states the ChatGPT app's executable name or title.
    App { id: "chatgpt", name: "ChatGPT", exe: "chatgpt.exe", title: "ChatGPT", verified: false },
];
pub fn app(id: &str) -> Option<&'static App> {
    APPS.iter().find(|a| a.id == id)
}

/// What an action does after the app is in front (if any).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// A key combination through the dock's reviewed hotkey sender ("Ctrl + D").
    Keys(&'static str),
    /// A media key through the dock's reviewed media sender ("Play / pause").
    Media(&'static str),
    /// Follows the target app (quick menu): the Claude entry or the ChatGPT entry.
    Target(&'static str, &'static str),
    /// Claude "Dictate & send": decided from whether Claude is capturing audio (see [`dictate_send`]).
    DictateSend,
}

/// One action in the catalogue. Data only: add apps and actions here.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    /// The app to bring forward first, or `None` for the focused app / the whole system.
    pub app: Option<&'static str>,
    pub effect: Effect,
    /// "verified" (vendor documentation or checked on the user's PC) or "unverified".
    pub status: &'static str,
    pub note: &'static str,
}

const fn e(
    id: &'static str,
    group: &'static str,
    label: &'static str,
    app: Option<&'static str>,
    effect: Effect,
    status: &'static str,
    note: &'static str,
) -> Entry {
    Entry { id, group, label, app, effect, status, note }
}
use Effect::{DictateSend, Keys, Media, Target};
const CL: Option<&str> = Some("claude");
const GPT: Option<&str> = Some("chatgpt");

/// Sources (2026-11-16): Claude Code desktop docs (code.claude.com/docs/en/desktop) list Enter to send and Esc
/// to interrupt; their other shortcuts are given for macOS ("swap Cmd for Ctrl"), so the Windows keys are
/// inferred and marked unverified. Claude dictation Ctrl + D was checked on the user's PC. OpenAI's help pages
/// could not be read; every ChatGPT shortcut is from third-party sites and marked unverified. Stop/interrupt is
/// in the list but never a default (the user does not want a key to stop the agent by accident).
pub const CATALOGUE: &[Entry] = &[
    e("claudeDictation", "Claude", "Start dictation", CL, Keys("Ctrl + D"), "verified", "Checked on this PC"),
    e("claudeSend", "Claude", "Send message", CL, Keys("Enter"), "verified", "Claude Code docs"),
    e("claudeDictateSend", "Claude", "Dictate & send", CL, DictateSend, "unverified", "New: first press starts dictation, the next stops it and sends; it checks Claude's microphone each time"),
    e("claudeNewSession", "Claude", "New session", CL, Keys("Ctrl + N"), "unverified", "Inferred from the macOS list"),
    e("claudeNextSession", "Claude", "Next session", CL, Keys("Ctrl + Tab"), "unverified", "Inferred from the macOS list"),
    e("claudePrevSession", "Claude", "Previous session", CL, Keys("Ctrl + Shift + Tab"), "unverified", "Inferred from the macOS list"),
    e("claudeModelMenu", "Claude", "Model menu", CL, Keys("Ctrl + Shift + I"), "unverified", "Inferred from the macOS list"),
    e("claudePermissionMenu", "Claude", "Permission mode menu", CL, Keys("Ctrl + Shift + M"), "unverified", "Inferred from the macOS list"),
    e("claudeSideChat", "Claude", "Side chat", CL, Keys("Ctrl + ;"), "unverified", "Inferred from the macOS list"),
    e("claudeTerminal", "Claude", "Toggle terminal", CL, Keys("Ctrl + `"), "unverified", "Inferred from the macOS list"),
    e("claudeDiff", "Claude", "Toggle diff pane", CL, Keys("Ctrl + Shift + D"), "unverified", "Inferred from the macOS list"),
    e("claudeShortcuts", "Claude", "Show all shortcuts", CL, Keys("Ctrl + /"), "unverified", "Inferred from the macOS list"),
    e("claudeQuickOpen", "Claude", "Open Claude from anywhere", None, Keys("Ctrl + Alt + Space"), "unverified", "Third-party report"),
    e("claudeStop", "Claude", "Stop / interrupt (Esc)", CL, Keys("Esc"), "verified", "Claude Code docs. Stops the running turn"),
    e("chatgptDictation", "ChatGPT", "Start dictation", GPT, Keys("Ctrl + Shift + D"), "unverified", "Third-party report"),
    e("chatgptNewChat", "ChatGPT", "New chat", GPT, Keys("Ctrl + Shift + O"), "unverified", "Third-party report"),
    e("chatgptSidebar", "ChatGPT", "Toggle sidebar", GPT, Keys("Ctrl + Shift + S"), "unverified", "Third-party report"),
    e("chatgptFocusInput", "ChatGPT", "Focus the message box", GPT, Keys("Shift + Esc"), "unverified", "Third-party report"),
    e("chatgptSend", "ChatGPT", "Send message", GPT, Keys("Enter"), "unverified", "Standard Enter"),
    e("chatgptShortcuts", "ChatGPT", "Show all shortcuts", GPT, Keys("Ctrl + /"), "unverified", "Third-party report"),
    e("chatgptCompanion", "ChatGPT", "Companion window", None, Keys("Alt + Space"), "unverified", "Third-party report; the hotkey can be changed in ChatGPT"),
    e("targetDictation", "Target app", "Start dictation", None, Target("claudeDictation", "chatgptDictation"), "unverified", "Follows the target app (Claude checked; ChatGPT a third-party report)"),
    e("targetSend", "Target app", "Send message", None, Target("claudeSend", "chatgptSend"), "unverified", "Follows the target app"),
    e("targetNew", "Target app", "New session / new chat", None, Target("claudeNewSession", "chatgptNewChat"), "unverified", "Follows the target app"),
    e("enter", "Windows / media", "Enter in the focused app", None, Keys("Enter"), "verified", "Standard key"),
    e("playPause", "Windows / media", "Play / pause", None, Media("Play / pause"), "verified", "Standard media key"),
    e("nextTrack", "Windows / media", "Next track", None, Media("Next track"), "verified", "Standard media key"),
    e("previousTrack", "Windows / media", "Previous track", None, Media("Previous track"), "verified", "Standard media key"),
    e("volumeUp", "Windows / media", "Volume up", None, Media("Volume up"), "verified", "Standard media key"),
    e("volumeDown", "Windows / media", "Volume down", None, Media("Volume down"), "verified", "Standard media key"),
    e("mute", "Windows / media", "Mute sound", None, Media("Mute"), "verified", "Standard media key"),
    e("micMute", "Windows / media", "Mute microphone in calls", None, Keys("Win + Alt + K"), "unverified", "Windows 11; only in call apps that support it"),
    e("showDesktop", "Windows / media", "Show desktop", None, Keys("Win + D"), "verified", "Windows shortcut"),
    e("taskView", "Windows / media", "Task view", None, Keys("Win + Tab"), "verified", "Windows shortcut"),
    e("screenshot", "Windows / media", "Screenshot (Snipping Tool)", None, Keys("Win + Shift + S"), "verified", "Windows shortcut"),
];
pub fn entry(id: &str) -> Option<&'static Entry> {
    CATALOGUE.iter().find(|e| e.id == id)
}
/// The apps "the target app" can mean (quick menu).
pub const TARGETS: [&str; 2] = ["claude", "chatgpt"];

/// Every saved kind: "none", "hotkey" (custom combination), "quickMenu" or a catalogue id.
pub fn kind_known(kind: &str) -> bool {
    kind == "none" || kind == OFFLINE || kind == "hotkey" || kind == QUICK_MENU || entry(kind).is_some()
}
pub const QUICK_MENU: &str = "quickMenu";
/// "Same as on its own": no UC action; the key types its offline layer even while UC runs.
pub const OFFLINE: &str = "offline";

/// The keys this action can leave on the key itself for its offline layer ("On its own"), if any: plain
/// keystrokes the key can store (user direction 2026-11-17). Media keys, the quick menu, Dictate & send,
/// Target-app actions, sessions and profiles need Unified Control. Without UC the keys go to the app in front.
pub fn offline_keys(e: &Entry) -> Option<&'static str> {
    match e.effect {
        Keys(k) if ulanzi_au05::protocol::KeyTyping::parse(k).is_ok() => Some(k),
        _ => None,
    }
}

/// The catalogue for the page, grouped in order.
pub fn catalogue_json() -> serde_json::Value {
    serde_json::json!({
        "actions": CATALOGUE.iter().map(|e| serde_json::json!({
            "id": e.id, "group": e.group, "label": e.label, "status": e.status, "note": e.note,
            "keys": match e.effect { Keys(k) => k, Media(m) => m, Target(..) => "Follows the target app", DictateSend => "Ctrl + D, then Enter" },
            "offlineKeys": offline_keys(e),
        })).collect::<Vec<_>>(),
        "apps": APPS.iter().map(|a| serde_json::json!({"id": a.id, "name": a.name, "verified": a.verified})).collect::<Vec<_>>(),
    })
}

/// The action UC runs when a key's own code arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Entry(&'static Entry),
    /// Open (or close) the quick menu.
    QuickMenu,
    /// A custom key combination, sent to the focused app or after bringing an app forward.
    Custom { keys: String, app: Option<&'static App> },
}

impl Action {
    pub fn from_setting(kind: &str, value: &str, app_id: &str) -> Option<Self> {
        Some(match kind {
            "none" | OFFLINE => Action::Nothing,
            QUICK_MENU => Action::QuickMenu,
            // A custom combination that is still being typed does nothing.
            "hotkey" if value.trim().is_empty() => Action::Nothing,
            "hotkey" => Action::Custom { keys: value.to_string(), app: app(app_id) },
            id => Action::Entry(entry(id)?),
        })
    }
    pub fn label(&self) -> String {
        match self {
            Action::Nothing => "nothing".into(),
            Action::QuickMenu => "Open quick menu".into(),
            Action::Entry(e) => format!("{}: {}", e.group, e.label),
            Action::Custom { keys, app: Some(a) } => format!("{keys} in {}", a.name),
            Action::Custom { keys, app: None } => keys.clone(),
        }
    }
    /// A "Target app" action becomes the Claude or the ChatGPT action. Others are unchanged.
    pub fn resolve(self, target: &str) -> Self {
        match self {
            Action::Entry(Entry { effect: Target(claude, chatgpt), .. }) => {
                let id = if target == "chatgpt" { chatgpt } else { claude };
                entry(id).map(Action::Entry).unwrap_or(Action::Nothing)
            }
            other => other,
        }
    }
    /// The app to bring forward first (exe, title), and what to send. A "Target app" action must be resolved
    /// first; the quick menu action is not sent anywhere.
    pub fn plan(&self) -> Option<(Option<(&'static str, &'static str)>, PendingAction)> {
        let target = |a: Option<&'static App>| a.map(|a| (a.exe, a.title));
        match self {
            Action::Nothing | Action::QuickMenu => None,
            Action::Entry(Entry { effect: Target(..) | DictateSend, .. }) => None,
            Action::Entry(e) => Some((
                target(e.app.and_then(app)),
                match e.effect {
                    Keys(k) => PendingAction { kind: "hotkey".into(), value: k.into() },
                    Media(m) => PendingAction { kind: "media".into(), value: m.into() },
                    Target(..) | DictateSend => unreachable!("matched above"),
                },
            )),
            Action::Custom { keys, app } => {
                Some((target(*app), PendingAction { kind: "hotkey".into(), value: keys.clone() }))
            }
        }
    }
}

/// Check a saved action before it is stored.
pub fn validate(kind: &str, value: &str, app_id: &str) -> Result<(), String> {
    if !app_id.is_empty() && app(app_id).is_none() {
        return Err("Choose a supported app".into());
    }
    match Action::from_setting(kind, value, app_id).ok_or("Choose a supported action")? {
        Action::Custom { keys, .. } => super::actions::plan_action(&PendingAction {
            kind: "hotkey".into(),
            value: keys,
        })
        .map(|_| ()),
        _ => Ok(()),
    }
}

pub type PressHandler = Arc<dyn Fn(u16) + Send + Sync>;

pub trait Desktop: Send + Sync {
    /// Called with the virtual key of a claimed hotkey, on the hotkey thread. Must return quickly.
    fn set_handler(&self, handler: PressHandler);
    /// Claim exactly these keys (release all others). Returns each key's result.
    fn set_hotkeys(&self, vks: &[u16]) -> BTreeMap<u16, Result<(), String>>;
    /// Run an action (may move focus and send keys). Called on its own thread.
    fn run(&self, action: &Action) -> Result<(), String>;
    /// Turn the mouse wheel by whole notches (negative = down): the knob turn's "Scroll" action.
    fn scroll(&self, notches: i32) -> Result<(), String>;
    /// Open a Claude session link (`claude://claude.ai/epitaxy/local_<uuid>`, checked by the caller).
    fn open_link(&self, url: &str) -> Result<(), String>;
    /// Does a process with this exe name (any of its processes) have an active audio capture session?
    fn capture_active(&self, exe: &str) -> Result<bool, String>;
    /// Bring the app's window to the front.
    fn bring_forward(&self, app: &'static App) -> Result<(), String>;
    /// Is the app's window in front now?
    fn is_foreground(&self, app: &'static App) -> bool;
    /// Press a key combination in the window in front ("Ctrl + D", "Enter").
    fn press(&self, keys: &str) -> Result<(), String>;
    /// Find the app's message box in its window (in front), focus it and confirm the focus moved there
    /// (`au05_composer`). Returns its description for the log, or why not. Sends no keys.
    fn focus_composer(&self, app: &'static App) -> Result<String, String>;
    /// Called (on the desktop's thread) when Windows ends the session; it may block for a few seconds.
    fn set_session_end(&self, _handler: Arc<dyn Fn() + Send + Sync>) {}
}

/// Actions that type into the app's message box: the message box gets the focus first, or nothing is pressed
/// (user report 2026-11-17: with Claude's terminal panel focused, Ctrl + D went into the terminal). Dictate &
/// send does the same before each of its keys.
pub const COMPOSER_ACTIONS: [&str; 4] = ["claudeDictation", "claudeSend", "chatgptDictation", "chatgptSend"];

/// The app whose message box this action types into, if any.
pub fn composer_app(action: &Action) -> Option<&'static App> {
    match action {
        Action::Entry(e) if COMPOSER_ACTIONS.contains(&e.id) || e.effect == DictateSend => e.app.and_then(app),
        _ => None,
    }
}

/// What the page shows when the message box was not found or not focused.
pub fn no_message_box(app: &App) -> String {
    format!("Couldn't find {}'s message box", app.name)
}

/// The message box was found once in this run (its description is logged once, to tune the search).
static COMPOSER_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Focus the message box of `app` (in front), or explain why not. Logs the found element once per run and
/// every failure.
fn composer(d: &dyn Desktop, app: &'static App, log: &dyn Fn(&str)) -> Result<(), String> {
    match d.focus_composer(app) {
        Ok(seen) => {
            if !COMPOSER_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                log(&format!("{} message box: {seen}", app.name));
            }
            Ok(())
        }
        Err(e) => {
            log(&format!("{} message box not found or not focused: {e}", app.name));
            Err(no_message_box(app))
        }
    }
}

/// Run an action. One that types into a message box brings the app forward, focuses its message box and
/// only then presses its keys; if that fails nothing is pressed.
pub fn run_action(d: &dyn Desktop, action: &Action, log: &dyn Fn(&str)) -> Result<(), String> {
    let Some(target) = composer_app(action) else { return d.run(action) };
    let (_, send) = action.plan().ok_or("Nothing to do")?;
    d.bring_forward(target)?;
    composer(d, target, log)?;
    d.press(&send.value)
}

/// Timing for [`dictate_send`].
#[derive(Clone, Copy, Debug)]
pub struct DictateTiming {
    /// How long a start or stop may take to show in the capture state.
    pub confirm: std::time::Duration,
    /// After capture ends: time for the transcription to land in the message box.
    pub settle: std::time::Duration,
    pub poll: std::time::Duration,
}
impl Default for DictateTiming {
    fn default() -> Self {
        use std::time::Duration;
        Self { confirm: Duration::from_millis(1500), settle: Duration::from_millis(800), poll: Duration::from_millis(50) }
    }
}

fn wait_for(d: &dyn Desktop, exe: &str, want: bool, t: DictateTiming) -> Result<bool, String> {
    let end = std::time::Instant::now() + t.confirm;
    loop {
        if d.capture_active(exe)? == want {
            return Ok(true);
        }
        if std::time::Instant::now() >= end {
            return Ok(false);
        }
        std::thread::sleep(t.poll);
    }
}

/// Claude "Dictate & send", decided from real state, never from counting presses (user request 2026-11-17):
/// - not dictating (no active capture session of claude.exe): bring Claude forward, Ctrl + D, and confirm the
///   capture started; if not, report "Dictation didn't start" (no second Ctrl + D);
/// - dictating: bring Claude forward, Ctrl + D, wait until the capture stops and the transcription settles,
///   check Claude is still in front, then Enter.
/// Before each key, Claude's message box gets the focus ([`run_action`]); if it cannot, nothing is pressed. If the capture did not stop or Claude is not in front, no
///   Enter: "Not sent".
/// Returns what happened (for the log; no typed text) or why nothing more was done.
pub fn dictate_send(d: &dyn Desktop, t: DictateTiming, log: &dyn Fn(&str)) -> Result<String, String> {
    let claude = app("claude").ok_or("Claude is not known")?;
    let dictating = d.capture_active(claude.exe)?;
    d.bring_forward(claude).map_err(|e| format!("Nothing done: {e}"))?;
    // Every key goes to the message box only (never the terminal panel).
    composer(d, claude, log).map_err(|e| format!("Nothing done: {e}"))?;
    if !dictating {
        d.press("Ctrl + D")?;
        return if wait_for(d, claude.exe, true, t)? {
            Ok("not dictating: Ctrl + D, dictation started".into())
        } else {
            Err("Dictation didn't start (Ctrl + D sent once, no microphone use seen)".into())
        };
    }
    d.press("Ctrl + D")?;
    if !wait_for(d, claude.exe, false, t)? {
        return Err("Not sent: dictation did not stop".into());
    }
    std::thread::sleep(t.settle);
    if !d.is_foreground(claude) {
        return Err("Not sent: Claude is no longer in front".into());
    }
    composer(d, claude, log).map_err(|e| format!("Not sent: {e}"))?;
    d.press("Enter")?;
    Ok("dictating: Ctrl + D, dictation stopped, Enter sent".into())
}

// ---------- fake ----------

/// Registers nothing, moves no focus, sends no input. Records what would have happened.
#[derive(Default)]
pub struct FakeDesktop {
    handler: Mutex<Option<PressHandler>>,
    claimed: Mutex<Vec<u16>>,
    /// Keys another app "holds": claiming them fails.
    pub busy: Mutex<Vec<u16>>,
    pub ran: Mutex<Vec<String>>,
    /// Tests: `run` panics, to check that a crash in an action is contained.
    pub crash: std::sync::atomic::AtomicBool,
    /// Claude's microphone capture (Dictate & send): on or off, whether Ctrl + D toggles it, whether Claude
    /// can be brought forward, and whether it loses the front after Ctrl + D.
    pub capture: std::sync::atomic::AtomicBool,
    pub ctrl_d_ignored: std::sync::atomic::AtomicBool,
    pub cannot_focus: std::sync::atomic::AtomicBool,
    pub loses_focus: std::sync::atomic::AtomicBool,
    pub claude_in_front: std::sync::atomic::AtomicBool,
    /// The app's message box cannot be found (or focused).
    pub no_composer: std::sync::atomic::AtomicBool,
}
impl FakeDesktop {
    pub fn claimed(&self) -> Vec<u16> {
        self.claimed.lock_or_recover().clone()
    }
    /// The key typed `vk`. Delivered only if claimed, as Windows would.
    pub fn press(&self, vk: u16) -> bool {
        if !self.claimed.lock_or_recover().contains(&vk) {
            return false;
        }
        let h = self.handler.lock_or_recover().clone();
        if let Some(h) = h {
            h(vk);
        }
        true
    }
}
impl Desktop for FakeDesktop {
    fn set_handler(&self, handler: PressHandler) {
        *self.handler.lock_or_recover() = Some(handler);
    }
    fn set_hotkeys(&self, vks: &[u16]) -> BTreeMap<u16, Result<(), String>> {
        let busy = self.busy.lock_or_recover().clone();
        let mut claimed = self.claimed.lock_or_recover();
        claimed.clear();
        vks.iter()
            .map(|&vk| {
                if busy.contains(&vk) {
                    (vk, Err("Another app already uses this key".to_string()))
                } else {
                    claimed.push(vk);
                    (vk, Ok(()))
                }
            })
            .collect()
    }
    fn run(&self, action: &Action) -> Result<(), String> {
        if self.crash.load(std::sync::atomic::Ordering::SeqCst) {
            panic!("test action crash");
        }
        let (app, send) = action.plan().ok_or("Nothing to do")?;
        let what = if send.kind == "media" { format!("media {}", send.value) } else { format!("press {}", send.value) };
        let text = match app {
            Some((exe, _)) => format!("focus {exe}, {what}"),
            None => what,
        };
        self.ran.lock_or_recover().push(text);
        Ok(())
    }
    fn scroll(&self, notches: i32) -> Result<(), String> {
        self.ran.lock_or_recover().push(format!("scroll {notches}"));
        Ok(())
    }
    fn open_link(&self, url: &str) -> Result<(), String> {
        self.ran.lock_or_recover().push(format!("open {url}"));
        Ok(())
    }
    fn capture_active(&self, exe: &str) -> Result<bool, String> {
        Ok(exe == "claude.exe" && self.capture.load(std::sync::atomic::Ordering::SeqCst))
    }
    fn bring_forward(&self, app: &'static App) -> Result<(), String> {
        use std::sync::atomic::Ordering::SeqCst;
        if self.cannot_focus.load(SeqCst) {
            return Err(format!("{} could not be brought to the front", app.title));
        }
        self.claude_in_front.store(true, SeqCst);
        self.ran.lock_or_recover().push(format!("focus {}", app.exe));
        Ok(())
    }
    fn is_foreground(&self, _app: &'static App) -> bool {
        self.claude_in_front.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn focus_composer(&self, app: &'static App) -> Result<String, String> {
        if self.no_composer.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("no message box (fake)".into());
        }
        self.ran.lock_or_recover().push(format!("message box {}", app.exe));
        Ok("Edit \"Message\" (fake)".into())
    }
    fn press(&self, keys: &str) -> Result<(), String> {
        use std::sync::atomic::Ordering::SeqCst;
        self.ran.lock_or_recover().push(format!("press {keys}"));
        if keys == "Ctrl + D" {
            if !self.ctrl_d_ignored.load(SeqCst) {
                self.capture.fetch_xor(true, SeqCst);
            }
            if self.loses_focus.load(SeqCst) {
                self.claude_in_front.store(false, SeqCst);
            }
        }
        Ok(())
    }
}

// ---------- Windows ----------

#[cfg(windows)]
pub use native::WindowsDesktop;
#[cfg(windows)]
pub(crate) use native::process_exe;

#[cfg(windows)]
mod native {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT};
    use windows_sys::Win32::System::Threading::{
        GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        MOD_NOREPEAT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible, PeekMessageW, PostThreadMessageW, SetForegroundWindow, ShowWindow, MSG, PM_NOREMOVE,
        SW_RESTORE, WM_APP, WM_HOTKEY, WM_QUIT,
    };

    const WM_RECONCILE: u32 = WM_APP + 0x51;

    /// The session-end handler for the hidden window (one desktop per app).
    static SESSION_END: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);

    unsafe extern "system" fn session_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: LPARAM) -> LRESULT {
        use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, WM_ENDSESSION, WM_QUERYENDSESSION};
        match msg {
            WM_QUERYENDSESSION => 1, // never block the end of the session
            WM_ENDSESSION => {
                if wparam != 0 {
                    let h = SESSION_END.lock_or_recover().clone();
                    if let Some(h) = h {
                        let _ = super::super::sync::guarded("Vibe Key session end", || h());
                    }
                }
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    /// A hidden top-level window (never shown) so the hotkey thread hears about session end.
    unsafe fn session_window() {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, RegisterClassW, WNDCLASSW, WS_OVERLAPPED};
        let name: Vec<u16> = "UnifiedControlVibeKeySession\0".encode_utf16().collect();
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(session_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: name.as_ptr(),
        };
        unsafe { RegisterClassW(&class) };
        unsafe {
            CreateWindowExW(0, name.as_ptr(), name.as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null())
        };
    }

    struct Shared {
        handler: Mutex<Option<PressHandler>>,
        wanted: Mutex<(Vec<u16>, Option<mpsc::Sender<BTreeMap<u16, Result<(), String>>>>)>,
    }

    /// One thread with a message queue owns every hotkey (thread hotkeys: `RegisterHotKey(NULL, ..)`).
    pub struct WindowsDesktop {
        shared: Arc<Shared>,
        thread_id: u32,
        actions: Mutex<super::super::actions::NativeActions>,
    }

    impl WindowsDesktop {
        /// Starts the hotkey thread. Claims nothing until `set_hotkeys`.
        pub fn start(actions: super::super::actions::NativeActions) -> Result<Arc<Self>, String> {
            let shared = Arc::new(Shared {
                handler: Mutex::new(None),
                wanted: Mutex::new((Vec::new(), None)),
            });
            let (tx, rx) = mpsc::channel();
            let s = shared.clone();
            std::thread::Builder::new()
                .name("uc-au05-hotkeys".into())
                .spawn(move || unsafe { hotkey_thread(s, tx) })
                .map_err(|_| "The hotkey thread could not start")?;
            let thread_id = rx.recv_timeout(Duration::from_secs(2)).map_err(|_| "The hotkey thread did not start")?;
            Ok(Arc::new(Self {
                shared,
                thread_id,
                actions: Mutex::new(actions),
            }))
        }
    }
    impl Drop for WindowsDesktop {
        fn drop(&mut self) {
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        }
    }

    unsafe fn hotkey_thread(shared: Arc<Shared>, ready: mpsc::Sender<u32>) {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        // Create the thread's message queue before anyone posts to it.
        unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
        let _ = ready.send(unsafe { GetCurrentThreadId() });
        unsafe { session_window() };
        let mut claimed: Vec<u16> = Vec::new();
        loop {
            let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if r <= 0 {
                break; // WM_QUIT or error
            }
            match msg.message {
                WM_HOTKEY => {
                    let vk = msg.wParam as u16;
                    let h = shared.handler.lock_or_recover().clone();
                    if let Some(h) = h {
                        let _ = super::super::sync::guarded("Vibe Key hotkey", || h(vk));
                    }
                }
                WM_RECONCILE => {
                    let (wanted, reply) = {
                        let mut w = shared.wanted.lock_or_recover();
                        (w.0.clone(), w.1.take())
                    };
                    for vk in claimed.clone() {
                        if !wanted.contains(&vk) {
                            unsafe { UnregisterHotKey(std::ptr::null_mut(), vk as i32) };
                            claimed.retain(|v| *v != vk);
                        }
                    }
                    let mut results = BTreeMap::new();
                    for &vk in &wanted {
                        if claimed.contains(&vk) {
                            results.insert(vk, Ok(()));
                        } else if unsafe { RegisterHotKey(std::ptr::null_mut(), vk as i32, MOD_NOREPEAT, vk as u32) } != 0 {
                            claimed.push(vk);
                            results.insert(vk, Ok(()));
                        } else {
                            results.insert(vk, Err("Another app already uses this key".to_string()));
                        }
                    }
                    if let Some(reply) = reply {
                        let _ = reply.send(results);
                    }
                }
                _ => unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
                    windows_sys::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
                },
            }
        }
        for vk in claimed {
            unsafe { UnregisterHotKey(std::ptr::null_mut(), vk as i32) };
        }
    }

    impl Desktop for WindowsDesktop {
        fn set_handler(&self, handler: PressHandler) {
            *self.shared.handler.lock_or_recover() = Some(handler);
        }
        fn set_hotkeys(&self, vks: &[u16]) -> BTreeMap<u16, Result<(), String>> {
            let (tx, rx) = mpsc::channel();
            *self.shared.wanted.lock_or_recover() = (vks.to_vec(), Some(tx));
            if unsafe { PostThreadMessageW(self.thread_id, WM_RECONCILE, 0, 0) } == 0 {
                return vks.iter().map(|&v| (v, Err("The hotkey thread is unavailable".into()))).collect();
            }
            rx.recv_timeout(Duration::from_secs(2))
                .unwrap_or_else(|_| vks.iter().map(|&v| (v, Err("The hotkey thread did not answer".into()))).collect())
        }
        fn run(&self, action: &Action) -> Result<(), String> {
            use super::super::actions::ActionSink;
            let (app, send) = action.plan().ok_or("Nothing to do")?;
            if let Some((exe, title)) = app {
                unsafe { bring_forward(exe, title) }?;
                std::thread::sleep(Duration::from_millis(120));
            }
            self.actions.lock_or_recover().execute(&send)
        }
        fn scroll(&self, notches: i32) -> Result<(), String> {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{INPUT_MOUSE, MOUSEEVENTF_WHEEL, MOUSEINPUT};
            let n = notches.clamp(-10, 10);
            if n == 0 {
                return Ok(());
            }
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: (n * 120) as u32, dwFlags: MOUSEEVENTF_WHEEL, time: 0, dwExtraInfo: 0 },
                },
            };
            let sent = unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
            if sent == 1 { Ok(()) } else { Err("Windows did not accept the scroll".into()) }
        }
        fn capture_active(&self, exe: &str) -> Result<bool, String> {
            super::super::audio_capture::capture_active(exe)
        }
        fn set_session_end(&self, handler: Arc<dyn Fn() + Send + Sync>) {
            *SESSION_END.lock_or_recover() = Some(handler);
        }
        fn bring_forward(&self, app: &'static App) -> Result<(), String> {
            unsafe { bring_forward(app.exe, app.title) }
        }
        fn focus_composer(&self, app: &'static App) -> Result<String, String> {
            let hwnd = unsafe { GetForegroundWindow() };
            if hwnd.is_null() || window_exe(hwnd).is_none_or(|e| e != app.exe) {
                return Err(format!("{} is not in front", app.name));
            }
            super::super::au05_composer::focus(hwnd as isize)
        }
        fn is_foreground(&self, app: &'static App) -> bool {
            let hwnd = unsafe { GetForegroundWindow() };
            !hwnd.is_null() && window_exe(hwnd).is_some_and(|e| e == app.exe)
        }
        fn press(&self, keys: &str) -> Result<(), String> {
            use super::super::actions::ActionSink;
            self.actions.lock_or_recover().execute(&PendingAction { kind: "hotkey".into(), value: keys.into() })
        }
        fn open_link(&self, url: &str) -> Result<(), String> {
            use windows_sys::Win32::UI::Shell::ShellExecuteW;
            use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
            if !url.starts_with("claude://claude.ai/epitaxy/local_") || !url.is_ascii() {
                return Err("Not a Claude session link".into());
            }
            let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
            let (verb, file) = (wide("open"), wide(url));
            let r = unsafe {
                ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
            };
            // ShellExecute returns a value above 32 on success.
            if r as isize > 32 { Ok(()) } else { Err("Windows could not open the Claude session link".into()) }
        }
    }

    struct Find {
        exe: &'static str,
        title: &'static str,
        found: HWND,
    }

    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> i32 {
        let find = unsafe { &mut *(lparam as *mut Find) };
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        let mut buf = [0u16; 256];
        let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), 256) };
        if n <= 0 || !String::from_utf16_lossy(&buf[..n as usize]).starts_with(find.title) {
            return 1;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return 1;
        }
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len) } != 0;
        unsafe { CloseHandle(process) };
        if ok {
            let full = String::from_utf16_lossy(&path[..len as usize]).to_ascii_lowercase();
            if full.rsplit(['\\', '/']).next() == Some(find.exe) {
                find.found = hwnd;
                return 0;
            }
        }
        1
    }

    /// The exe file name (lower case) of a window's process.
    fn window_exe(hwnd: HWND) -> Option<String> {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        process_exe(pid)
    }
    pub(crate) fn process_exe(pid: u32) -> Option<String> {
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len) } != 0;
        unsafe { CloseHandle(process) };
        ok.then(|| String::from_utf16_lossy(&path[..len as usize]).to_ascii_lowercase().rsplit(['\\', '/']).next().unwrap_or("").to_string())
    }

    /// Tap Alt, unless Alt is already down. Returns false when the input was not accepted.
    fn tap_alt() -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        if unsafe { GetAsyncKeyState(0x12) } < 0 {
            return true; // Alt is already held: a tap would release the user's Alt.
        }
        let key = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0x12,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [key(false), key(true)];
        let sent = unsafe { SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
        if sent == 1 {
            // Only the Alt press went in: send the release so Alt is never left down.
            let up = [key(true)];
            unsafe { SendInput(1, up.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
        }
        sent == 2
    }

    fn wait_front(hwnd: HWND, ms: u64) -> bool {
        let end = Instant::now() + Duration::from_millis(ms);
        loop {
            if unsafe { GetForegroundWindow() } == hwnd {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Find the app's visible window and bring it to the front. UC has just received WM_HOTKEY, so Windows
    /// normally lets it set the foreground window: try that first. Only if that fails, tap Alt (the
    /// prototype's method) and try once more.
    unsafe fn bring_forward(exe: &'static str, title: &'static str) -> Result<(), String> {
        let mut find = Find {
            exe,
            title,
            found: std::ptr::null_mut(),
        };
        unsafe { EnumWindows(Some(each), &mut find as *mut Find as LPARAM) };
        let hwnd = find.found;
        if hwnd.is_null() {
            return Err(format!("The {title} app is not open"));
        }
        if unsafe { GetForegroundWindow() } == hwnd {
            return Ok(());
        }
        if unsafe { IsIconic(hwnd) } != 0 {
            unsafe { ShowWindow(hwnd, SW_RESTORE) };
        }
        unsafe { SetForegroundWindow(hwnd) };
        if wait_front(hwnd, 300) {
            return Ok(());
        }
        if !tap_alt() {
            return Err("Windows did not accept the key input".into());
        }
        unsafe { SetForegroundWindow(hwnd) };
        if wait_front(hwnd, 600) {
            return Ok(());
        }
        Err(format!("{title} could not be brought to the front"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_parse_and_plan() {
        assert_eq!(Action::from_setting("claudeDictation", "", ""), Some(Action::Entry(entry("claudeDictation").unwrap())));
        assert_eq!(Action::from_setting("hotkey", "", ""), Some(Action::Nothing));
        assert_eq!(Action::from_setting("launch", "x", ""), None);
        let (target, send) = Action::from_setting("claudeDictation", "", "").unwrap().plan().unwrap();
        assert_eq!(target, Some(("claude.exe", "Claude")));
        assert_eq!((send.kind.as_str(), send.value.as_str()), ("hotkey", "Ctrl + D"));
        assert!(validate("hotkey", "Ctrl + Enter", "").is_ok());
        assert!(validate("hotkey", "Ctrl + Enter", "notepad").is_err());
        assert!(validate("hotkey", "Ctrl", "").is_err());
        assert!(validate("shell", "", "").is_err());
        assert_eq!(Action::from_setting("quickMenu", "", ""), Some(Action::QuickMenu));
        assert_eq!(Action::QuickMenu.plan(), None);
        let target = Action::from_setting("targetDictation", "", "").unwrap();
        assert_eq!(target.plan(), None, "a target action must be resolved first");
        assert_eq!(target.clone().resolve("claude"), Action::Entry(entry("claudeDictation").unwrap()));
        assert_eq!(target.resolve("chatgpt"), Action::Entry(entry("chatgptDictation").unwrap()));
        // Every catalogue entry is a valid plan for the reviewed senders.
        for e in CATALOGUE.iter().filter(|e| e.effect != DictateSend) {
            let (_, send) = Action::Entry(e).resolve("chatgpt").plan().unwrap();
            assert!(Action::Entry(e).resolve("claude").plan().is_some(), "{}", e.id);
            assert!(super::super::actions::plan_action(&send).is_ok(), "{}", e.id);
            assert!(e.app.is_none_or(|a| app(a).is_some()), "{}", e.id);
        }
    }
    /// The offline layer offers only plain keystrokes the key can store.
    #[test]
    fn only_storable_keystrokes_are_offered_for_the_offline_layer() {
        assert_eq!(offline_keys(entry("claudeDictation").unwrap()), Some("Ctrl + D"));
        assert_eq!(offline_keys(entry("claudeSend").unwrap()), Some("Enter"));
        for id in ["claudeDictateSend", "targetDictation", "playPause", "volumeUp"] {
            assert_eq!(offline_keys(entry(id).unwrap()), None, "{id}");
        }
        for e in CATALOGUE {
            if let Some(k) = offline_keys(e) {
                assert!(ulanzi_au05::protocol::KeyTyping::parse(k).is_ok(), "{}", e.id);
            }
        }
        assert!(catalogue_json()["actions"].as_array().unwrap().iter().any(|a| a["offlineKeys"] == "Ctrl + D"));
    }
    /// Dictate & send decides from the capture state: start, stop and send, a missed start (no second Ctrl + D),
    /// a stop that does not end the capture, and Claude losing the front: no Enter in the last three.
    #[test]
    fn dictate_and_send_follows_the_real_capture_state() {
        use std::sync::atomic::Ordering::SeqCst;
        use std::time::Duration;
        let t = DictateTiming { confirm: Duration::from_millis(60), settle: Duration::from_millis(5), poll: Duration::from_millis(5) };
        let run = |setup: &dyn Fn(&FakeDesktop)| {
            let d = FakeDesktop::default();
            setup(&d);
            let r = dictate_send(&d, t, &|_| {});
            let ran = d.ran.lock_or_recover().clone();
            (r, ran, d.capture.load(SeqCst))
        };
        let (r, ran, cap) = run(&|_| {});
        assert!(r.unwrap().contains("dictation started"));
        assert_eq!(ran, vec!["focus claude.exe", "message box claude.exe", "press Ctrl + D"]);
        assert!(cap);
        let (r, ran, cap) = run(&|d| d.capture.store(true, SeqCst));
        assert!(r.unwrap().contains("Enter sent"));
        assert_eq!(ran, vec!["focus claude.exe", "message box claude.exe", "press Ctrl + D", "message box claude.exe", "press Enter"]);
        assert!(!cap);
        let (r, ran, _) = run(&|d| d.ctrl_d_ignored.store(true, SeqCst));
        assert!(r.unwrap_err().contains("didn't start"));
        assert_eq!(ran.iter().filter(|x| *x == "press Ctrl + D").count(), 1, "never a blind second Ctrl + D");
        let (r, ran, _) = run(&|d| { d.capture.store(true, SeqCst); d.ctrl_d_ignored.store(true, SeqCst); });
        assert!(r.unwrap_err().starts_with("Not sent"));
        assert!(!ran.contains(&"press Enter".to_string()));
        let (r, ran, _) = run(&|d| { d.capture.store(true, SeqCst); d.loses_focus.store(true, SeqCst); });
        assert!(r.unwrap_err().contains("no longer in front"));
        assert!(!ran.contains(&"press Enter".to_string()));
        let (r, ran, _) = run(&|d| d.cannot_focus.store(true, SeqCst));
        assert!(r.unwrap_err().starts_with("Nothing done"));
        assert!(ran.is_empty(), "no key when Claude cannot come forward");
        // Claude's message box cannot be found (the terminal panel has the focus): no key at all.
        let (r, ran, _) = run(&|d| d.no_composer.store(true, SeqCst));
        assert_eq!(r.unwrap_err(), "Nothing done: Couldn't find Claude's message box");
        assert!(ran.iter().all(|x| !x.starts_with("press")), "{ran:?}");
    }

    /// User report 2026-11-17: Start dictation / Send go to the message box only. If it cannot be found or
    /// focused, nothing is pressed and the page says so; the log says why.
    #[test]
    fn message_box_actions_press_nothing_without_the_message_box() {
        use std::sync::atomic::Ordering::SeqCst;
        let logs = Mutex::new(Vec::<String>::new());
        let log = |l: &str| logs.lock_or_recover().push(l.to_string());
        for id in COMPOSER_ACTIONS {
            let d = FakeDesktop::default();
            let action = Action::from_setting(id, "", "").unwrap();
            let target = composer_app(&action).unwrap();
            run_action(&d, &action, &log).unwrap();
            let (_, send) = action.plan().unwrap();
            assert_eq!(
                *d.ran.lock_or_recover(),
                vec![format!("focus {}", target.exe), format!("message box {}", target.exe), format!("press {}", send.value)],
                "{id}"
            );
            let d = FakeDesktop::default();
            d.no_composer.store(true, SeqCst);
            assert_eq!(run_action(&d, &action, &log).unwrap_err(), no_message_box(target), "{id}");
            assert!(d.ran.lock_or_recover().iter().all(|x| !x.starts_with("press")), "{id}: nothing pressed");
        }
        assert!(logs.lock_or_recover().iter().any(|l| l.contains("message box not found or not focused")));
        // Target-app actions resolve first; actions that do not type into a message box are unchanged.
        assert_eq!(composer_app(&Action::from_setting("targetDictation", "", "").unwrap().resolve("claude")), app("claude"));
        assert_eq!(composer_app(&Action::from_setting("claudeNewSession", "", "").unwrap()), None);
        let d = FakeDesktop::default();
        d.no_composer.store(true, SeqCst);
        run_action(&d, &Action::from_setting("volumeUp", "", "").unwrap(), &log).unwrap();
        assert_eq!(*d.ran.lock_or_recover(), vec!["media Volume up".to_string()]);
    }

    #[test]
    fn the_fake_delivers_only_claimed_keys_and_records_actions() {
        let d = FakeDesktop::default();
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        d.set_handler(Arc::new(move |vk| g.lock_or_recover().push(vk)));
        *d.busy.lock_or_recover() = vec![VK_F21];
        let r = d.set_hotkeys(&[VK_F22, VK_F21]);
        assert!(r[&VK_F22].is_ok() && r[&VK_F21].is_err());
        assert!(d.press(VK_F22));
        assert!(!d.press(VK_F20), "an unclaimed key is not delivered");
        assert_eq!(*got.lock_or_recover(), vec![VK_F22]);
        d.set_hotkeys(&[]);
        assert!(!d.press(VK_F22), "released");
        d.run(&Action::from_setting("claudeDictation", "", "").unwrap()).unwrap();
        d.run(&Action::from_setting("volumeUp", "", "").unwrap()).unwrap();
        assert_eq!(
            *d.ran.lock_or_recover(),
            vec!["focus claude.exe, press Ctrl + D".to_string(), "media Volume up".to_string()]
        );
    }
}
