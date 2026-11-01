//! Reviewed first-party key actions. No action runs when a profile is read or edited.
//! The dock service must call this only for an enabled, real physical key press.
//! Tests use `ActionPlatform` fakes; `NativeActions(fake=true)` refuses every effect.
use super::dock_runtime::PendingAction;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MAX_VALUE: usize = 4096;
const CTRL: u16 = 0x11;
const SHIFT: u16 = 0x10;
const ALT: u16 = 0x12;
const WIN: u16 = 0x5b;

pub trait ActionSink: Send {
    fn execute(&mut self, action: &PendingAction) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub key: u16,
    pub unicode: bool,
    pub extended: bool,
    pub up: bool,
}

impl Stroke {
    fn virtual_key(key: u16, up: bool) -> Self {
        Self {
            key,
            unicode: false,
            extended: is_extended(key),
            up,
        }
    }
    fn release(&self) -> Self {
        Self {
            up: true,
            ..self.clone()
        }
    }
    fn same_key(&self, other: &Self) -> bool {
        self.key == other.key && self.unicode == other.unicode
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LaunchSpec {
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorSpec {
    pub targets: Vec<String>,
    #[serde(default = "toggle")]
    pub mode: String,
}
fn toggle() -> String {
    "toggle".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonitorInfo {
    pub key: String,
    pub label: String,
}

/// Read-only discovery; never called for simulated devices. It starts the same
/// trusted helper in `list` mode, which does not apply a topology or audio guard.
pub fn list_monitors(resource_dir: &Path) -> Result<Vec<MonitorInfo>, String> {
    #[cfg(windows)]
    {
        let result = monitor_helper(resource_dir, &["-Action".into(), "list".into()])?;
        parse_monitor_list(&result)
    }
    #[cfg(not(windows))]
    {
        let _ = resource_dir;
        Err("Monitor discovery requires Windows".into())
    }
}

fn parse_monitor_list(result: &serde_json::Value) -> Result<Vec<MonitorInfo>, String> {
    if result["status"].as_str() != Some("listed") {
        return Err("Monitor discovery returned an invalid result".into());
    }
    // PowerShell unwraps a one-item pipeline and emits null for no items.
    // Normalize only those documented shapes before validating each display.
    let displays: Vec<&serde_json::Value> = match result.get("displays") {
        Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::Object(_)) => vec![&result["displays"]],
        Some(serde_json::Value::Array(items)) => items.iter().collect(),
        _ => return Err("Monitor discovery returned no display list".into()),
    };
    if displays.len() > 128 {
        return Err("Monitor discovery exceeded its display limit".into());
    }
    let mut seen = HashSet::new();
    let mut monitors = Vec::new();
    for (index, display) in displays.iter().enumerate() {
        let key = display["key"]
            .as_str()
            .ok_or("A discovered monitor has no stable key")?;
        if key.is_empty()
            || key.len() > 512
            || key.starts_with('-')
            || key.chars().any(|c| c.is_control() || c == ',')
        {
            return Err("A discovered monitor has an invalid stable key".into());
        }
        if !seen.insert(key.to_ascii_lowercase()) {
            continue;
        }
        let label = display["friendlyName"]
            .as_str()
            .filter(|name| {
                !name.trim().is_empty()
                    && name.chars().count() <= 256
                    && !name.chars().any(char::is_control)
            })
            .map(String::from)
            .unwrap_or_else(|| format!("Display {}", index + 1));
        monitors.push(MonitorInfo {
            key: key.into(),
            label,
        });
    }
    Ok(monitors)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionPlan {
    None,
    Hotkey { modifiers: Vec<u16>, key: u16 },
    Media(u16),
    Text(String),
    Launch(LaunchSpec),
    Url(String),
    Monitor(MonitorSpec),
}

/// Narrow seam: fakes inspect requested effects without sending input, spawning
/// processes, changing displays or even reading the Windows keyboard state.
pub trait ActionPlatform {
    fn held_modifiers(&mut self) -> Result<Vec<u16>, String>;
    /// Number of events accepted, following SendInput's partial-write contract.
    fn send_input(&mut self, events: &[Stroke]) -> Result<usize, String>;
    fn launch(&mut self, spec: &LaunchSpec) -> Result<(), String>;
    fn open_url(&mut self, url: &str) -> Result<(), String>;
    fn monitor(&mut self, spec: &MonitorSpec) -> Result<(), String>;
}

/// Parse data only. No expansion of environment variables, command line,
/// shell metacharacters, URI schemes or scripts is performed.
pub fn plan_action(action: &PendingAction) -> Result<ActionPlan, String> {
    if action.value.len() > MAX_VALUE || action.value.contains('\0') {
        return Err("Action data must be at most 4096 bytes and contain no null character".into());
    }
    match action.kind.as_str() {
        "none" => Ok(ActionPlan::None),
        "hotkey" => parse_hotkey(&action.value),
        "media" => {
            let key = match action.value.trim().to_ascii_lowercase().as_str() {
                "play / pause" | "play/pause" => 0xb3,
                "next track" => 0xb0,
                "previous track" => 0xb1,
                "volume up" => 0xaf,
                "volume down" => 0xae,
                "mute" => 0xad,
                _ => return Err("Choose a supported media control".into()),
            };
            Ok(ActionPlan::Media(key))
        }
        "text" => {
            if action.value.is_empty() {
                return Err("Enter text for this key".into());
            }
            Ok(ActionPlan::Text(action.value.clone()))
        }
        "launch" => Ok(ActionPlan::Launch(parse_launch(&action.value)?)),
        "url" => {
            validate_url(&action.value)?;
            Ok(ActionPlan::Url(action.value.clone()))
        }
        "monitor" => Ok(ActionPlan::Monitor(parse_monitor(&action.value)?)),
        "page" | "folder" | "back" => Err("Page navigation belongs to the dock runtime".into()),
        _ => Err("Unsupported key action".into()),
    }
}

pub fn execute_with(
    action: &PendingAction,
    platform: &mut impl ActionPlatform,
) -> Result<(), String> {
    match plan_action(action)? {
        ActionPlan::None => Ok(()),
        ActionPlan::Hotkey { modifiers, key } => {
            let held: Vec<_> = platform
                .held_modifiers()?
                .into_iter()
                .map(|key| if key == 0x5c { WIN } else { key })
                .collect();
            if held.iter().any(|key| !modifiers.contains(key)) {
                return Err("Release the other keyboard modifiers before using this hotkey".into());
            }
            // Do not release a modifier that the user was already holding.
            let injected: Vec<_> = modifiers
                .into_iter()
                .filter(|m| !held.contains(m))
                .collect();
            let mut events: Vec<_> = injected
                .iter()
                .map(|&m| Stroke::virtual_key(m, false))
                .collect();
            events.extend([
                Stroke::virtual_key(key, false),
                Stroke::virtual_key(key, true),
            ]);
            events.extend(injected.iter().rev().map(|&m| Stroke::virtual_key(m, true)));
            send_batch(platform, &events)
        }
        ActionPlan::Media(key) => send_batch(
            platform,
            &[
                Stroke::virtual_key(key, false),
                Stroke::virtual_key(key, true),
            ],
        ),
        ActionPlan::Text(text) => {
            if !platform.held_modifiers()?.is_empty() {
                return Err(
                    "Release the keyboard modifiers before typing text from this key".into(),
                );
            }
            let events: Vec<_> = text
                .encode_utf16()
                .flat_map(|key| {
                    [
                        Stroke {
                            key,
                            unicode: true,
                            extended: false,
                            up: false,
                        },
                        Stroke {
                            key,
                            unicode: true,
                            extended: false,
                            up: true,
                        },
                    ]
                })
                .collect();
            send_batch(platform, &events)
        }
        ActionPlan::Launch(spec) => platform.launch(&spec),
        ActionPlan::Url(url) => platform.open_url(&url),
        ActionPlan::Monitor(spec) => platform.monitor(&spec),
    }
}

fn send_batch(platform: &mut impl ActionPlatform, events: &[Stroke]) -> Result<(), String> {
    let accepted = platform.send_input(events)?;
    if accepted == events.len() {
        return Ok(());
    }
    if accepted > events.len() {
        return Err("Keyboard adapter returned an invalid event count".into());
    }
    let mut down: Vec<Stroke> = Vec::new();
    for event in &events[..accepted] {
        if !event.up {
            down.push(event.clone());
        } else if let Some(index) = down.iter().rposition(|old| old.same_key(event)) {
            down.remove(index);
        }
    }
    let release: Vec<_> = down.iter().rev().map(Stroke::release).collect();
    if !release.is_empty() {
        match platform.send_input(&release) {
            Ok(count) if count == release.len() => {}
            _ => {
                return Err(
                    "Keyboard input was incomplete and injected keys could not all be released"
                        .into(),
                )
            }
        }
    }
    Err("Windows accepted only part of the keyboard action; injected keys were released".into())
}

fn parse_hotkey(value: &str) -> Result<ActionPlan, String> {
    let parts: Vec<_> = value.split('+').map(str::trim).collect();
    if parts.is_empty() || parts.len() > 5 || parts.iter().any(|p| p.is_empty()) {
        return Err(
            "Enter a key combination such as Ctrl + Shift + M; use Plus for the plus key".into(),
        );
    }
    let mut modifiers = Vec::new();
    let mut key = None;
    for part in parts {
        let name = part.to_ascii_uppercase();
        let modifier = match name.as_str() {
            "CTRL" | "CONTROL" => Some(CTRL),
            "SHIFT" => Some(SHIFT),
            "ALT" => Some(ALT),
            "WIN" | "WINDOWS" | "SUPER" => Some(WIN),
            _ => None,
        };
        if let Some(modifier) = modifier {
            if modifiers.contains(&modifier) {
                return Err("A hotkey modifier is repeated".into());
            }
            modifiers.push(modifier);
        } else {
            if key.is_some() {
                return Err("A hotkey has one main key and optional modifiers".into());
            }
            key = Some(key_code(&name).ok_or("Unsupported hotkey key name")?);
        }
    }
    let key = key.ok_or("Choose a main key as well as modifiers")?;
    if key == 0x2e && modifiers.contains(&CTRL) && modifiers.contains(&ALT) {
        return Err("Windows does not permit synthetic Ctrl + Alt + Delete".into());
    }
    Ok(ActionPlan::Hotkey { modifiers, key })
}

fn key_code(name: &str) -> Option<u16> {
    if name.len() == 1 {
        let c = name.as_bytes()[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u16);
        }
    }
    if let Some(number) = name
        .strip_prefix('F')
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|n| (1..=24).contains(n))
    {
        return Some(0x70 + number - 1);
    }
    if let Some(number) = name
        .strip_prefix("NUMPAD")
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|n| *n <= 9)
    {
        return Some(0x60 + number);
    }
    Some(match name {
        "ENTER" | "RETURN" => 0x0d,
        "SPACE" => 0x20,
        "TAB" => 0x09,
        "ESC" | "ESCAPE" => 0x1b,
        "BACKSPACE" => 0x08,
        "DELETE" | "DEL" => 0x2e,
        "INSERT" | "INS" => 0x2d,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" | "PGUP" => 0x21,
        "PAGEDOWN" | "PGDN" => 0x22,
        "LEFT" => 0x25,
        "UP" => 0x26,
        "RIGHT" => 0x27,
        "DOWN" => 0x28,
        "PRINTSCREEN" | "PRTSC" => 0x2c,
        "PAUSE" => 0x13,
        "CAPSLOCK" => 0x14,
        "NUMLOCK" => 0x90,
        "SCROLLLOCK" => 0x91,
        "MENU" | "APPS" => 0x5d,
        "PLUS" | "EQUAL" | "=" => 0xbb,
        "MINUS" | "-" => 0xbd,
        "COMMA" | "," => 0xbc,
        "PERIOD" | "." => 0xbe,
        "SLASH" | "/" => 0xbf,
        "SEMICOLON" | ";" => 0xba,
        "QUOTE" | "'" => 0xde,
        "LEFTBRACKET" | "[" => 0xdb,
        "RIGHTBRACKET" | "]" => 0xdd,
        "BACKSLASH" | "\\" => 0xdc,
        "BACKTICK" | "`" => 0xc0,
        "MULTIPLY" => 0x6a,
        "ADD" => 0x6b,
        "SUBTRACT" => 0x6d,
        "DECIMAL" => 0x6e,
        "DIVIDE" => 0x6f,
        _ => return None,
    })
}
fn is_extended(key: u16) -> bool {
    matches!(key, 0x21..=0x28 | 0x2c..=0x2e | 0x5b..=0x5d | 0x6f | 0x90 | 0xad..=0xb3)
}

fn parse_launch(value: &str) -> Result<LaunchSpec, String> {
    let trimmed = value.trim();
    let spec: LaunchSpec = if trimmed.starts_with('{') {
        serde_json::from_str(trimmed).map_err(|_| "Use a path or launch JSON with path and args")?
    } else {
        LaunchSpec {
            path: trimmed.into(),
            args: Vec::new(),
        }
    };
    if !absolute_windows_path(&spec.path)
        || spec
            .path
            .chars()
            .any(|c| c.is_control() || ['"', '<', '>', '|', '?', '*'].contains(&c))
        || spec.path.len() > 2048
    {
        return Err("Choose an absolute app, file or folder path".into());
    }
    let extension = Path::new(&spec.path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if [
        "bat", "cmd", "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "py", "pyw",
        "rb", "pl", "sh",
    ]
    .contains(&extension.as_str())
    {
        return Err("Script files are not supported as launch actions".into());
    }
    if spec.args.len() > 32
        || spec.args.iter().any(|a| a.contains('\0') || a.len() > 2048)
        || spec.args.iter().map(String::len).sum::<usize>() > 4096
    {
        return Err("Launch arguments exceed their size limit".into());
    }
    if !spec.args.is_empty() && !["exe", "com"].contains(&extension.as_str()) {
        return Err("Arguments are supported only for executable apps".into());
    }
    Ok(spec)
}
fn absolute_windows_path(value: &str) -> bool {
    let b = value.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || (value.starts_with("\\\\")
            && !value.starts_with("\\\\?\\")
            && !value.starts_with("\\\\.\\")
            && value[2..].split('\\').filter(|s| !s.is_empty()).count() >= 2)
}
fn validate_url(value: &str) -> Result<(), String> {
    let lower = value.to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &value[8..]
    } else if lower.starts_with("http://") {
        &value[7..]
    } else {
        return Err("Only http and https URLs are supported".into());
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty()
        || host.starts_with(':')
        || host.contains('@')
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || ['"', '<', '>', '\\'].contains(&c))
    {
        return Err("Enter a valid http or https URL".into());
    }
    Ok(())
}
fn parse_monitor(value: &str) -> Result<MonitorSpec, String> {
    let mut spec: MonitorSpec = serde_json::from_str(value)
        .map_err(|_| "Configure Monitor Toggle with targets JSON first")?;
    if !["toggle", "enable", "disable"].contains(&spec.mode.as_str()) {
        return Err("Unsupported monitor action mode".into());
    }
    if spec.targets.is_empty() || spec.targets.len() > 8 {
        return Err("Choose 1 to 8 monitor targets".into());
    }
    let mut unique = HashSet::new();
    for target in &mut spec.targets {
        *target = target.trim().to_string();
        if target.is_empty()
            || target.len() > 512
            || target.starts_with('-')
            || target.chars().any(|c| c.is_control() || c == ',')
            || !unique.insert(target.to_ascii_lowercase())
        {
            return Err("Monitor target keys are invalid or repeated".into());
        }
    }
    // A target group owns one stable layout snapshot, independent of display order.
    spec.targets.sort_by_key(|s| s.to_ascii_lowercase());
    Ok(spec)
}

pub struct NativeActions {
    resource_dir: PathBuf,
    app_home: PathBuf,
    fake: bool,
}
impl NativeActions {
    /// Resource directory contains `monitor/WindowsDisplayControl.ps1` and its audio helper.
    /// Construction never reads devices, keyboard state, files or accounts.
    pub fn new(resource_dir: PathBuf, app_home: PathBuf, fake: bool) -> Self {
        Self {
            resource_dir,
            app_home,
            fake,
        }
    }
}
impl ActionSink for NativeActions {
    fn execute(&mut self, action: &PendingAction) -> Result<(), String> {
        if self.fake {
            return Err("Native actions are disabled for simulated devices".into());
        }
        #[cfg(windows)]
        {
            static ACTION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
            let _guard = ACTION_LOCK
                .lock()
                .map_err(|_| "Action executor lock is unavailable")?;
            execute_with(
                action,
                &mut WindowsPlatform {
                    resource_dir: &self.resource_dir,
                    app_home: &self.app_home,
                },
            )
        }
        #[cfg(not(windows))]
        {
            let _ = (&self.resource_dir, &self.app_home, action);
            Err("Native actions require Windows".into())
        }
    }
}

#[cfg(windows)]
struct WindowsPlatform<'a> {
    resource_dir: &'a Path,
    app_home: &'a Path,
}

#[cfg(windows)]
impl ActionPlatform for WindowsPlatform<'_> {
    fn held_modifiers(&mut self) -> Result<Vec<u16>, String> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        Ok([CTRL, SHIFT, ALT, WIN, 0x5c]
            .into_iter()
            .filter(|&k| unsafe { GetAsyncKeyState(k as i32) } < 0)
            .collect())
    }
    fn send_input(&mut self, events: &[Stroke]) -> Result<usize, String> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        };
        let inputs: Vec<_> = events
            .iter()
            .map(|e| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: if e.unicode { 0 } else { e.key },
                        wScan: if e.unicode { e.key } else { 0 },
                        dwFlags: (if e.up { KEYEVENTF_KEYUP } else { 0 })
                            | (if e.extended { KEYEVENTF_EXTENDEDKEY } else { 0 })
                            | (if e.unicode { KEYEVENTF_UNICODE } else { 0 }),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            })
            .collect();
        let accepted = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        } as usize;
        Ok(accepted)
    }
    fn launch(&mut self, spec: &LaunchSpec) -> Result<(), String> {
        let metadata = std::fs::metadata(&spec.path)
            .map_err(|_| "The selected app, file or folder is unavailable")?;
        let extension = Path::new(&spec.path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if metadata.is_file() && ["exe", "com"].contains(&extension.as_str()) {
            // Direct CreateProcess through std; no shell and no UAC elevation verb.
            std::process::Command::new(&spec.path)
                .args(&spec.args)
                .spawn()
                .map_err(|_| "The selected app could not be started as the current user")?;
            Ok(())
        } else {
            if !spec.args.is_empty() {
                return Err("Arguments are supported only for executable apps".into());
            }
            shell_open(&spec.path)
        }
    }
    fn open_url(&mut self, url: &str) -> Result<(), String> {
        shell_open(url)
    }
    fn monitor(&mut self, spec: &MonitorSpec) -> Result<(), String> {
        let state = self.app_home.join("monitor");
        std::fs::create_dir_all(&state).map_err(|_| "Monitor snapshot folder is unavailable")?;
        let snapshot = state.join(format!("group-{:016x}.bin", monitor_group_key(spec)));
        // Wait for the original transaction, including its audio finally block.
        // Never kill the helper midway through display/audio preservation.
        let result = monitor_helper(
            self.resource_dir,
            &[
                "-Action".into(),
                spec.mode.clone().into(),
                "-TargetKeys".into(),
                spec.targets.join(",").into(),
                "-SnapshotPath".into(),
                snapshot.into_os_string(),
            ],
        )?;
        if result["matchedTargetCount"].as_u64().unwrap_or(0) == 0
            || !matches!(result["status"].as_str(), Some("enabled" | "disabled"))
        {
            return Err("The selected monitor targets were not fully changed".into());
        }
        if result["audioPreservation"]["Warnings"]
            .as_array()
            .is_some_and(|warnings| !warnings.is_empty())
        {
            return Err("Monitor changed, but audio preservation reported a warning".into());
        }
        Ok(())
    }
}

#[cfg(windows)]
fn monitor_helper(
    resource_dir: &Path,
    args: &[std::ffi::OsString],
) -> Result<serde_json::Value, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    static MONITOR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = MONITOR_LOCK
        .lock()
        .map_err(|_| "Monitor action lock is unavailable")?;
    let script = resource_dir.join("monitor/WindowsDisplayControl.ps1");
    let audio = resource_dir.join("monitor/WindowsAudioDefaults.cs");
    if !script.is_file() || !audio.is_file() {
        return Err("Monitor Toggle resources are unavailable".into());
    }
    let system = std::env::var_os("SystemRoot").ok_or("Windows system folder is unavailable")?;
    let powershell = PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let output = Command::new(powershell)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| "Monitor Toggle helper could not be started")?;
    if !output.status.success() {
        return Err(
            "Monitor operation failed; check its configured targets and saved layout".into(),
        );
    }
    if output.stdout.len() > 1024 * 1024 {
        return Err("Monitor helper result exceeds its size limit".into());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "Monitor helper returned an invalid result".into())
}

#[cfg(windows)]
fn shell_open(target: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
    };
    let _com = shell_com::Apartment::new()?;
    let target: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: target.as_ptr(),
        nShow: 1,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err("The selected file, folder or URL could not be opened".into());
    }
    Ok(())
}

#[cfg(windows)]
mod shell_com {
    // These two documented HRESULT/void ABI functions need no additional crate
    // feature. The action worker initializes its own apartment for shell handlers.
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut core::ffi::c_void, flags: u32) -> i32;
        fn CoUninitialize();
    }
    pub struct Apartment;
    impl Apartment {
        pub fn new() -> Result<Self, String> {
            // COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE.
            let result = unsafe { CoInitializeEx(std::ptr::null_mut(), 0x2 | 0x4) };
            if result < 0 {
                return Err(
                    "The shell action worker could not initialize its Windows apartment".into(),
                );
            }
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }
}

fn monitor_group_key(spec: &MonitorSpec) -> u64 {
    // Defined FNV-1a, not a randomized hasher: stable snapshot names across runs.
    let mut hash = 0xcbf29ce484222325u64;
    for target in &spec.targets {
        for byte in target.to_ascii_lowercase().bytes().chain(Some(0)) {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    #[ignore = "Read-only Windows monitor discovery; run explicitly with user present"]
    fn windows_monitor_discovery_read_only() {
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let monitors = list_monitors(&resources).expect("Read-only monitor discovery failed");
        assert!(!monitors.is_empty(), "No active displays were discovered");
        println!("Read-only discovery found {} displays", monitors.len());
    }
    #[derive(Default)]
    struct Fake {
        held: Vec<u16>,
        batches: Vec<Vec<Stroke>>,
        responses: Vec<usize>,
        launches: Vec<LaunchSpec>,
        urls: Vec<String>,
        monitors: Vec<MonitorSpec>,
        fail_launch: bool,
    }
    impl ActionPlatform for Fake {
        fn held_modifiers(&mut self) -> Result<Vec<u16>, String> {
            Ok(self.held.clone())
        }
        fn send_input(&mut self, events: &[Stroke]) -> Result<usize, String> {
            self.batches.push(events.to_vec());
            Ok(if self.responses.is_empty() {
                events.len()
            } else {
                self.responses.remove(0)
            })
        }
        fn launch(&mut self, spec: &LaunchSpec) -> Result<(), String> {
            self.launches.push(spec.clone());
            if self.fail_launch {
                Err("fake launch failure".into())
            } else {
                Ok(())
            }
        }
        fn open_url(&mut self, url: &str) -> Result<(), String> {
            self.urls.push(url.into());
            Ok(())
        }
        fn monitor(&mut self, spec: &MonitorSpec) -> Result<(), String> {
            self.monitors.push(spec.clone());
            Ok(())
        }
    }
    fn action(kind: &str, value: &str) -> PendingAction {
        PendingAction {
            kind: kind.into(),
            value: value.into(),
        }
    }
    #[test]
    fn hotkeys_press_and_release_once_without_releasing_held_modifiers() {
        let mut fake = Fake {
            held: vec![CTRL],
            ..Default::default()
        };
        execute_with(&action("hotkey", "Ctrl + Shift + M"), &mut fake).unwrap();
        assert_eq!(
            fake.batches[0],
            [
                Stroke::virtual_key(SHIFT, false),
                Stroke::virtual_key(0x4d, false),
                Stroke::virtual_key(0x4d, true),
                Stroke::virtual_key(SHIFT, true)
            ]
        );
        assert!(matches!(
            plan_action(&action("hotkey", "Win + F24")).unwrap(),
            ActionPlan::Hotkey { key: 0x87, .. }
        ));
        let mut fake = Fake {
            held: vec![0x5c],
            ..Default::default()
        };
        execute_with(&action("hotkey", "Win + D"), &mut fake).unwrap();
        assert_eq!(
            fake.batches[0].len(),
            2,
            "a physically held right Win key must not be released"
        );
    }
    #[test]
    fn unrelated_held_modifiers_do_not_change_a_hotkey_into_another_shortcut() {
        let mut fake = Fake {
            held: vec![CTRL],
            ..Default::default()
        };
        assert!(execute_with(&action("hotkey", "A"), &mut fake).is_err());
        assert!(fake.batches.is_empty());
    }
    #[test]
    fn partial_keyboard_delivery_releases_only_injected_down_keys() {
        let mut fake = Fake {
            responses: vec![2, 2],
            ..Default::default()
        };
        assert!(execute_with(&action("hotkey", "Ctrl + M"), &mut fake).is_err());
        assert_eq!(
            fake.batches[1],
            [
                Stroke::virtual_key(0x4d, true),
                Stroke::virtual_key(CTRL, true)
            ]
        );
        let mut fake = Fake {
            held: vec![CTRL],
            responses: vec![1, 1],
            ..Default::default()
        };
        assert!(execute_with(&action("hotkey", "Ctrl + M"), &mut fake).is_err());
        assert_eq!(fake.batches[1], [Stroke::virtual_key(0x4d, true)]);
    }
    #[test]
    fn zero_delivery_and_cleanup_failure_are_reported() {
        let mut fake = Fake {
            responses: vec![0],
            ..Default::default()
        };
        assert!(execute_with(&action("media", "Mute"), &mut fake).is_err());
        assert_eq!(fake.batches.len(), 1);
        let mut fake = Fake {
            responses: vec![1, 0],
            ..Default::default()
        };
        assert!(execute_with(&action("hotkey", "Win + D"), &mut fake)
            .unwrap_err()
            .contains("could not all be released"));
    }
    #[test]
    fn unicode_text_includes_surrogate_pairs_and_releases() {
        let mut fake = Fake::default();
        execute_with(&action("text", "A🦀"), &mut fake).unwrap();
        assert_eq!(fake.batches[0].len(), 6);
        assert!(fake.batches[0].iter().all(|s| s.unicode));
        assert!(fake.batches[0]
            .chunks_exact(2)
            .all(|pair| !pair[0].up && pair[1].up && pair[0].key == pair[1].key));
        let mut fake = Fake {
            held: vec![ALT],
            ..Default::default()
        };
        assert!(execute_with(&action("text", "No shortcuts"), &mut fake).is_err());
        assert!(fake.batches.is_empty());
    }
    #[test]
    fn all_media_controls_dispatch_without_real_input() {
        for value in [
            "Play / pause",
            "Next track",
            "Previous track",
            "Volume up",
            "Volume down",
            "Mute",
        ] {
            let mut fake = Fake::default();
            execute_with(&action("media", value), &mut fake).unwrap();
            assert_eq!(fake.batches[0].len(), 2);
            assert!(fake.batches[0][0].extended);
        }
    }
    #[test]
    fn launch_arguments_remain_literal_data_and_failure_propagates() {
        let mut fake = Fake::default();
        execute_with(
            &action(
                "launch",
                r#"{"path":"C:\\Apps\\App.exe","args":["hello world","$(secret)","a&b"]}"#,
            ),
            &mut fake,
        )
        .unwrap();
        assert_eq!(fake.launches[0].args, ["hello world", "$(secret)", "a&b"]);
        fake.fail_launch = true;
        assert!(execute_with(&action("launch", r"C:\Apps\App.exe"), &mut fake).is_err());
    }
    #[test]
    fn urls_are_http_only_and_launch_paths_are_absolute() {
        let mut fake = Fake::default();
        execute_with(
            &action("url", "https://example.com/path?a=1&b=2"),
            &mut fake,
        )
        .unwrap();
        assert_eq!(fake.urls.len(), 1);
        for value in [
            "javascript:alert(1)",
            "file:///C:/secret",
            "https://",
            "https://user:pass@example.com",
            "https://example.com/a b",
        ] {
            assert!(plan_action(&action("url", value)).is_err());
        }
        for value in [
            "cmd /c anything",
            "app.exe",
            r"C:\script.ps1",
            r"\\.\device",
        ] {
            assert!(plan_action(&action("launch", value)).is_err());
        }
        assert!(plan_action(&action("launch", r"C:\Docs\file.txt")).is_ok());
        assert!(plan_action(&action("launch", r"C:\")).is_ok());
        assert!(plan_action(&action("launch", r"\\server\share\file.txt")).is_ok());
    }
    #[test]
    fn invalid_values_are_refused_before_platform_calls() {
        for value in [
            "Ctrl + Ctrl + M",
            "Ctrl + Alt + Delete",
            "Ctrl",
            "A+B",
            "Ctrl++",
            "Ctrl + Unknown",
        ] {
            assert!(plan_action(&action("hotkey", value)).is_err());
        }
        let mut fake = Fake::default();
        assert!(execute_with(&action("text", &"x".repeat(4097)), &mut fake).is_err());
        assert!(fake.batches.is_empty());
        assert!(plan_action(&action("launch", "C:\\app.exe\0bad")).is_err());
        assert!(plan_action(&action("shell", "anything")).is_err());
        assert!(plan_action(&action("page", "page-1")).is_err());
    }
    #[test]
    fn monitor_config_is_explicit_stable_and_fake_dispatched_only() {
        let mut fake = Fake::default();
        execute_with(
            &action("monitor", r#"{"targets":["target-id:2","target-id:1"]}"#),
            &mut fake,
        )
        .unwrap();
        assert_eq!(fake.monitors[0].targets, ["target-id:1", "target-id:2"]);
        let other =
            parse_monitor(r#"{"targets":["TARGET-ID:1","target-id:2"],"mode":"enable"}"#).unwrap();
        assert_eq!(
            monitor_group_key(&fake.monitors[0]),
            monitor_group_key(&other)
        );
        for value in [
            "",
            "{}",
            r#"{"targets":[]}"#,
            r#"{"targets":["a,a"]}"#,
            r#"{"targets":["a","A"]}"#,
            r#"{"targets":["a"],"mode":"restore"}"#,
        ] {
            assert!(plan_action(&action("monitor", value)).is_err());
        }
    }
    #[test]
    fn native_fake_guard_never_creates_folders_or_runs_effects() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("must-not-exist");
        let mut native = NativeActions::new(temp.path().join("resources"), home.clone(), true);
        for action in [
            action("hotkey", "Ctrl + M"),
            action("launch", r"C:\App.exe"),
            action("monitor", r#"{"targets":["target-id:1"]}"#),
        ] {
            assert!(native.execute(&action).unwrap_err().contains("simulated"));
        }
        assert!(!home.exists());
    }

    #[test]
    fn monitor_list_parser_exposes_only_stable_keys_and_readable_names() {
        let result = serde_json::json!({"status":"listed","displays":[
            {"key":"device:one","friendlyName":"Main display","devicePath":"not exported"},
            {"key":"device:two","friendlyName":""},
            {"key":"DEVICE:ONE","friendlyName":"Duplicate"}
        ]});
        assert_eq!(
            parse_monitor_list(&result).unwrap(),
            [
                MonitorInfo {
                    key: "device:one".into(),
                    label: "Main display".into()
                },
                MonitorInfo {
                    key: "device:two".into(),
                    label: "Display 2".into()
                },
            ]
        );
        assert!(
            parse_monitor_list(&serde_json::json!({"status":"disabled","displays":[]})).is_err()
        );
        assert!(parse_monitor_list(
            &serde_json::json!({"status":"listed","displays":[{"key":"bad,key"}]})
        )
        .is_err());
    }

    #[test]
    fn monitor_list_accepts_powershell_zero_one_and_many_shapes() {
        assert!(
            parse_monitor_list(&serde_json::json!({"status":"listed","displays":null}))
                .unwrap()
                .is_empty()
        );
        let display = serde_json::json!({"key":"display:one","friendlyName":"Main"});
        let one =
            parse_monitor_list(&serde_json::json!({"status":"listed","displays":display})).unwrap();
        assert_eq!(
            one,
            parse_monitor_list(&serde_json::json!({"status":"listed","displays":[display]}))
                .unwrap()
        );
        assert_eq!(one[0].label, "Main");
        for displays in [
            serde_json::json!(false),
            serde_json::json!("invalid"),
            serde_json::json!({"friendlyName":"No stable key"}),
        ] {
            assert!(parse_monitor_list(
                &serde_json::json!({"status":"listed","displays":displays})
            )
            .is_err());
        }
        assert!(parse_monitor_list(&serde_json::json!({"status":"listed"})).is_err());
    }
}
