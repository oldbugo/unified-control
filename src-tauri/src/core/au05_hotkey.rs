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

/// The action UC runs when a key's own code arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    /// Bring the Claude desktop app forward, then Ctrl + D (dictation, user-confirmed 2026-11-16).
    ClaudeDictation,
    /// Bring the ChatGPT desktop app forward, then Ctrl + Shift + D (reported, not confirmed).
    ChatGptDictation,
    /// Enter in the focused app (send / confirm).
    Enter,
    /// A custom key combination in the focused app, such as "Ctrl + Enter".
    Hotkey(String),
}
pub const ACTION_KINDS: [&str; 5] = ["none", "claudeDictation", "chatgptDictation", "enter", "hotkey"];

impl Action {
    pub fn from_setting(kind: &str, value: &str) -> Option<Self> {
        Some(match kind {
            "none" => Action::Nothing,
            "claudeDictation" => Action::ClaudeDictation,
            "chatgptDictation" => Action::ChatGptDictation,
            "enter" => Action::Enter,
            "hotkey" if !value.trim().is_empty() => Action::Hotkey(value.to_string()),
            // A custom hotkey that is still being typed does nothing.
            "hotkey" => Action::Nothing,
            _ => return None,
        })
    }
    pub fn label(&self) -> String {
        match self {
            Action::Nothing => "nothing".into(),
            Action::ClaudeDictation => "dictation in Claude".into(),
            Action::ChatGptDictation => "dictation in ChatGPT".into(),
            Action::Enter => "Enter".into(),
            Action::Hotkey(v) => v.clone(),
        }
    }
    /// The app to bring forward first (exe file name, window title prefix), and the keys to press.
    pub fn plan(&self) -> Option<(Option<(&'static str, &'static str)>, String)> {
        match self {
            Action::Nothing => None,
            Action::ClaudeDictation => Some((Some(("claude.exe", "Claude")), "Ctrl + D".into())),
            Action::ChatGptDictation => Some((Some(("chatgpt.exe", "ChatGPT")), "Ctrl + Shift + D".into())),
            Action::Enter => Some((None, "Enter".into())),
            Action::Hotkey(v) => Some((None, v.clone())),
        }
    }
}

/// Check a saved action before it is stored.
pub fn validate(kind: &str, value: &str) -> Result<(), String> {
    match Action::from_setting(kind, value).ok_or("Choose a supported action")? {
        Action::Hotkey(v) => super::actions::plan_action(&PendingAction {
            kind: "hotkey".into(),
            value: v,
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
        let (app, keys) = action.plan().ok_or("Nothing to do")?;
        let text = match app {
            Some((exe, _)) => format!("focus {exe}, press {keys}"),
            None => format!("press {keys}"),
        };
        self.ran.lock_or_recover().push(text);
        Ok(())
    }
}

// ---------- Windows ----------

#[cfg(windows)]
pub use native::WindowsDesktop;

#[cfg(windows)]
mod native {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
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
                _ => {}
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
            let (app, keys) = action.plan().ok_or("Nothing to do")?;
            if let Some((exe, title)) = app {
                unsafe { bring_forward(exe, title) }?;
                std::thread::sleep(Duration::from_millis(120));
            }
            self.actions.lock_or_recover().execute(&PendingAction {
                kind: "hotkey".into(),
                value: keys,
            })
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
        assert_eq!(Action::from_setting("claudeDictation", ""), Some(Action::ClaudeDictation));
        assert_eq!(Action::from_setting("hotkey", ""), Some(Action::Nothing));
        assert_eq!(Action::from_setting("launch", "x"), None);
        assert_eq!(
            Action::ClaudeDictation.plan(),
            Some((Some(("claude.exe", "Claude")), "Ctrl + D".to_string()))
        );
        assert!(validate("hotkey", "Ctrl + Enter").is_ok());
        assert!(validate("hotkey", "Ctrl").is_err());
        assert!(validate("shell", "").is_err());
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
        d.run(&Action::ClaudeDictation).unwrap();
        assert_eq!(*d.ran.lock_or_recover(), vec!["focus claude.exe, press Ctrl + D".to_string()]);
    }
}
