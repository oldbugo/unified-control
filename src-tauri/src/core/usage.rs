//! Managed, explicitly activated Node account reader over private inherited pipes.
//! No constructor/render fallback starts a process, and no authentication file is read.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const USAGE_VIEWS: &[&str] = &[
    "rings",
    "bars",
    "trend",
    "tokens",
    "resets",
    "five-hour-ring",
    "weekly-ring",
    "five-hour-number",
    "weekly-number",
    "five-hour-reset",
    "weekly-reset",
    "five-hour-time",
    "weekly-time",
    "five-hour-trend",
    "weekly-trend",
    "wide-dashboard",
];
const MAX_REPLY: usize = 2 * 1024 * 1024;
const FRESH_MS: u64 = 300_000;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveProvider {
    pub provider: Provider,
    pub token_history: bool,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UsageStatus {
    Loading,
    Ok,
    Stale,
    Unavailable,
    Error,
    Inactive,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageWindow {
    pub used_percent: f64,
    pub resets_at: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DailyTokens {
    pub date: String,
    pub tokens: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageSnapshot {
    pub provider: Provider,
    pub status: UsageStatus,
    pub observed_at: Option<u64>,
    pub provenance_verified: bool,
    pub five_hour: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
    pub token_history_status: String,
    pub token_history_observed_at: Option<u64>,
    pub token_history: Vec<DailyTokens>,
    pub message: String,
}
impl UsageSnapshot {
    pub fn unknown(provider: Provider, status: UsageStatus) -> Self {
        Self {
            provider,
            status,
            observed_at: None,
            provenance_verified: false,
            five_hour: None,
            weekly: None,
            token_history_status: "unavailable".into(),
            token_history_observed_at: None,
            token_history: Vec::new(),
            message: "Current account usage is unavailable.".into(),
        }
    }
    /// Re-evaluated at receipt, never after writing an old PNG to a display.
    pub fn guarded(mut self, now: u64) -> Self {
        self.message = "Current account usage is unavailable.".into();
        let fresh = |at: Option<u64>, age| at.is_some_and(|t| t > 0 && t <= now.saturating_add(1000) && now.saturating_sub(t) <= age);
        if self.status != UsageStatus::Ok || !self.provenance_verified || !fresh(self.observed_at, FRESH_MS) {
            let stale = self.status == UsageStatus::Stale
                || self.status == UsageStatus::Ok && self.observed_at.is_some_and(|t| now.saturating_sub(t) > FRESH_MS);
            self.status = if stale {
                UsageStatus::Stale
            } else if self.status == UsageStatus::Ok {
                UsageStatus::Unavailable
            } else {
                self.status
            };
            self.provenance_verified = false;
            self.five_hour = None;
            self.weekly = None;
            self.token_history.clear();
            self.token_history_observed_at = None;
            self.token_history_status = "unavailable".into();
            return self;
        }
        let valid = |w: &UsageWindow| {
            w.used_percent.is_finite()
                && (0.0..=100.0).contains(&w.used_percent)
                && w.resets_at.is_none_or(|t| t > now / 1000 && t <= 100_000_000_000)
        };
        self.five_hour = self.five_hour.filter(valid);
        self.weekly = self.weekly.filter(valid);
        if self.five_hour.is_none() && self.weekly.is_none() {
            return Self::unknown(self.provider, UsageStatus::Unavailable);
        }
        if self.provider != Provider::Codex || self.token_history_status != "ok" || !fresh(self.token_history_observed_at, 7_200_000) {
            self.token_history.clear();
            self.token_history_observed_at = None;
            self.token_history_status = "unavailable".into();
        } else {
            self.token_history
                .retain(|v| valid_date(&v.date) && v.tokens <= 9_007_199_254_740_991);
            self.token_history.truncate(30);
            if self.token_history.is_empty() {
                self.token_history_status = "unavailable".into();
                self.token_history_observed_at = None;
            }
        }
        self.message = "Verified account observation.".into();
        self
    }
}
fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' || !b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit()) {
        return false;
    }
    let year = s[..4].parse::<u32>().unwrap_or(0);
    let month = s[5..7].parse::<usize>().unwrap_or(0);
    let day = s[8..].parse::<u32>().unwrap_or(0);
    let days = [
        0,
        31,
        if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    year > 0 && month > 0 && month < 13 && day > 0 && day <= days[month]
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageRender {
    pub png: Vec<u8>,
    pub snapshot: UsageSnapshot,
}
#[derive(Debug)]
pub struct UsageError(&'static str);
impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for UsageError {}
type Result<T> = std::result::Result<T, UsageError>;
fn error(_: impl std::fmt::Debug) -> UsageError {
    UsageError("The usage reader is unavailable.")
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

struct ServiceState {
    process: Option<Process>,
    active: Vec<ActiveProvider>,
}
pub struct UsageService {
    node: PathBuf,
    script: PathBuf,
    home: PathBuf,
    timeout: Duration,
    state: Mutex<ServiceState>,
}
impl UsageService {
    pub fn new(node_executable: impl Into<PathBuf>, reader_script: impl Into<PathBuf>, uc_home: impl Into<PathBuf>) -> Self {
        Self {
            node: node_executable.into(),
            script: reader_script.into(),
            home: uc_home.into(),
            timeout: Duration::from_secs(5),
            state: Mutex::new(ServiceState {
                process: None,
                active: Vec::new(),
            }),
        }
    }
    /// This is the only method allowed to spawn. Empty config stops the owned child.
    pub fn set_active_providers(&self, active: &[ActiveProvider]) -> Result<()> {
        if active.len() > 2
            || active.iter().any(|c| c.provider == Provider::Claude && c.token_history)
            || active
                .iter()
                .enumerate()
                .any(|(i, c)| active[..i].iter().any(|o| o.provider == c.provider))
        {
            return Err(UsageError("Invalid active providers."));
        }
        let mut state = self.state.lock().map_err(error)?;
        if active.is_empty() {
            state.active.clear();
            if let Some(mut process) = state.process.take() {
                process.shutdown();
            }
            return Ok(());
        }
        if state.process.is_none() {
            state.process = Some(Process::start(&self.node, &self.script, &self.home)?);
        }
        let result = state
            .process
            .as_mut()
            .unwrap()
            .request(json!({"method":"set_active","providers":active}), self.timeout);
        if result.is_err() {
            state.process = None;
            state.active.clear();
            return result.map(|_| ());
        }
        state.active = active.to_vec();
        Ok(())
    }
    pub fn is_running(&self) -> bool {
        let Ok(mut state) = self.state.lock() else { return false };
        let alive = state.process.as_mut().is_some_and(Process::healthy);
        if !alive {
            state.process = None;
            state.active.clear();
        }
        alive
    }
    /// Bounded, account-free pipe heartbeat. Intended for a supervisor (e.g. every
    /// 30 seconds), not every rendered key. Failure releases this child only.
    pub fn check_health(&self) -> bool {
        let Ok(mut state) = self.state.lock() else { return false };
        let healthy = state.process.as_mut().is_some_and(|process| {
            process.healthy()
                && process
                    .request(json!({"method":"health"}), self.timeout.min(Duration::from_millis(500)))
                    .ok()
                    .is_some_and(|v| v.get("alive") == Some(&json!(true)))
        });
        if !healthy {
            state.process = None;
            state.active.clear();
        }
        healthy
    }
    pub fn snapshot(&self, provider: Provider) -> Result<UsageSnapshot> {
        let mut state = self.state.lock().map_err(error)?;
        if !state.active.iter().any(|c| c.provider == provider) {
            return Ok(UsageSnapshot::unknown(provider, UsageStatus::Inactive));
        }
        let result = state
            .process
            .as_mut()
            .ok_or(UsageError("The usage reader is inactive."))?
            .request(json!({"method":"snapshot","provider":provider}), self.timeout);
        match result.and_then(|v| serde_json::from_value::<UsageSnapshot>(v).map_err(error)) {
            Ok(snapshot) if snapshot.provider == provider => Ok(snapshot.guarded(now_ms())),
            _ => {
                state.process = None;
                state.active.clear();
                Err(UsageError("The usage reader returned an invalid snapshot."))
            }
        }
    }
    /// Call on the app's worker path, never the UI thread. Inactive/error => caller
    /// renders unknown. No last-good numeric PNG is cached by this service.
    pub fn render(&self, provider: Provider, view: &str, width: u32, height: u32) -> Result<UsageRender> {
        if !USAGE_VIEWS.contains(&view)
            || view == "tokens" && provider != Provider::Codex
            || ![(196, 196), (458, 196)].contains(&(width, height))
        {
            return Err(UsageError("Invalid usage presentation."));
        }
        let mut state = self.state.lock().map_err(error)?;
        if !state.active.iter().any(|c| c.provider == provider) {
            return Err(UsageError("The usage reader is inactive."));
        }
        let result = state.process.as_mut().ok_or(UsageError("The usage reader is inactive."))?.request(
            json!({
            "method":"render","provider":provider,"view":view,"width":width,"height":height}),
            self.timeout,
        );
        let result = result.and_then(|v| serde_json::from_value::<UsageRender>(v).map_err(error));
        let mut rendered = match result {
            Ok(v) => v,
            Err(e) => {
                state.process = None;
                state.active.clear();
                return Err(e);
            }
        };
        if rendered.snapshot.provider != provider || !valid_png(&rendered.png, width, height) {
            state.process = None;
            state.active.clear();
            return Err(UsageError("The usage reader returned an invalid image."));
        }
        rendered.snapshot = guard_delivery(rendered.snapshot, now_ms())?;
        Ok(rendered)
    }
    pub fn shutdown(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active.clear();
            if let Some(mut process) = state.process.take() {
                process.shutdown();
            }
        }
    }
}
impl Drop for UsageService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn valid_png(data: &[u8], width: u32, height: u32) -> bool {
    if data.len() < 33 || data.len() > 512 * 1024 {
        return false;
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_limits(png::Limits { bytes: 2 * 1024 * 1024 });
    let mut reader = match decoder.read_info() {
        Ok(v) => v,
        Err(_) => return false,
    };
    if reader.info().width != width || reader.info().height != height || reader.info().animation_control.is_some() {
        return false;
    }
    let size = match reader.output_buffer_size() {
        Some(n) if n <= 512 * 1024 => n,
        _ => return false,
    };
    let mut pixels = vec![0; size];
    reader.next_frame(&mut pixels).is_ok() && reader.finish().is_ok()
}
fn guard_delivery(before: UsageSnapshot, now: u64) -> Result<UsageSnapshot> {
    let guarded = before.clone().guarded(now);
    // Freshness/reset/token data can expire independently while the child draws.
    // Reject the existing PNG if ANY numerical field was removed, even when the
    // other quota window keeps the overall snapshot status OK.
    if before.status == UsageStatus::Ok
        && (guarded.status != UsageStatus::Ok
            || before.five_hour != guarded.five_hour
            || before.weekly != guarded.weekly
            || before.token_history != guarded.token_history
            || before.token_history_observed_at != guarded.token_history_observed_at)
    {
        return Err(UsageError("The usage image expired before delivery."));
    }
    Ok(guarded)
}

struct Process {
    child: Child,
    writer: Option<mpsc::SyncSender<Vec<u8>>>,
    replies: mpsc::Receiver<std::result::Result<Value, ()>>,
    next_id: u64,
    alive: Arc<AtomicBool>,
    #[cfg(windows)]
    job: OwnedJob,
}
impl Process {
    fn start(node: &Path, script: &Path, home: &Path) -> Result<Self> {
        let mut command = Command::new(node);
        command
            .arg(script)
            .env("UC_USAGE_HOME", home.join("usage"))
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_PATH")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(error)?;
        #[cfg(windows)]
        let job = match OwnedJob::adopt(&child) {
            Ok(job) => job,
            Err(e) => {
                let _ = child.kill();
                bounded_reap(&mut child);
                return Err(e);
            }
        };
        let mut input = child.stdin.take().ok_or(UsageError("Usage input pipe unavailable."))?;
        let output = child.stdout.take().ok_or(UsageError("Usage output pipe unavailable."))?;
        let (write_tx, write_rx) = mpsc::sync_channel::<Vec<u8>>(1);
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let write_errors = reply_tx.clone();
        let alive = Arc::new(AtomicBool::new(true));
        let write_alive = alive.clone();
        let read_alive = alive.clone();
        std::thread::spawn(move || {
            while let Ok(line) = write_rx.recv() {
                if input.write_all(&line).and_then(|_| input.flush()).is_err() {
                    write_alive.store(false, Ordering::SeqCst);
                    let _ = write_errors.send(Err(()));
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                let got = reader.by_ref().take((MAX_REPLY + 1) as u64).read_until(b'\n', &mut line);
                if !matches!(got,Ok(n) if n>0) || line.len() > MAX_REPLY || !line.ends_with(b"\n") {
                    read_alive.store(false, Ordering::SeqCst);
                    let _ = reply_tx.send(Err(()));
                    break;
                }
                let parsed = serde_json::from_slice(&line).map_err(|_| ());
                let invalid = parsed.is_err();
                if reply_tx.send(parsed).is_err() || invalid {
                    read_alive.store(false, Ordering::SeqCst);
                    break;
                }
            }
        });
        Ok(Self {
            child,
            writer: Some(write_tx),
            replies: reply_rx,
            next_id: 1,
            alive,
            #[cfg(windows)]
            job,
        })
    }
    fn request(&mut self, mut value: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(UsageError("Usage request limit reached."))?;
        value["id"] = json!(id);
        let mut bytes = serde_json::to_vec(&value).map_err(error)?;
        bytes.push(b'\n');
        if bytes.len() > 4096 {
            return Err(UsageError("Usage request too large."));
        }
        self.writer
            .as_ref()
            .ok_or(UsageError("Usage pipe closed."))?
            .try_send(bytes)
            .map_err(error)?;
        let reply = self.replies.recv_timeout(timeout).map_err(error)?.map_err(error)?;
        if reply.get("id").and_then(Value::as_u64) != Some(id) || reply.get("error").is_some() {
            return Err(UsageError("Usage reader rejected the request."));
        }
        reply.get("result").cloned().ok_or(UsageError("Usage result missing."))
    }
    fn healthy(&mut self) -> bool {
        self.alive.load(Ordering::SeqCst) && matches!(self.child.try_wait(), Ok(None))
    }
    fn shutdown(&mut self) {
        let _ = self.request(json!({"method":"shutdown"}), Duration::from_millis(500));
        self.writer = None;
        self.terminate();
    }
    fn terminate(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        self.job.close();
        let _ = self.child.kill();
        bounded_reap(&mut self.child);
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.writer = None;
        self.terminate();
    }
}
fn bounded_reap(child: &mut Child) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(250) {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(windows)]
struct OwnedJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for OwnedJob {}
#[cfg(windows)]
impl OwnedJob {
    fn adopt(child: &Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::HANDLE, System::JobObjects::*};
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(UsageError("Usage process containment unavailable."));
            }
            let job = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle() as HANDLE) == 0
            {
                return Err(UsageError("Usage process containment unavailable."));
            }
            Ok(job)
        }
    }
    fn close(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
            self.0 = std::ptr::null_mut();
        }
    }
}
#[cfg(windows)]
impl Drop for OwnedJob {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> PathBuf {
        std::env::var_os("UC_USAGE_TEST_FIXTURES")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../companions/usage-reader/tests"))
            .join(name)
    }
    fn service(name: &str) -> (tempfile::TempDir, UsageService) {
        let dir = tempfile::tempdir().unwrap();
        let node = std::env::var_os("UC_TEST_NODE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "node.exe" } else { "node" }));
        let service = UsageService::new(node, fixture(name), dir.path());
        (dir, service)
    }
    fn current() -> UsageSnapshot {
        let now = now_ms();
        UsageSnapshot {
            provider: Provider::Codex,
            status: UsageStatus::Ok,
            observed_at: Some(now),
            provenance_verified: true,
            five_hour: Some(UsageWindow {
                used_percent: 12.5,
                resets_at: Some(now / 1000 + 3600),
            }),
            weekly: None,
            token_history_status: "ok".into(),
            token_history_observed_at: Some(now),
            token_history: vec![DailyTokens {
                date: "2026-10-31".into(),
                tokens: 123456,
            }],
            message: "discard untrusted message".into(),
        }
    }
    #[test]
    fn inactive_calls_never_spawn_or_read_accounts() {
        let (_dir, s) = service("fixture.mjs");
        assert!(!s.is_running());
        assert_eq!(s.snapshot(Provider::Codex).unwrap().status, UsageStatus::Inactive);
        assert!(s.render(Provider::Codex, "rings", 196, 196).is_err());
        assert!(!s.is_running());
        s.set_active_providers(&[]).unwrap();
        assert!(!s.is_running());
    }
    #[test]
    fn private_pipe_fixture_returns_physical_png_and_stops_owned_child() {
        let (_dir, s) = service("fixture.mjs");
        s.set_active_providers(&[ActiveProvider {
            provider: Provider::Codex,
            token_history: false,
        }])
        .unwrap();
        assert!(s.is_running());
        assert!(s.check_health());
        let result = s.render(Provider::Codex, "rings", 196, 196).unwrap();
        assert!(valid_png(&result.png, 196, 196));
        assert_eq!(result.snapshot.status, UsageStatus::Unavailable);
        assert!(result.snapshot.five_hour.is_none());
        assert!(valid_png(
            &s.render(Provider::Codex, "wide-dashboard", 458, 196).unwrap().png,
            458,
            196
        ));
        assert_eq!(s.snapshot(Provider::Codex).unwrap().status, UsageStatus::Unavailable);
        s.shutdown();
        assert!(!s.is_running());
        assert_eq!(s.snapshot(Provider::Codex).unwrap().status, UsageStatus::Inactive);
    }
    #[test]
    fn hung_reader_has_a_deadline_and_no_automatic_restart() {
        let (_dir, mut s) = service("hang-fixture.mjs");
        s.timeout = Duration::from_millis(150);
        let start = Instant::now();
        assert!(s
            .set_active_providers(&[ActiveProvider {
                provider: Provider::Codex,
                token_history: false
            }])
            .is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(!s.is_running());
        assert!(s.render(Provider::Codex, "rings", 196, 196).is_err());
        assert!(!s.is_running());
    }
    #[test]
    fn malformed_child_output_fails_closed_and_releases_process() {
        let (_dir, s) = service("bad-fixture.mjs");
        assert!(s
            .set_active_providers(&[ActiveProvider {
                provider: Provider::Codex,
                token_history: false
            }])
            .is_err());
        assert!(!s.is_running());
    }
    #[test]
    fn child_exit_is_detected_without_rendering_or_reading_an_account() {
        let (_dir, s) = service("exit-fixture.mjs");
        s.set_active_providers(&[ActiveProvider {
            provider: Provider::Codex,
            token_history: false,
        }])
        .unwrap();
        let start = Instant::now();
        while s.is_running() && start.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!s.is_running(), "child death must not remain a running process object");
        assert!(!s.check_health());
        assert!(s.render(Provider::Codex, "rings", 196, 196).is_err());
    }
    #[test]
    fn unresponsive_live_child_fails_bounded_health_check_without_auto_restart() {
        let (_dir, s) = service("silent-fixture.mjs");
        s.set_active_providers(&[ActiveProvider {
            provider: Provider::Codex,
            token_history: false,
        }])
        .unwrap();
        assert!(s.is_running());
        let start = Instant::now();
        assert!(!s.check_health());
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(!s.is_running());
    }
    #[test]
    fn bounded_health_ping_never_starts_inactive_reader() {
        let (_dir, s) = service("fixture.mjs");
        assert!(!s.check_health());
        assert!(!s.is_running());
    }
    #[test]
    fn freshness_provenance_reset_and_logout_strip_all_current_numbers() {
        let now = now_ms();
        let raw = current();
        assert_eq!(raw.clone().guarded(now).five_hour.unwrap().used_percent, 12.5);
        let mut unverified = raw.clone();
        unverified.provenance_verified = false;
        let clean = unverified.guarded(now);
        assert!(clean.five_hour.is_none());
        assert!(clean.token_history.is_empty());
        let mut stale = raw.clone();
        stale.observed_at = Some(now - FRESH_MS - 1);
        let stale = stale.guarded(now);
        assert_eq!(stale.status, UsageStatus::Stale);
        assert!(stale.token_history.is_empty());
        let mut future = raw.clone();
        future.observed_at = Some(now + 1001);
        assert!(future.guarded(now).five_hour.is_none());
        let mut logout = raw.clone();
        logout.status = UsageStatus::Unavailable;
        assert!(logout.guarded(now).token_history.is_empty());
        let mut expired = raw;
        expired.five_hour.as_mut().unwrap().resets_at = Some(now / 1000);
        assert!(expired.guarded(now).five_hour.is_none());
    }
    #[test]
    fn invalid_views_dimensions_and_extra_identity_fields_are_rejected() {
        let (_dir, s) = service("fixture.mjs");
        assert!(s.render(Provider::Claude, "tokens", 196, 196).is_err());
        assert!(s.render(Provider::Codex, "rings", 100000, 196).is_err());
        assert!(s.render(Provider::Codex, "bad", 196, 196).is_err());
        let mut v = serde_json::to_value(current()).unwrap();
        v["accountId"] = json!("synthetic-disallowed");
        assert!(serde_json::from_value::<UsageSnapshot>(v).is_err());
        assert!(!valid_date("2026-02-30"));
        assert!(valid_date("2024-02-29"));
        assert!(!s.is_running());
    }
    #[test]
    fn one_expired_window_or_bad_token_day_rejects_png_even_if_other_window_keeps_ok() {
        let now = now_ms();
        let mut raw = current();
        raw.weekly = Some(UsageWindow {
            used_percent: 25.0,
            resets_at: Some(now / 1000 + 86400),
        });
        raw.five_hour.as_mut().unwrap().resets_at = Some(now / 1000);
        assert_eq!(raw.clone().guarded(now).status, UsageStatus::Ok);
        assert!(guard_delivery(raw, now).is_err());
        let mut raw = current();
        raw.token_history[0].date = "2026-02-30".into();
        assert_eq!(raw.clone().guarded(now).status, UsageStatus::Ok);
        assert!(guard_delivery(raw, now).is_err());
    }
    #[test]
    fn png_validation_decodes_data_and_rejects_header_only_or_corrupt_pixels() {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 196, 196);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&vec![255; 196 * 196 * 4]).unwrap();
        }
        assert!(valid_png(&bytes, 196, 196));
        assert!(!valid_png(&bytes, 458, 196));
        assert!(!valid_png(&bytes[..33], 196, 196));
        bytes.truncate(bytes.len() / 2);
        assert!(!valid_png(&bytes, 196, 196));
    }
}
