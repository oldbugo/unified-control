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
use super::au05_hotkey::{self as hotkey, Action, Desktop, FakeDesktop};
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
    }
}
fn settings_key(key: KeyId) -> &'static str {
    ACTION_KEYS[key.index() as usize].0
}

pub fn default_settings() -> Value {
    let actions: serde_json::Map<String, Value> = ACTION_KEYS
        .iter()
        .map(|(k, _)| (k.to_string(), json!({"kind":"none","value":"","app":""})))
        .collect();
    json!({"enabled": false, "actions": actions})
}

/// Keep only known keys with valid values.
pub fn sanitize(v: &Value) -> Value {
    let mut out = default_settings();
    out["enabled"] = json!(v["enabled"].as_bool().unwrap_or(false));
    for (key, _) in ACTION_KEYS {
        let a = &v["actions"][key];
        let kind = a["kind"].as_str().filter(|k| hotkey::kind_known(k)).unwrap_or("none");
        let value = a["value"].as_str().filter(|s| s.len() <= 4096 && !s.contains('\0')).unwrap_or("");
        let app = a["app"].as_str().filter(|id| hotkey::app(id).is_some()).unwrap_or("");
        let custom = kind == "hotkey";
        out["actions"][key] = json!({"kind": kind, "value": if custom { value } else { "" }, "app": if custom { app } else { "" }});
    }
    out
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
    KeyTyping(KeyId, KeyTyping),
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
        "keyTyping" => {
            let key = ACTION_KEYS
                .iter()
                .find(|(k, _)| Some(*k) == v["key"].as_str())
                .map(|(_, id)| *id)
                .ok_or("Choose K1, K2, K3 or the knob press")?;
            let typing = match v["mode"].as_str().unwrap_or("") {
                "factory" => KeyTyping::factory(),
                // "Unified Control action": the key types its own unused code, which UC claims as a hotkey.
                "uc" => KeyTyping::own_code(key),
                "keys" => KeyTyping::parse(v["keys"].as_str().unwrap_or(""))?,
                _ => return Err("Choose what the key does".into()),
            };
            Ok(Change::KeyTyping(key, typing))
        }
        _ => Err("Unknown Vibe Key setting".into()),
    }
}

/// What the key reported. `None` = not read yet.
#[derive(Clone, Debug, Default)]
struct Known {
    battery: Option<(u16, u8)>,
    standby_s: Option<u32>,
    sleep_s: Option<u32>,
    light: Option<LightState>,
    hooks_mode: Option<u8>,
    mic: Option<MicNr>,
    keys: [Option<KeyTyping>; 4],
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
    jobs: [Option<KeyJob>; 4],
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
        let au = Arc::new(Self {
            fake: fake.then(FakeHandle::default),
            status: Arc::new(Mutex::new(Status {
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
            })),
            actions: Arc::new(Mutex::new(default_settings()["actions"].clone())),
            tx: Mutex::new(None),
            thread: Mutex::new(None),
            stop: Arc::new(AtomicBool::new(false)),
            desktop,
            fake_desktop,
            hotkeys: Mutex::new(BTreeMap::new()),
            config_lock: Mutex::new(()),
            config,
            log: Arc::new(Log::default()),
        });
        if let Some(d) = &au.desktop {
            let (status, actions, desk, log) = (au.status.clone(), au.actions.clone(), Arc::downgrade(d), au.log.clone());
            d.set_handler(Arc::new(move |vk| {
                let Some(key) = KeyId::ALL.into_iter().find(|k| own_vk(*k) == vk) else { return };
                let action = saved_action(&actions.lock_or_recover(), settings_key(key));
                let (status, desk, log) = (status.clone(), desk.clone(), log.clone());
                // Never run an action on the hotkey thread.
                let _ = std::thread::Builder::new().name("uc-au05-action".into()).spawn(move || {
                    let result = desk.upgrade().ok_or_else(|| "Unified Control is closing".to_string()).and_then(|d| {
                        super::sync::guarded("Vibe Key action", || d.run(&action))
                            .unwrap_or_else(|| Err("The action crashed".into()))
                    });
                    let note = match result {
                        Ok(()) => format!("ran {}", action.label()),
                        Err(e) => format!("{}: {e}", action.label()),
                    };
                    log.line(&format!("hotkey {}: {note}", key.control().name()));
                    push_event(&status, key.control(), true, key.control().own_code().unwrap_or(0), Some(note));
                });
            }));
        }
        au
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
        let wanted: Vec<u16> = if enabled {
            ACTION_KEYS
                .iter()
                .filter(|(k, _)| saved_action(&actions, k) != Action::Nothing)
                .map(|(_, id)| own_vk(*id))
                .collect()
        } else {
            Vec::new()
        };
        let results = d.set_hotkeys(&wanted);
        for (key, id) in ACTION_KEYS {
            let vk = own_vk(id);
            let line = match results.get(&vk) {
                Some(Ok(())) => format!("hotkey {key} claimed ({})", saved_action(&actions, key).label()),
                Some(Err(e)) => format!("hotkey {key} not claimed: {e}"),
                None => continue,
            };
            self.log.line(&line);
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
        let enabled = settings["enabled"].as_bool().unwrap_or(false);
        let running = self.tx.lock_or_recover().is_some();
        if enabled && !running {
            self.start();
        } else if !enabled && running {
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
    }

    /// Stop the worker and release every hotkey (quit).
    pub fn shutdown(&self) {
        let _guard = self.config_lock.lock_or_recover();
        self.stop_worker();
        self.apply_hotkeys(false);
    }

    /// A settings change is busy until the key answers or the request fails.
    pub fn busy(&self) -> bool {
        self.status.lock_or_recover().pending_sets > 0
    }

    /// Send one change to the key and wait for its answer (at most `SET_WAIT`).
    pub fn change(&self, change: Change) -> Result<(), String> {
        if let Change::KeyTyping(key, typing) = change {
            self.log.line(&format!("page change key typing {} ({} entries)", key.control().name(), typing.0.len()));
            return self.set_key(key, typing);
        }
        self.log.line(&format!("page change {change:?}"));
        let tx = self.tx.lock_or_recover().clone().ok_or("Enable the Vibe Key first")?;
        let (rtx, rrx) = mpsc::channel();
        tx.send(Cmd::Change(change, rtx)).map_err(|_| "The Vibe Key worker is unavailable")?;
        rrx.recv_timeout(SET_WAIT + Duration::from_secs(5))
            .map_err(|_| "The Vibe Key did not answer in time".to_string())?
    }

    /// Ask for a key to type `typing`. Returns at once: the worker sends it when the key is awake, and the
    /// page follows the job's state ("Waiting for the key", "Sending", "Done" or the error).
    pub fn set_key(&self, key: KeyId, typing: KeyTyping) -> Result<(), String> {
        typing.validate()?;
        let mut s = self.status.lock_or_recover();
        if !s.enabled {
            return Err("Enable the Vibe Key first".into());
        }
        if s.jobs[key.index() as usize].as_ref().is_some_and(|j| j.state == "sending") {
            return Err("This key is being changed now. Try again in a moment".into());
        }
        s.jobs[key.index() as usize] = Some(KeyJob {
            typing,
            state: "waiting",
            message: String::new(),
        });
        drop(s);
        self.log.line(&format!("key job queued for {}", key.control().name()));
        Ok(())
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

    pub fn status(&self) -> Value {
        let s = self.status.lock_or_recover().clone();
        let k = &s.known;
        let hotkeys = self.hotkeys.lock_or_recover().clone();
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
                            ("waiting", "not_found") => "Waiting for the receiver: plug it in".to_string(),
                            ("waiting", "paused") => "Waiting: sending is paused after a slow reply".to_string(),
                            ("waiting", "suspended") => "Stopped after slow replies: use Reconnect".to_string(),
                            ("waiting", _) => "Waiting for the key: press any key to wake it".to_string(),
                            ("sending", _) => "Sending to the key (it can take up to 20 seconds)".to_string(),
                            ("done", _) => "Done".to_string(),
                            _ => j.message.clone(),
                        };
                        json!({"state": j.state, "message": message, "label": j.typing.label(id)})
                    }),
                    // Every key can be set; the knob press typing is not checked on the real key yet.
                    "canSet": true,
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
            "battery": k.battery.map(|(mv, pct)| json!({"millivolts": mv, "percent": pct})),
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
            "events": events,
            "actions": self.actions.lock_or_recover().clone(),
            "hotkeysAvailable": self.desktop.is_some(),
            "catalogue": hotkey::catalogue_json(),
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
                if let Some(id) = KeyId::ALL.into_iter().find(|k| k.control() == control) {
                    let own = fake.lock().typing[id.index() as usize] == KeyTyping::own_code(id);
                    if let (true, Some(d)) = (own, &self.fake_desktop) {
                        d.press(own_vk(id));
                    }
                }
            }
            "wake" => fake.wake(),
            "sleep" => fake.sleep(),
            "stallWrites" => fake.lock().stall_writes = v["value"].as_bool().unwrap_or(false),
            "dropReplies" => fake.lock().drop_replies = v["value"].as_u64().unwrap_or(0).min(10) as u32,
            "battery" => fake.lock().percent = v["value"].as_u64().unwrap_or(30).min(100) as u8,
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
    fake: Option<FakeHandle>,
    status: Arc<Mutex<Status>>,
    stop: Arc<AtomicBool>,
    config: SessionConfig,
    log: Arc<Log>,
}

enum Purpose {
    Read,
    Set(Sender<Result<(), String>>),
    /// A key-typing job (see `KeyJob`).
    Key(KeyId),
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

    /// Send the next waiting key job when the key is awake and no other key-typing command is pending.
    fn start_jobs(&self, session: &mut Session<Box<dyn Transport>>, tickets: &mut HashMap<u64, Purpose>) {
        if session.state() != SessionState::Awake || session.has_pending(Request::slow_reply) {
            return;
        }
        let next = {
            let s = self.status.lock_or_recover();
            KeyId::ALL.into_iter().find_map(|k| {
                s.jobs[k.index() as usize]
                    .as_ref()
                    .filter(|j| j.state == "waiting")
                    .map(|j| (k, j.typing.clone()))
            })
        };
        let Some((key, typing)) = next else { return };
        let t = TICKETS.fetch_add(1, Ordering::Relaxed);
        let request = Request::SetKeyTyping(key, typing);
        let name = describe(&request);
        let result = session.submit(t, request);
        let mut s = self.status.lock_or_recover();
        let job = s.jobs[key.index() as usize].as_mut().unwrap();
        match result {
            Ok(()) => {
                job.state = "sending";
                tickets.insert(t, Purpose::Key(key));
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
            return Ok(Box::new(f.transport()));
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
        let mut tickets: HashMap<u64, Purpose> = HashMap::new();
        let mut read_done = false;
        let mut read_burst: Option<(Instant, Vec<u64>)> = None;
        let mut last_state = None;
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
                        self.status.lock_or_recover().last_error = None;
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
                        if session.has_pending(Request::slow_reply) {
                            let _ = reply.send(Err("Another key is being read. Try again in a moment".into()));
                        } else {
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
            for o in outputs {
                self.handle(o, &mut tickets);
            }
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
            if last_state != Some(state) {
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
            Change::KeyTyping(k, t) => Request::SetKeyTyping(k, t),
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
                self.status.lock_or_recover().known.battery = Some((st.millivolts, st.percent));
            }
            // Private events are only shown (K1 and the knob turns send them). Actions use hotkeys.
            Output::Control(e) => push_event(&self.status, e.control, e.pressed, e.code, None),
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
                match purpose {
                    Some(Purpose::Set(reply)) => {
                        let _ = reply.send(result.map(|_| ()).map_err(|f| f.message()));
                    }
                    Some(Purpose::Key(key)) => {
                        let mut s = self.status.lock_or_recover();
                        if let Some(job) = s.jobs[key.index() as usize].as_mut() {
                            match &result {
                                Ok(_) => job.state = "done",
                                // Never written (the key went to sleep first): wait for it again.
                                Err(Failure::Asleep) | Err(Failure::Closed) | Err(Failure::Cancelled) => {
                                    job.state = "waiting"
                                }
                                Err(f) => {
                                    job.state = "failed";
                                    job.message = f.message();
                                }
                            }
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
            Request::GetBattery => k.battery = Some(protocol::battery(r)),
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
        std::thread::sleep(Duration::from_millis(5200)); // past the test awake window
        assert_eq!(au.status()["keys"][2]["mode"], "unknown");
        let started = Instant::now();
        au.configure(&json!({"enabled": true, "actions": {"k3": {"kind":"claudeDictation"}}}));
        au.change(Change::KeyTyping(KeyId::K3, KeyTyping::own_code(KeyId::K3))).unwrap();
        assert!(started.elapsed() < Duration::from_millis(500), "the choice returns at once");
        let s = au.status();
        assert_eq!(s["actions"]["k3"]["kind"], "claudeDictation", "saved before the list is read");
        assert_eq!(s["keys"][2]["hotkey"]["state"], "ready", "the hotkey is claimed at once");
        assert_eq!(s["keys"][2]["job"]["state"], "waiting");
        assert_eq!(s["keys"][2]["job"]["message"], "Waiting for the key: press any key to wake it");
        std::thread::sleep(Duration::from_millis(300));
        assert!(fake.received().iter().all(|p| p[2] != 0x50), "nothing is written to a sleeping key");
        // Waking the key sends the change; the job ends in Done and the key now types its own code.
        fake.wake();
        let s = wait_for(&au, |s| s["keys"][2]["job"]["state"] == "done");
        assert_eq!(s["keys"][2]["job"]["message"], "Done");
        assert_eq!(s["keys"][2]["mode"], "uc");
        // A failure ends in the error, not in a spinner.
        fake.lock().drop_replies = 1;
        au.change(Change::KeyTyping(KeyId::K3, KeyTyping::factory())).unwrap();
        let s = wait_for(&au, |s| s["keys"][2]["job"]["state"] == "failed");
        assert!(s["keys"][2]["job"]["message"].as_str().unwrap().contains("did not answer"));
        assert_eq!(s["busy"], false);
        au.shutdown();
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
        assert_eq!(d["actions"]["k3"], json!({"kind":"none","value":"","app":""}));
        assert!(d["actions"].get("knobRight").is_none(), "knob turns keep the factory scroll");
        let s = sanitize(&json!({"enabled":true,"actions":{"k1":{"kind":"claudeDictation"},"k3":{"kind":"hotkey","value":"Ctrl + Enter"},"k2":{"kind":"shell","value":"x"}}}));
        assert_eq!(s["enabled"], true);
        assert_eq!(s["actions"]["k1"]["kind"], "claudeDictation");
        assert_eq!(s["actions"]["k3"]["value"], "Ctrl + Enter");
        assert_eq!(s["actions"]["k2"]["kind"], "none", "unknown action kinds are dropped");
        assert!(validate_action("hotkey", "Ctrl + Enter", "").is_ok());
        assert!(validate_action("launch", "C:/x.exe", "").is_err(), "only reviewed kinds");
    }

    #[test]
    fn page_changes_parse_to_allowlisted_values_only() {
        assert_eq!(parse_change(&json!({"setting":"standbyTime","value":600})), Ok(Change::StandbyTime(600)));
        assert!(parse_change(&json!({"setting":"sleepTime","value":1234})).is_err());
        assert_eq!(
            parse_change(&json!({"setting":"keyTyping","key":"k3","mode":"uc"})),
            Ok(Change::KeyTyping(KeyId::K3, KeyTyping::own_code(KeyId::K3)))
        );
        assert_eq!(
            parse_change(&json!({"setting":"keyTyping","key":"knobPress","mode":"uc"})),
            Ok(Change::KeyTyping(KeyId::KnobPress, KeyTyping::own_code(KeyId::KnobPress)))
        );
        assert_eq!(
            parse_change(&json!({"setting":"keyTyping","key":"k1","mode":"keys","keys":"Ctrl + D"})),
            Ok(Change::KeyTyping(KeyId::K1, KeyTyping::parse("Ctrl + D").unwrap()))
        );
        assert!(parse_change(&json!({"setting":"keyTyping","key":"knobLeft","mode":"factory"})).is_err());
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
        au.change(Change::StandbyTime(900)).unwrap();
        au.change(Change::KeyTyping(KeyId::K1, KeyTyping::parse("Ctrl + D").unwrap())).unwrap();
        au.change(Change::SleepTime(0)).unwrap();
        au.change(Change::MicNrLevel(2)).unwrap();
        let s = wait_for(&au, |s| s["keys"][0]["mode"] == "keys" && s["standbyS"] == 900);
        assert_eq!(s["keys"][0]["label"], "Ctrl + D", "learned from the set's echo");
        let sent50: Vec<_> = au.fake_handle().unwrap().received().into_iter().filter(|p| p[2] == 0x50).collect();
        assert_eq!(sent50.len(), 2, "one read (K3) and one set (K1); no read-back");
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
        wait_for(&au, |s| s["state"] == "connected" && s["micNr"].is_object());
        au.read_key(KeyId::K3).unwrap();
        let s = au.status();
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
        // K2 types its factory Enter, not its own code: no hotkey press reaches UC.
        au.test_control(&json!({"op":"click","control":"k2"})).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(desk.ran.lock_or_recover().len(), 1);
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
}
