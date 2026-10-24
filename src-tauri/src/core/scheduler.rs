//! Work scheduler (replaces 1.2's single FIFO `gate.js`), per docs/perf/2026-10-11-performance-investigation.md:
//! priorities (user > selected clip > visible > background), cancellation by job / clip / version that kills the
//! ffmpeg processes a task started, resource classes with limits (cpu, gpu, device exclusive, net, capture, io),
//! coalescing of superseded requests (a newer estimate for the same clip replaces the older one), preemption of
//! background work when a high-priority task is starved, and job events for the UI.
//!
//! Every task runs on its own OS thread once its resources are granted (the limits bound the thread count).
use super::error::{CoreError, CoreResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------- resources ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Res {
    /// CPU encodes and CPU decodes (ffmpeg software paths).
    Cpu = 0,
    /// NVDEC / NVENC sessions.
    Gpu = 1,
    /// The USB screen. Always exclusive (limit 1).
    Device = 2,
    /// Network downloads.
    Net = 3,
    /// Scene recording (Windows Graphics Capture).
    Capture = 4,
    /// Light work: probes, thumbnails, gallery scans.
    Io = 5,
}
pub const N_RES: usize = 6;
const RES_NAMES: [&str; N_RES] = ["cpu", "gpu", "device", "net", "capture", "io"];

/// Units a task needs of each class; it starts only when all are free.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resources(pub [u32; N_RES]);

impl Resources {
    pub fn none() -> Self {
        Resources([0; N_RES])
    }
    pub fn one(r: Res) -> Self {
        Self::none().with(r, 1)
    }
    pub fn with(mut self, r: Res, n: u32) -> Self {
        self.0[r as usize] = n;
        self
    }
    pub fn get(&self, r: Res) -> u32 {
        self.0[r as usize]
    }
    fn media(&self) -> bool {
        self.get(Res::Cpu) > 0 || self.get(Res::Gpu) > 0
    }
    fn names(&self) -> Vec<&'static str> {
        (0..N_RES).filter(|&i| self.0[i] > 0).map(|i| RES_NAMES[i]).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits(pub [u32; N_RES]);

impl Default for Limits {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4);
        Limits([(cores / 2).max(1), 2, 1, 4, 1, 8])
    }
}

// ---------- priority / state ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// Explicit user actions: save version, send, setup.
    User = 0,
    /// The clip the user has open.
    Selected = 1,
    /// Clips on screen in the gallery.
    Visible = 2,
    Background = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn finished(self) -> bool {
        matches!(self, JobState::Done | JobState::Failed | JobState::Cancelled)
    }
    /// The 1.2 `status` field.
    fn legacy(self) -> &'static str {
        match self {
            JobState::Queued => "waiting",
            JobState::Running => "running",
            JobState::Done => "done",
            JobState::Failed | JobState::Cancelled => "error",
        }
    }
}

// ---------- task spec / job record ----------

/// What to run and how to schedule it.
#[derive(Clone, Debug)]
pub struct TaskSpec {
    pub kind: String,
    pub label: String,
    pub original_id: Option<String>,
    pub version_id: Option<String>,
    pub source_id: Option<String>,
    pub item_id: Option<String>,
    pub priority: Priority,
    pub resources: Resources,
    /// May be cancelled and re-queued to make room for selected/user work.
    pub preemptible: bool,
    /// A newer task with the same key replaces this one (queued: dropped; running: cancelled).
    pub coalesce_key: Option<String>,
    /// Shown in the Activity strip / sent as `cs:job` events.
    pub visible: bool,
    /// Cancelled when the user leaves its clip (`set_focus`).
    pub cancel_on_blur: bool,
    /// Follows `set_focus` (selected / visible / background). `User` tasks never move.
    pub auto_priority: bool,
}

impl TaskSpec {
    pub fn new(kind: &str, label: impl Into<String>, resources: Resources) -> Self {
        TaskSpec {
            kind: kind.into(),
            label: label.into(),
            original_id: None,
            version_id: None,
            source_id: None,
            item_id: None,
            priority: Priority::User,
            resources,
            preemptible: false,
            coalesce_key: None,
            visible: false,
            cancel_on_blur: false,
            auto_priority: false,
        }
    }
    pub fn original(mut self, id: impl Into<String>) -> Self {
        self.original_id = Some(id.into());
        self
    }
    pub fn version(mut self, id: impl Into<String>) -> Self {
        self.version_id = Some(id.into());
        self
    }
    pub fn priority(mut self, p: Priority) -> Self {
        self.priority = p;
        self
    }
    pub fn preemptible(mut self) -> Self {
        self.preemptible = true;
        self
    }
    pub fn coalesce(mut self, key: impl Into<String>) -> Self {
        self.coalesce_key = Some(key.into());
        self
    }
    pub fn visible(mut self) -> Self {
        self.visible = true;
        self
    }
    pub fn cancel_on_blur(mut self) -> Self {
        self.cancel_on_blur = true;
        self
    }
    pub fn auto_priority(mut self) -> Self {
        self.auto_priority = true;
        self
    }
}

/// The job as the UI sees it (docs/v2/api-contract.md, 3.1).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub priority: Priority,
    pub state: JobState,
    pub status: &'static str,
    pub progress: f64,
    pub message: String,
    pub resources: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub failed_usb: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_sent: Option<u64>,
    pub started_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running_since: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(skip)]
    pub visible: bool,
    #[serde(skip)]
    pub full_result: Option<Value>,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn new_job_id() -> String {
    use std::sync::atomic::AtomicU64;
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) as u64;
    let mut x = t ^ (n.wrapping_mul(0x9e37_79b9_7f4a_7c15)) ^ (std::process::id() as u64) << 32;
    // splitmix64
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    format!("{:012x}", x & 0xffff_ffff_ffff)
}

// ---------- events ----------

pub trait JobSink: Send + Sync + 'static {
    /// Called on creation, every state change and (throttled) progress of visible jobs.
    fn job(&self, job: &Job);
    /// A device task started (true) or none is running or queued any more (false).
    fn device_busy(&self, _busy: bool) {}
}

pub struct NoSink;
impl JobSink for NoSink {
    fn job(&self, _: &Job) {}
}

// ---------- cancellation and child processes ----------

#[derive(Default)]
struct CancelInner {
    flag: AtomicBool,
    reason: Mutex<Option<CoreError>>,
    children: Mutex<Vec<Weak<Mutex<Child>>>>,
}

#[derive(Clone, Default)]
pub struct CancelToken(Arc<CancelInner>);

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }
    fn cancel(&self, reason: CoreError) {
        {
            let mut r = self.0.reason.lock().unwrap();
            if r.is_none() {
                *r = Some(reason);
            }
        }
        self.0.flag.store(true, Ordering::SeqCst);
        for w in self.0.children.lock().unwrap().iter() {
            if let Some(c) = w.upgrade() {
                let _ = c.lock().unwrap().kill();
            }
        }
    }
    fn reason(&self) -> CoreError {
        self.0.reason.lock().unwrap().clone().unwrap_or_else(CoreError::cancelled)
    }
    fn reset(&self) {
        self.0.flag.store(false, Ordering::SeqCst);
        *self.0.reason.lock().unwrap() = None;
        self.0.children.lock().unwrap().clear();
    }
}

/// A child process started through `TaskCtx::spawn`; it is killed when the task is cancelled (or dropped).
pub struct ChildGuard {
    child: Arc<Mutex<Child>>,
    token: CancelToken,
}

impl ChildGuard {
    pub fn id(&self) -> u32 {
        self.child.lock().unwrap().id()
    }
    /// Take the child's stdout / stderr pipes (if they were piped).
    pub fn take_stdout(&self) -> Option<std::process::ChildStdout> {
        self.child.lock().unwrap().stdout.take()
    }
    pub fn take_stderr(&self) -> Option<std::process::ChildStderr> {
        self.child.lock().unwrap().stderr.take()
    }
    /// Wait for exit. Returns `cancelled` if the task was cancelled (the process has been killed).
    pub fn wait(&self) -> CoreResult<ExitStatus> {
        loop {
            if let Some(st) = self.child.lock().unwrap().try_wait()? {
                if self.token.is_cancelled() {
                    return Err(self.token.reason());
                }
                return Ok(st);
            }
            if self.token.is_cancelled() {
                let _ = self.child.lock().unwrap().kill();
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    }
    pub fn kill(&self) {
        let _ = self.child.lock().unwrap().kill();
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let mut c = self.child.lock().unwrap();
        if let Ok(None) = c.try_wait() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Output of `TaskCtx::run`.
pub struct RunOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Handed to every running task.
pub struct TaskCtx {
    id: String,
    token: CancelToken,
    sched: Weak<Shared>,
}

impl TaskCtx {
    pub fn job_id(&self) -> &str {
        &self.id
    }
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
    /// `Err(cancelled)` once the task has been cancelled: call between steps.
    pub fn check(&self) -> CoreResult<()> {
        if self.is_cancelled() {
            Err(self.token.reason())
        } else {
            Ok(())
        }
    }
    pub fn progress(&self, f: f64, msg: &str) {
        if let Some(s) = self.sched.upgrade() {
            s.progress(&self.id, f, Some(msg), None);
        }
    }
    /// Progress with byte counts (screen uploads).
    pub fn progress_bytes(&self, f: f64, msg: &str, sent: u64, total: u64) {
        if let Some(s) = self.sched.upgrade() {
            s.progress(&self.id, f, Some(msg), Some((sent, total)));
        }
    }
    /// Start a process that is killed if this task is cancelled. No console window on Windows.
    pub fn spawn(&self, cmd: &mut Command) -> CoreResult<ChildGuard> {
        self.check()?;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000 | 0x0000_4000); // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS (keeps the UI and uploads smooth)
        }
        let spawned = cmd.spawn()?;
        super::jobobject::adopt(&spawned); // never outlives the app
        let child = Arc::new(Mutex::new(spawned));
        self.token.0.children.lock().unwrap().push(Arc::downgrade(&child));
        let g = ChildGuard { child, token: self.token.clone() };
        if self.is_cancelled() {
            g.kill();
            return Err(self.token.reason());
        }
        Ok(g)
    }
    /// Run a process to completion, collecting stdout and stderr; killed on cancel.
    pub fn run(&self, cmd: &mut Command) -> CoreResult<RunOutput> {
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let g = self.spawn(cmd)?;
        let mut so = g.take_stdout();
        let mut se = g.take_stderr();
        let t1 = std::thread::spawn(move || {
            let mut b = Vec::new();
            if let Some(s) = so.as_mut() {
                let _ = s.read_to_end(&mut b);
            }
            b
        });
        let t2 = std::thread::spawn(move || {
            let mut b = Vec::new();
            if let Some(s) = se.as_mut() {
                let _ = s.read_to_end(&mut b);
            }
            b
        });
        let status = g.wait();
        let stdout = t1.join().unwrap_or_default();
        let stderr = t2.join().unwrap_or_default();
        Ok(RunOutput { status: status?, stdout, stderr })
    }
    /// Sleep that wakes early (with `cancelled`) when the task is cancelled.
    pub fn sleep(&self, d: Duration) -> CoreResult<()> {
        let end = Instant::now() + d;
        while Instant::now() < end {
            self.check()?;
            std::thread::sleep((end - Instant::now()).min(Duration::from_millis(20)));
        }
        self.check()
    }
}

// ---------- handles ----------

type Outcome = CoreResult<Value>;

#[derive(Default)]
struct Slot {
    v: Mutex<Option<Outcome>>,
    cv: Condvar,
}

/// Returned by `submit`: the job id and a way to wait for the result.
#[derive(Clone)]
pub struct JobHandle {
    pub id: String,
    slot: Arc<Slot>,
}

impl JobHandle {
    /// Block until the job finishes.
    pub fn wait(&self) -> Outcome {
        let mut g = self.slot.v.lock().unwrap();
        while g.is_none() {
            g = self.slot.cv.wait(g).unwrap();
        }
        g.clone().unwrap()
    }
    pub fn wait_timeout(&self, d: Duration) -> Option<Outcome> {
        let end = Instant::now() + d;
        let mut g = self.slot.v.lock().unwrap();
        while g.is_none() {
            let now = Instant::now();
            if now >= end {
                return None;
            }
            g = self.slot.cv.wait_timeout(g, end - now).unwrap().0;
        }
        g.clone()
    }
    pub fn try_result(&self) -> Option<Outcome> {
        self.slot.v.lock().unwrap().clone()
    }
}

// ---------- scheduler ----------

pub type TaskFn = Arc<dyn Fn(&TaskCtx) -> CoreResult<Value> + Send + Sync>;

struct Entry {
    seq: u64,
    spec: TaskSpec,
    f: TaskFn,
    token: CancelToken,
    slot: Arc<Slot>,
    job: Job,
    last_emit: Option<Instant>,
    /// Running: start order (preemption picks the youngest).
    started_seq: u64,
    /// Running and asked to make room: re-queue instead of finishing as cancelled.
    preempted: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// 1.2 behaviour (fallback): while a device task is running or queued, cpu/gpu work does not start, and the
    /// device task waits for running cpu/gpu work (1.2 saw an encode stall an upload). Default off
    /// (decision 2026-10-11: allow media during uploads with a reduced cpu cap; to be confirmed on hardware).
    pub device_blocks_media: bool,
    /// While a device task runs (and media is allowed), at most this many cpu units are in use by new starts.
    pub cpu_during_device: u32,
}

impl Default for Policy {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4);
        Policy { device_blocks_media: false, cpu_during_device: (cores / 4).max(1) }
    }
}

impl Policy {
    /// The 1.2 full block.
    pub fn blocking() -> Self {
        Policy { device_blocks_media: true, ..Policy::default() }
    }
}

struct Inner {
    limits: Limits,
    policy: Policy,
    in_use: [u32; N_RES],
    seq: u64,
    start_seq: u64,
    queued: Vec<String>,
    running: Vec<String>,
    entries: HashMap<String, Entry>,
    /// Finished jobs kept for `activity` / `job_get` (newest last), at most KEEP_FINISHED.
    finished: Vec<Job>,
    device_busy_reported: bool,
    /// Finished jobs whose waiters are woken by `emit`, right after their final event was sent.
    wakes: Vec<(String, Arc<Slot>, Outcome)>,
}

const KEEP_FINISHED: usize = 100;
const EMIT_EVERY: Duration = Duration::from_millis(100);

struct Shared {
    inner: Mutex<Inner>,
    sink: Arc<dyn JobSink>,
    me: Mutex<Weak<Shared>>,
}

#[derive(Clone)]
pub struct Scheduler(Arc<Shared>);

impl Scheduler {
    pub fn new(limits: Limits, policy: Policy, sink: Arc<dyn JobSink>) -> Scheduler {
        let s = Arc::new(Shared {
            inner: Mutex::new(Inner {
                limits,
                policy,
                in_use: [0; N_RES],
                seq: 0,
                start_seq: 0,
                queued: vec![],
                running: vec![],
                entries: HashMap::new(),
                finished: vec![],
                device_busy_reported: false,
                wakes: vec![],
            }),
            sink,
            me: Mutex::new(Weak::new()),
        });
        *s.me.lock().unwrap() = Arc::downgrade(&s);
        Scheduler(s)
    }

    pub fn set_limits(&self, limits: Limits) {
        self.0.inner.lock().unwrap().limits = limits;
        self.0.pump();
    }
    pub fn set_policy(&self, policy: Policy) {
        self.0.inner.lock().unwrap().policy = policy;
        self.0.pump();
    }
    pub fn policy(&self) -> Policy {
        self.0.inner.lock().unwrap().policy
    }

    /// Queue a task. It runs when its resources are free and nothing more important is waiting for them.
    pub fn submit(&self, spec: TaskSpec, f: impl Fn(&TaskCtx) -> CoreResult<Value> + Send + Sync + 'static) -> JobHandle {
        self.submit_arc(spec, Arc::new(f))
    }

    pub fn submit_arc(&self, spec: TaskSpec, f: TaskFn) -> JobHandle {
        let id = new_job_id();
        let slot = Arc::new(Slot::default());
        let mut emits = vec![];
        {
            let mut g = self.0.inner.lock().unwrap();
            // Coalesce: drop queued tasks with the same key, cancel a running one.
            if let Some(key) = &spec.coalesce_key {
                let old: Vec<String> = g
                    .entries
                    .iter()
                    .filter(|(_, e)| e.spec.coalesce_key.as_deref() == Some(key))
                    .map(|(k, _)| k.clone())
                    .collect();
                for oid in old {
                    if let Some(j) = Shared::cancel_locked(&mut g, &oid, CoreError::superseded()) {
                        emits.push(j);
                    }
                }
            }
            g.seq += 1;
            let seq = g.seq;
            let job = Job {
                id: id.clone(),
                kind: spec.kind.clone(),
                label: spec.label.clone(),
                original_id: spec.original_id.clone(),
                version_id: spec.version_id.clone(),
                source_id: spec.source_id.clone(),
                item_id: spec.item_id.clone(),
                priority: spec.priority,
                state: JobState::Queued,
                status: JobState::Queued.legacy(),
                progress: 0.0,
                message: String::new(),
                resources: spec.resources.names(),
                error: None,
                code: None,
                failed_usb: false,
                result: None,
                bytes_total: None,
                bytes_sent: None,
                started_at: now_ms(),
                running_since: None,
                finished_at: None,
                visible: spec.visible,
                full_result: None,
            };
            emits.push(job.clone());
            g.entries.insert(
                id.clone(),
                Entry {
                    seq,
                    spec,
                    f,
                    token: CancelToken::default(),
                    slot: slot.clone(),
                    job,
                    last_emit: None,
                    started_seq: 0,
                    preempted: false,
                },
            );
            g.queued.push(id.clone());
        }
        for j in emits {
            self.0.emit(&j);
        }
        self.0.pump();
        JobHandle { id, slot }
    }

    /// Cancel one job. Running device work cannot be cancelled (an upload must not be cut off); returns false then.
    pub fn cancel(&self, id: &str) -> bool {
        let j = {
            let mut g = self.0.inner.lock().unwrap();
            Shared::cancel_locked(&mut g, id, CoreError::cancelled())
        };
        let ok = j.is_some();
        if let Some(j) = j {
            self.0.emit(&j);
        }
        self.0.pump();
        ok
    }

    /// Cancel every job matching `pred` (by clip, by version, ...). Returns how many were cancelled.
    pub fn cancel_where(&self, pred: impl Fn(&TaskSpec) -> bool) -> usize {
        let js = {
            let mut g = self.0.inner.lock().unwrap();
            let ids: Vec<String> = g.entries.iter().filter(|(_, e)| pred(&e.spec)).map(|(k, _)| k.clone()).collect();
            ids.iter().filter_map(|id| Shared::cancel_locked(&mut g, id, CoreError::cancelled())).collect::<Vec<_>>()
        };
        for j in &js {
            self.0.emit(j);
        }
        self.0.pump();
        js.len()
    }

    pub fn cancel_original(&self, oid: &str) -> usize {
        self.cancel_where(|s| s.original_id.as_deref() == Some(oid))
    }
    pub fn cancel_version(&self, vid: &str) -> usize {
        self.cancel_where(|s| s.version_id.as_deref() == Some(vid))
    }

    /// The user selected `selected` (or nothing) and sees `visible` in the gallery: re-rank auto-priority work and
    /// cancel blur-cancellable work (estimates) of clips that are no longer selected.
    pub fn set_focus(&self, selected: Option<&str>, visible: &[String]) {
        let mut emits = vec![];
        {
            let mut g = self.0.inner.lock().unwrap();
            let ids: Vec<String> = g.entries.keys().cloned().collect();
            for id in ids {
                let (oid, blur, auto) = {
                    let e = &g.entries[&id];
                    (e.spec.original_id.clone(), e.spec.cancel_on_blur, e.spec.auto_priority)
                };
                let Some(oid) = oid else { continue };
                let is_sel = selected == Some(oid.as_str());
                if blur && !is_sel {
                    if let Some(j) = Shared::cancel_locked(&mut g, &id, CoreError::cancelled()) {
                        emits.push(j);
                    }
                    continue;
                }
                if auto {
                    let p = if is_sel {
                        Priority::Selected
                    } else if visible.iter().any(|v| v == &oid) {
                        Priority::Visible
                    } else {
                        Priority::Background
                    };
                    let e = g.entries.get_mut(&id).unwrap();
                    if e.spec.priority != p {
                        e.spec.priority = p;
                        e.job.priority = p;
                    }
                }
            }
        }
        for j in &emits {
            self.0.emit(j);
        }
        self.0.pump();
    }

    /// A queued or running job submitted with this coalesce key (to join it instead of superseding it).
    pub fn live_with_key(&self, key: &str) -> Option<String> {
        let g = self.0.inner.lock().unwrap();
        g.entries.iter().find(|(_, e)| e.spec.coalesce_key.as_deref() == Some(key)).map(|(id, _)| id.clone())
    }

    pub fn job(&self, id: &str) -> Option<Job> {
        let g = self.0.inner.lock().unwrap();
        if let Some(e) = g.entries.get(id) {
            return Some(e.job.clone());
        }
        g.finished.iter().rev().find(|j| j.id == id).cloned()
    }

    /// Visible jobs that are queued/running or finished in the last `keep_ms`.
    pub fn activity(&self, keep_ms: u64) -> Vec<Job> {
        let g = self.0.inner.lock().unwrap();
        let now = now_ms();
        let mut out: Vec<(u64, Job)> = g
            .finished
            .iter()
            .filter(|j| j.visible && now.saturating_sub(j.finished_at.unwrap_or(0)) < keep_ms)
            .map(|j| (0, j.clone()))
            .collect();
        let mut live: Vec<(u64, Job)> =
            g.entries.values().filter(|e| e.spec.visible).map(|e| (e.seq, e.job.clone())).collect();
        live.sort_by_key(|x| x.0);
        out.extend(live);
        out.into_iter().map(|x| x.1).collect()
    }

    /// True while a device task is running or queued.
    pub fn device_active(&self) -> bool {
        Shared::device_active(&self.0.inner.lock().unwrap())
    }

    /// Any job the user can see is queued or running (upload, conversion, store, recording): not a moment to quit from a script.
    pub fn any_visible_job(&self) -> bool {
        self.0.inner.lock().unwrap().entries.values().any(|e| e.spec.visible)
    }

    /// True while a visible job of one of these kinds is running (close guard).
    pub fn running_kind(&self, kinds: &[&str]) -> bool {
        let g = self.0.inner.lock().unwrap();
        g.running.iter().any(|id| kinds.contains(&g.entries[id].spec.kind.as_str()))
    }

    /// Wait until no task is running (or, with `ignore_device`, only device tasks are), at most `max`.
    /// Returns true if that state was reached.
    pub fn wait_idle(&self, max: Duration, ignore_device: bool) -> bool {
        let end = Instant::now() + max;
        loop {
            let busy = {
                let g = self.0.inner.lock().unwrap();
                g.running.iter().any(|id| !(ignore_device && g.entries[id].spec.resources.get(Res::Device) > 0))
            };
            if !busy {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Number of queued and running tasks (tests, diagnostics).
    pub fn counts(&self) -> (usize, usize) {
        let g = self.0.inner.lock().unwrap();
        (g.queued.len(), g.running.len())
    }
}

impl Shared {
    fn device_active(g: &Inner) -> bool {
        g.entries.values().any(|e| e.spec.resources.get(Res::Device) > 0)
    }

    fn device_running(g: &Inner) -> bool {
        g.running.iter().any(|id| g.entries[id].spec.resources.get(Res::Device) > 0)
    }

    /// Every job state change goes through here. A finished job's waiters are woken only after its final event
    /// was sent, so whoever waits on a job has always seen that event first.
    fn emit(&self, j: &Job) {
        if j.visible {
            self.sink.job(j);
        }
        if j.state.finished() {
            let w = {
                let mut g = self.inner.lock().unwrap();
                g.wakes.iter().position(|(id, _, _)| id == &j.id).map(|i| g.wakes.swap_remove(i))
            };
            if let Some((_, slot, r)) = w {
                *slot.v.lock().unwrap() = Some(r);
                slot.cv.notify_all();
            }
        }
    }

    /// Cancel under the lock. Queued: finished immediately. Running: token set (children killed); finishes when the
    /// thread returns. Running device tasks are not cancellable. Returns the job to emit.
    fn cancel_locked(g: &mut Inner, id: &str, reason: CoreError) -> Option<Job> {
        let e = g.entries.get(id)?;
        if g.queued.iter().any(|x| x == id) {
            g.queued.retain(|x| x != id);
            let e = g.entries.remove(id).unwrap();
            return Some(Self::finish_entry(g, e, Err(reason)));
        }
        if e.spec.resources.get(Res::Device) > 0 {
            return None;
        }
        if e.token.is_cancelled() {
            // Being preempted (it would be re-queued): turn that into a real cancel. Already cancelling: nothing to do.
            if !e.preempted {
                return None;
            }
            let e = g.entries.get_mut(id).unwrap();
            e.preempted = false;
            *e.token.0.reason.lock().unwrap() = Some(reason);
            e.job.message = "Cancelling".into();
            return Some(e.job.clone());
        }
        let e = g.entries.get_mut(id).unwrap();
        e.preempted = false;
        e.token.cancel(reason);
        e.job.message = "Cancelling".into();
        Some(e.job.clone())
    }

    fn finish_entry(g: &mut Inner, e: Entry, r: Outcome) -> Job {
        let mut job = e.job;
        job.finished_at = Some(now_ms());
        match &r {
            Ok(v) => {
                job.state = JobState::Done;
                job.progress = 1.0;
                job.full_result = Some(v.clone());
                job.result = Some(public_result(&job.kind, v));
            }
            Err(err) => {
                job.state = if err.is_cancelled() { JobState::Cancelled } else { JobState::Failed };
                job.error = Some(err.message.clone());
                job.code = err.code.clone();
                job.failed_usb = err.failed_usb;
            }
        }
        job.status = job.state.legacy();
        g.wakes.push((job.id.clone(), e.slot.clone(), r));
        g.finished.push(job.clone());
        if g.finished.len() > KEEP_FINISHED {
            let n = g.finished.len() - KEEP_FINISHED;
            g.finished.drain(..n);
        }
        job
    }

    fn progress(&self, id: &str, f: f64, msg: Option<&str>, bytes: Option<(u64, u64)>) {
        let emit = {
            let mut g = self.inner.lock().unwrap();
            let Some(e) = g.entries.get_mut(id) else { return };
            if e.token.is_cancelled() {
                return;
            }
            e.job.progress = f.clamp(0.0, 1.0);
            if let Some(m) = msg {
                e.job.message = m.to_string();
            }
            if let Some((a, b)) = bytes {
                e.job.bytes_sent = Some(a);
                e.job.bytes_total = Some(b);
            }
            let due = e.last_emit.is_none_or(|t| t.elapsed() >= EMIT_EVERY) || f >= 1.0;
            if due {
                e.last_emit = Some(Instant::now());
                Some(e.job.clone())
            } else {
                None
            }
        };
        if let Some(j) = emit {
            self.emit(&j);
        }
    }

    fn can_start(g: &Inner, spec: &TaskSpec) -> bool {
        let r = &spec.resources;
        for i in 0..N_RES {
            let (want, lim) = (r.0[i], g.limits.0[i].max(1));
            // A task asking for more than the whole class runs alone in it (instead of never).
            if want > 0 && (if want > lim { g.in_use[i] > 0 } else { g.in_use[i] + want > lim }) {
                return false;
            }
        }
        let dev = r.get(Res::Device) > 0;
        if g.policy.device_blocks_media {
            if dev && (g.in_use[Res::Cpu as usize] > 0 || g.in_use[Res::Gpu as usize] > 0) {
                return false;
            }
            if r.media() && Self::device_active(g) && !dev {
                return false;
            }
        } else if !dev && r.get(Res::Cpu) > 0 && Self::device_running(g) {
            let cap = g.policy.cpu_during_device.max(1).min(g.limits.0[Res::Cpu as usize].max(1));
            let (want, used) = (r.get(Res::Cpu), g.in_use[Res::Cpu as usize]);
            if if want > cap { used > 0 } else { used + want > cap } {
                return false;
            }
        }
        true
    }

    fn waiting_message(g: &Inner, spec: &TaskSpec) -> &'static str {
        let dev_ahead = g.entries.values().any(|e| e.spec.resources.get(Res::Device) > 0);
        if spec.resources.get(Res::Device) > 0 {
            if g.running.iter().any(|id| g.entries[id].spec.resources.get(Res::Device) > 0) {
                "Waiting for the screen operation to finish…"
            } else {
                "Waiting for conversions to finish…"
            }
        } else if dev_ahead && spec.resources.media() && (g.policy.device_blocks_media || Self::device_running(g)) {
            "Waiting for the screen operation to finish…"
        } else {
            "Waiting for the previous conversion to finish…"
        }
    }

    /// Start everything that can start, highest priority first (no head-of-line blocking), and preempt background
    /// work when a selected/user task is starved of cpu/gpu.
    fn pump(&self) {
        let mut starts: Vec<(String, TaskFn, CancelToken)> = vec![];
        let mut emits = vec![];
        let device_change;
        {
            let mut g = self.inner.lock().unwrap();
            let mut order = g.queued.clone();
            order.sort_by_key(|id| {
                let e = &g.entries[id];
                (e.spec.priority, e.seq)
            });
            // Classes a more important waiting task is waiting for: lower-priority tasks may not take them
            // (otherwise a stream of small background tasks could starve a big selected one).
            let mut reserved = [false; N_RES];
            let mut starved: Option<(Priority, Resources)> = None;
            for id in order {
                let spec = g.entries[&id].spec.clone();
                let blocked_by_reserve = (0..N_RES).any(|i| spec.resources.0[i] > 0 && reserved[i]);
                if !blocked_by_reserve && Self::can_start(&g, &spec) {
                    for i in 0..N_RES {
                        g.in_use[i] += spec.resources.0[i];
                    }
                    g.queued.retain(|x| x != &id);
                    g.running.push(id.clone());
                    g.start_seq += 1;
                    let ss = g.start_seq;
                    let e = g.entries.get_mut(&id).unwrap();
                    e.started_seq = ss;
                    e.job.state = JobState::Running;
                    e.job.status = JobState::Running.legacy();
                    e.job.message.clear();
                    e.job.running_since = Some(now_ms());
                    emits.push(e.job.clone());
                    starts.push((id.clone(), e.f.clone(), e.token.clone()));
                } else {
                    for i in 0..N_RES {
                        if spec.resources.0[i] > 0 {
                            reserved[i] = true;
                        }
                    }
                    if spec.priority <= Priority::Selected && starved.is_none() {
                        starved = Some((spec.priority, spec.resources));
                    }
                    let msg = Self::waiting_message(&g, &spec);
                    let e = g.entries.get_mut(&id).unwrap();
                    if e.job.message != msg {
                        e.job.message = msg.into();
                        emits.push(e.job.clone());
                    }
                }
            }
            // Preemption: free cpu/gpu held by preemptible lower-priority work for a starved important task.
            if let Some((p, res)) = starved {
                let dev = res.get(Res::Device) > 0 && g.policy.device_blocks_media;
                let wants = |e: &Entry| {
                    let r = &e.spec.resources;
                    (res.get(Res::Cpu) > 0 || dev) && r.get(Res::Cpu) > 0 || (res.get(Res::Gpu) > 0 || dev) && r.get(Res::Gpu) > 0
                };
                let already = g.running.iter().any(|id| g.entries[id].preempted);
                if !already {
                    let victim = g
                        .running
                        .iter()
                        .filter(|id| {
                            let e = &g.entries[*id];
                            e.spec.preemptible && e.spec.priority > p && !e.token.is_cancelled() && wants(e)
                        })
                        .max_by_key(|id| g.entries[*id].started_seq)
                        .cloned();
                    if let Some(v) = victim {
                        let e = g.entries.get_mut(&v).unwrap();
                        e.token.cancel(CoreError::coded("preempted", "Paused for more important work."));
                        e.preempted = true;
                    }
                }
            }
            let busy = Self::device_active(&g);
            device_change = if busy != g.device_busy_reported {
                g.device_busy_reported = busy;
                if busy {
                    // why live is about to step aside (device.log)
                    let kinds: Vec<String> = g.entries.values().filter(|e| e.spec.resources.get(Res::Device) > 0).map(|e| format!("{} \"{}\"", e.spec.kind, e.spec.label)).collect();
                    crate::device::log(&format!("device task: {} (live steps aside until it is done)", kinds.join(", ")));
                } else {
                    crate::device::log("device tasks: none left");
                }
                Some(busy)
            } else {
                None
            };
        }
        for j in &emits {
            self.emit(j);
        }
        if let Some(b) = device_change {
            self.sink.device_busy(b);
        }
        let me = self.me.lock().unwrap().clone();
        for (id, f, token) in starts {
            let me = me.clone();
            std::thread::Builder::new()
                .name(format!("job-{id}"))
                .spawn(move || {
                    let ctx = TaskCtx { id: id.clone(), token: token.clone(), sched: me.clone() };
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&ctx)))
                        .unwrap_or_else(|_| Err(CoreError::new("Internal error (the task crashed).").status(500)));
                    // A task that was cancelled reports `cancelled`, whatever it returned on its way out.
                    let r = if token.is_cancelled() && r.as_ref().is_err() { Err(token.reason()) } else { r };
                    if let Some(s) = me.upgrade() {
                        s.task_done(&id, r);
                    }
                })
                .expect("spawn job thread");
        }
    }

    fn task_done(&self, id: &str, r: Outcome) {
        let job = {
            let mut g = self.inner.lock().unwrap();
            g.running.retain(|x| x != id);
            let Some(mut e) = g.entries.remove(id) else { return };
            for i in 0..N_RES {
                g.in_use[i] -= e.spec.resources.0[i];
            }
            if e.preempted && r.is_err() {
                // Re-queue with the same id and position; it starts again when there is room.
                e.preempted = false;
                e.token.reset();
                e.job.state = JobState::Queued;
                e.job.status = JobState::Queued.legacy();
                e.job.progress = 0.0;
                e.job.message = "Paused for more important work".into();
                e.job.running_since = None;
                let j = e.job.clone();
                g.queued.push(id.to_string());
                g.entries.insert(id.to_string(), e);
                j
            } else {
                Self::finish_entry(&mut g, e, r)
            }
        };
        self.emit(&job);
        self.pump();
    }
}

/// 1.2 `publicJob`: loop searches and source jobs return their whole result; others a small summary.
fn public_result(kind: &str, v: &Value) -> Value {
    if kind == "loopsearch" || kind == "source" || kind == "apply" || !v.is_object() {
        return v.clone();
    }
    json!({
        "warning": v.get("warning"),
        "serialWarning": v.get("serialWarning"),
        "name": v.get("name"),
        "label": v.get("label"),
        "restarted": v.get("restarted").and_then(Value::as_bool).unwrap_or(false),
        "restartFailed": v.get("restartFailed").cloned().unwrap_or(Value::Null),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;

    fn sched(limits: [u32; N_RES]) -> Scheduler {
        Scheduler::new(Limits(limits), Policy::default(), Arc::new(NoSink))
    }

    /// A task that blocks until released (or cancelled), recording its start in `log`.
    fn gate_task(
        name: &'static str,
        log: Arc<Mutex<Vec<&'static str>>>,
        release: Arc<AtomicBool>,
    ) -> impl Fn(&TaskCtx) -> CoreResult<Value> + Send + Sync {
        move |ctx| {
            log.lock().unwrap().push(name);
            while !release.load(Ordering::SeqCst) {
                ctx.sleep(Duration::from_millis(5))?;
            }
            Ok(json!(name))
        }
    }

    fn wait_until(mut f: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !f() {
            assert!(Instant::now() < end, "timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn runs_and_returns_result() {
        let s = sched([2, 2, 1, 4, 1, 8]);
        let h = s.submit(TaskSpec::new("t", "t", Resources::one(Res::Cpu)), |_| Ok(json!(42)));
        assert_eq!(h.wait().unwrap(), json!(42));
        assert_eq!(s.job(&h.id).unwrap().state, JobState::Done);
    }

    #[test]
    fn respects_class_limits() {
        let s = sched([2, 2, 1, 4, 1, 8]);
        let peak = Arc::new(AtomicUsize::new(0));
        let cur = Arc::new(AtomicUsize::new(0));
        let hs: Vec<_> = (0..8)
            .map(|_| {
                let (peak, cur) = (peak.clone(), cur.clone());
                s.submit(TaskSpec::new("t", "t", Resources::one(Res::Cpu)), move |_| {
                    let n = cur.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(n, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(20));
                    cur.fetch_sub(1, Ordering::SeqCst);
                    Ok(Value::Null)
                })
            })
            .collect();
        for h in hs {
            h.wait().unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn higher_priority_runs_first() {
        let s = sched([1, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let rel = Arc::new(AtomicBool::new(false));
        let blocker = s.submit(TaskSpec::new("t", "blocker", Resources::one(Res::Cpu)), gate_task("blocker", log.clone(), rel.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        let order = Arc::new(Mutex::new(vec![]));
        let mk = |name: &'static str, p: Priority| {
            let order = order.clone();
            s.submit(TaskSpec::new("t", name, Resources::one(Res::Cpu)).priority(p), move |_| {
                order.lock().unwrap().push(name);
                Ok(Value::Null)
            })
        };
        let a = mk("bg", Priority::Background);
        let b = mk("vis", Priority::Visible);
        let c = mk("sel", Priority::Selected);
        let d = mk("user", Priority::User);
        rel.store(true, Ordering::SeqCst);
        for h in [blocker, a, b, c, d] {
            h.wait().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec!["user", "sel", "vis", "bg"]);
    }

    #[test]
    fn no_head_of_line_blocking() {
        // cpu is full; a queued cpu task must not stop a net task from starting.
        let s = sched([1, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let rel = Arc::new(AtomicBool::new(false));
        let a = s.submit(TaskSpec::new("t", "a", Resources::one(Res::Cpu)), gate_task("a", log.clone(), rel.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        let b = s.submit(TaskSpec::new("t", "b", Resources::one(Res::Cpu)).priority(Priority::Selected), gate_task("b", log.clone(), rel.clone()));
        let c = s.submit(TaskSpec::new("t", "c", Resources::one(Res::Net)).priority(Priority::Background), |_| Ok(json!("c")));
        assert_eq!(c.wait().unwrap(), json!("c"));
        assert_eq!(*log.lock().unwrap(), vec!["a"]);
        rel.store(true, Ordering::SeqCst);
        a.wait().unwrap();
        b.wait().unwrap();
    }

    #[test]
    fn cancel_queued_and_running() {
        let s = sched([1, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let rel = Arc::new(AtomicBool::new(false));
        let a = s.submit(TaskSpec::new("t", "a", Resources::one(Res::Cpu)).original("o1"), gate_task("a", log.clone(), rel.clone()));
        let b = s.submit(TaskSpec::new("t", "b", Resources::one(Res::Cpu)).original("o1"), gate_task("b", log.clone(), rel.clone()));
        let c = s.submit(TaskSpec::new("t", "c", Resources::one(Res::Cpu)).original("o2"), |_| Ok(json!("c")));
        wait_until(|| log.lock().unwrap().len() == 1);
        assert_eq!(s.cancel_original("o1"), 2);
        assert!(a.wait().unwrap_err().code_is("cancelled"));
        assert!(b.wait().unwrap_err().code_is("cancelled"));
        assert_eq!(c.wait().unwrap(), json!("c"));
        assert_eq!(*log.lock().unwrap(), vec!["a"], "b never started");
        assert_eq!(s.job(&a.id).unwrap().state, JobState::Cancelled);
        assert_eq!(s.job(&a.id).unwrap().status, "error");
    }

    #[test]
    fn cancel_kills_child_process() {
        let s = sched([1, 2, 1, 4, 1, 8]);
        let (tx, rx) = mpsc::channel();
        let h = s.submit(TaskSpec::new("t", "sleeper", Resources::one(Res::Cpu)).version("v1"), move |ctx| {
            // A process that would run for 30 s.
            #[cfg(windows)]
            let mut cmd = {
                let mut c = Command::new("powershell");
                c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
                c
            };
            #[cfg(not(windows))]
            let mut cmd = {
                let mut c = Command::new("sleep");
                c.arg("30");
                c
            };
            let g = ctx.spawn(&mut cmd)?;
            tx.send(g.id()).unwrap();
            g.wait()?;
            Ok(Value::Null)
        });
        let pid = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let t0 = Instant::now();
        assert_eq!(s.cancel_version("v1"), 1);
        let e = h.wait().unwrap_err();
        assert!(e.code_is("cancelled"));
        assert!(t0.elapsed() < Duration::from_secs(15), "child killed promptly (the child would otherwise sleep 30 s)");
        assert!(!process_alive(pid), "child process {pid} was killed");
    }

    fn process_alive(pid: u32) -> bool {
        #[cfg(windows)]
        {
            let out = Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
        }
        #[cfg(not(windows))]
        {
            Command::new("kill").args(["-0", &pid.to_string()]).status().map(|s| s.success()).unwrap_or(false)
        }
    }

    #[test]
    fn coalesces_superseded_requests() {
        let s = sched([1, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let rel = Arc::new(AtomicBool::new(false));
        let blocker = s.submit(TaskSpec::new("t", "x", Resources::one(Res::Cpu)), gate_task("x", log.clone(), rel.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        let est = |n: i64| s.submit(TaskSpec::new("estimate", "e", Resources::one(Res::Cpu)).coalesce("estimate:o1"), move |_| Ok(json!(n)));
        let e1 = est(1);
        let e2 = est(2);
        let e3 = est(3);
        rel.store(true, Ordering::SeqCst);
        blocker.wait().unwrap();
        assert!(e1.wait().unwrap_err().code_is("superseded"));
        assert!(e2.wait().unwrap_err().code_is("superseded"));
        assert_eq!(e3.wait().unwrap(), json!(3));
    }

    #[test]
    fn coalescing_cancels_running_older() {
        let s = sched([2, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let never = Arc::new(AtomicBool::new(false));
        let e1 = s.submit(TaskSpec::new("estimate", "e1", Resources::one(Res::Cpu)).coalesce("k"), gate_task("e1", log.clone(), never));
        wait_until(|| log.lock().unwrap().len() == 1);
        let e2 = s.submit(TaskSpec::new("estimate", "e2", Resources::one(Res::Cpu)).coalesce("k"), |_| Ok(json!(2)));
        assert!(e1.wait().unwrap_err().code_is("superseded"));
        assert_eq!(e2.wait().unwrap(), json!(2));
    }

    #[test]
    fn preempts_background_for_selected() {
        // Priority inversion: background work fills the cpu class; a selected task arrives. The youngest preemptible
        // background task is paused (re-queued) so the selected task starts without waiting for it.
        let s = sched([2, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let never = Arc::new(AtomicBool::new(false));
        let runs = Arc::new(AtomicUsize::new(0));
        let bg1 = s.submit(
            TaskSpec::new("cards", "bg1", Resources::one(Res::Cpu)).priority(Priority::Background).preemptible(),
            gate_task("bg1", log.clone(), Arc::new(AtomicBool::new(false))),
        );
        wait_until(|| log.lock().unwrap().len() == 1);
        let r2 = runs.clone();
        let rel2 = Arc::new(AtomicBool::new(false));
        let rel2c = rel2.clone();
        let bg2 = s.submit(
            TaskSpec::new("cards", "bg2", Resources::one(Res::Cpu)).priority(Priority::Background).preemptible(),
            move |ctx| {
                r2.fetch_add(1, Ordering::SeqCst);
                while !rel2c.load(Ordering::SeqCst) {
                    ctx.sleep(Duration::from_millis(5))?;
                }
                Ok(json!("bg2"))
            },
        );
        wait_until(|| runs.load(Ordering::SeqCst) == 1);
        let sel = s.submit(TaskSpec::new("estimate", "sel", Resources::one(Res::Cpu)).priority(Priority::Selected), |_| Ok(json!("sel")));
        assert_eq!(sel.wait_timeout(Duration::from_secs(5)).expect("selected not starved").unwrap(), json!("sel"));
        // bg2 (youngest) was paused and runs again; bg1 kept running.
        wait_until(|| runs.load(Ordering::SeqCst) == 2);
        assert_eq!(s.job(&bg2.id).unwrap().state, JobState::Running);
        assert_eq!(s.job(&bg1.id).unwrap().state, JobState::Running);
        rel2.store(true, Ordering::SeqCst);
        assert_eq!(bg2.wait().unwrap(), json!("bg2"));
        assert!(s.cancel(&bg1.id));
        let _ = never;
        assert!(bg1.wait().is_err());
    }

    #[test]
    fn cancelling_a_task_being_preempted_cancels_it() {
        let s = sched([1, 2, 1, 4, 1, 8]);
        let runs = Arc::new(AtomicUsize::new(0));
        let r2 = runs.clone();
        // A background task that takes 300 ms to stop after being asked (like a slow child process exit).
        let bg = s.submit(TaskSpec::new("cards", "bg", Resources::one(Res::Cpu)).priority(Priority::Background).preemptible(), move |ctx| {
            r2.fetch_add(1, Ordering::SeqCst);
            while !ctx.is_cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            std::thread::sleep(Duration::from_millis(300));
            ctx.check()?;
            Ok(Value::Null)
        });
        wait_until(|| runs.load(Ordering::SeqCst) == 1);
        let sel = s.submit(TaskSpec::new("estimate", "sel", Resources::one(Res::Cpu)).priority(Priority::Selected), |_| Ok(json!(1)));
        std::thread::sleep(Duration::from_millis(50)); // bg is now being preempted
        assert!(s.cancel(&bg.id), "a preempting task can still be cancelled");
        assert!(bg.wait().unwrap_err().code_is("cancelled"));
        sel.wait().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(runs.load(Ordering::SeqCst), 1, "not re-queued after the cancel");
        assert_eq!(s.job(&bg.id).unwrap().state, JobState::Cancelled);
    }

    #[test]
    fn non_preemptible_is_not_preempted() {
        let s = sched([1, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let rel = Arc::new(AtomicBool::new(false));
        let conv = s.submit(TaskSpec::new("version", "conv", Resources::one(Res::Cpu)).priority(Priority::Background), gate_task("conv", log.clone(), rel.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        let sel = s.submit(TaskSpec::new("estimate", "sel", Resources::one(Res::Cpu)).priority(Priority::Selected), |_| Ok(json!(1)));
        assert!(sel.wait_timeout(Duration::from_millis(150)).is_none());
        rel.store(true, Ordering::SeqCst);
        conv.wait().unwrap();
        sel.wait().unwrap();
    }

    #[test]
    fn reserved_class_not_starved_by_small_background_tasks() {
        // A selected task needing 2 cpu waits while 1 is busy; later background 1-cpu tasks must not jump in.
        let s = sched([2, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let rel = Arc::new(AtomicBool::new(false));
        let a = s.submit(TaskSpec::new("t", "a", Resources::one(Res::Cpu)), gate_task("a", log.clone(), rel.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        let big = s.submit(TaskSpec::new("t", "big", Resources::none().with(Res::Cpu, 2)).priority(Priority::Selected), gate_task("big", log.clone(), rel.clone()));
        let small = s.submit(TaskSpec::new("t", "small", Resources::one(Res::Cpu)).priority(Priority::Background), gate_task("small", log.clone(), rel.clone()));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(*log.lock().unwrap(), vec!["a"]);
        rel.store(true, Ordering::SeqCst);
        for h in [a, big, small] {
            h.wait().unwrap();
        }
        assert_eq!(*log.lock().unwrap(), vec!["a", "big", "small"]);
    }

    #[test]
    fn device_is_exclusive_and_blocks_media() {
        let s = Scheduler::new(Limits([4, 2, 1, 4, 1, 8]), Policy::blocking(), Arc::new(NoSink));
        let log = Arc::new(Mutex::new(vec![]));
        let rel_conv = Arc::new(AtomicBool::new(false));
        let rel_dev = Arc::new(AtomicBool::new(false));
        let conv = s.submit(TaskSpec::new("version", "conv", Resources::one(Res::Cpu)), gate_task("conv", log.clone(), rel_conv.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        let dev = s.submit(TaskSpec::new("send", "send", Resources::one(Res::Device)), gate_task("send", log.clone(), rel_dev.clone()));
        let dev2 = s.submit(TaskSpec::new("send", "send2", Resources::one(Res::Device)), gate_task("send2", log.clone(), rel_dev.clone()));
        let est = s.submit(TaskSpec::new("estimate", "est", Resources::one(Res::Cpu)).priority(Priority::Selected), gate_task("est", log.clone(), rel_dev.clone()));
        std::thread::sleep(Duration::from_millis(50));
        // The device waits for the running conversion; the estimate waits for the device.
        assert_eq!(*log.lock().unwrap(), vec!["conv"]);
        assert!(s.device_active());
        rel_conv.store(true, Ordering::SeqCst);
        conv.wait().unwrap();
        wait_until(|| log.lock().unwrap().len() == 2);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(*log.lock().unwrap(), vec!["conv", "send"], "one device task at a time, no media during it");
        rel_dev.store(true, Ordering::SeqCst);
        for h in [dev, dev2, est] {
            h.wait().unwrap();
        }
        assert_eq!(*log.lock().unwrap(), vec!["conv", "send", "send2", "est"]);
        assert!(!s.device_active());
    }

    #[test]
    fn media_during_upload_is_capped() {
        // Default policy: media may run during an upload, but new cpu work is capped to cpu_during_device.
        let pol = Policy { device_blocks_media: false, cpu_during_device: 1 };
        let s = Scheduler::new(Limits([4, 2, 1, 4, 1, 8]), pol, Arc::new(NoSink));
        let log = Arc::new(Mutex::new(vec![]));
        let rel_dev = Arc::new(AtomicBool::new(false));
        let rel = Arc::new(AtomicBool::new(false));
        let dev = s.submit(TaskSpec::new("send", "send", Resources::one(Res::Device)), gate_task("send", log.clone(), rel_dev.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        let a = s.submit(TaskSpec::new("estimate", "a", Resources::one(Res::Cpu)), gate_task("a", log.clone(), rel.clone()));
        let b = s.submit(TaskSpec::new("estimate", "b", Resources::one(Res::Cpu)), gate_task("b", log.clone(), rel.clone()));
        wait_until(|| log.lock().unwrap().len() == 2);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(*log.lock().unwrap(), vec!["send", "a"], "one cpu task while uploading");
        rel_dev.store(true, Ordering::SeqCst);
        dev.wait().unwrap();
        wait_until(|| log.lock().unwrap().len() == 3);
        rel.store(true, Ordering::SeqCst);
        a.wait().unwrap();
        b.wait().unwrap();
    }

    #[test]
    fn shutdown_wait_is_bounded_and_skips_uploads() {
        let s = sched([2, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let never = Arc::new(AtomicBool::new(false));
        let dev = s.submit(TaskSpec::new("send", "send", Resources::one(Res::Device)), gate_task("send", log.clone(), never.clone()));
        let cpu = s.submit(TaskSpec::new("version", "conv", Resources::one(Res::Cpu)), gate_task("conv", log.clone(), never.clone()));
        wait_until(|| log.lock().unwrap().len() == 2);
        s.cancel_where(|_| true);
        let t = Instant::now();
        assert!(s.wait_idle(Duration::from_secs(1), true), "only the upload is left");
        assert!(t.elapsed() < Duration::from_millis(500));
        assert!(cpu.wait().is_err());
        let t = Instant::now();
        assert!(!s.wait_idle(Duration::from_millis(200), false), "the upload keeps running");
        assert!(t.elapsed() < Duration::from_millis(400));
        never.store(true, Ordering::SeqCst);
        dev.wait().unwrap();
    }

    #[test]
    fn running_device_task_cannot_be_cancelled() {
        let s = sched([1, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let rel = Arc::new(AtomicBool::new(false));
        let d = s.submit(TaskSpec::new("send", "send", Resources::one(Res::Device)).version("v"), gate_task("send", log.clone(), rel.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        assert!(!s.cancel(&d.id));
        assert_eq!(s.cancel_version("v"), 0);
        rel.store(true, Ordering::SeqCst);
        assert!(d.wait().is_ok());
    }

    #[test]
    fn set_focus_reranks_and_cancels_blur_work() {
        let s = sched([1, 2, 1, 4, 1, 8]);
        let log = Arc::new(Mutex::new(vec![]));
        let rel = Arc::new(AtomicBool::new(false));
        let blocker = s.submit(TaskSpec::new("t", "x", Resources::one(Res::Cpu)), gate_task("x", log.clone(), rel.clone()));
        wait_until(|| log.lock().unwrap().len() == 1);
        let order = Arc::new(Mutex::new(vec![]));
        let mk = |name: &'static str, oid: &str| {
            let order = order.clone();
            s.submit(
                TaskSpec::new("assets", name, Resources::one(Res::Cpu)).original(oid).priority(Priority::Background).auto_priority(),
                move |_| {
                    order.lock().unwrap().push(name);
                    Ok(Value::Null)
                },
            )
        };
        let a = mk("a", "oa");
        let b = mk("b", "ob");
        let c = mk("c", "oc");
        let est = s.submit(TaskSpec::new("estimate", "est", Resources::one(Res::Cpu)).original("oa").priority(Priority::Selected).cancel_on_blur(), |_| Ok(Value::Null));
        s.set_focus(Some("oc"), &["ob".to_string()]);
        assert_eq!(s.job(&c.id).unwrap().priority, Priority::Selected);
        assert!(est.wait().unwrap_err().code_is("cancelled"));
        rel.store(true, Ordering::SeqCst);
        for h in [blocker, a, b, c] {
            h.wait().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec!["c", "b", "a"]);
    }

    #[test]
    fn events_for_visible_jobs_and_activity() {
        struct Rec(Mutex<Vec<(String, JobState)>>, Mutex<Vec<bool>>);
        impl JobSink for Rec {
            fn job(&self, j: &Job) {
                self.0.lock().unwrap().push((j.label.clone(), j.state));
            }
            fn device_busy(&self, b: bool) {
                self.1.lock().unwrap().push(b);
            }
        }
        let rec = Arc::new(Rec(Mutex::new(vec![]), Mutex::new(vec![])));
        let s = Scheduler::new(Limits([1, 1, 1, 1, 1, 1]), Policy::default(), rec.clone());
        let h = s.submit(TaskSpec::new("send", "vis", Resources::one(Res::Device)).visible(), |ctx| {
            for i in 0..5 {
                ctx.progress_bytes(i as f64 / 5.0, "Uploading", i, 5);
            }
            Ok(json!({"name": "x", "secret": 1}))
        });
        s.submit(TaskSpec::new("probe", "hidden", Resources::one(Res::Io)), |_| Ok(Value::Null)).wait().unwrap();
        h.wait().unwrap();
        let ev = rec.0.lock().unwrap().clone();
        assert!(ev.iter().all(|(l, _)| l == "vis"));
        assert_eq!(ev.first().unwrap().1, JobState::Queued);
        assert_eq!(ev.last().unwrap().1, JobState::Done);
        let act = s.activity(120_000);
        assert_eq!(act.len(), 1);
        assert_eq!(act[0].result.as_ref().unwrap()["name"], json!("x"));
        assert!(act[0].result.as_ref().unwrap().get("secret").is_none());
        wait_until(|| rec.1.lock().unwrap().last() == Some(&false));
        assert_eq!(rec.1.lock().unwrap().first(), Some(&true));
    }

    #[test]
    fn panicking_task_fails_cleanly() {
        let s = sched([1, 1, 1, 1, 1, 1]);
        let h = s.submit(TaskSpec::new("t", "p", Resources::one(Res::Cpu)), |_| panic!("boom"));
        assert!(h.wait().is_err());
        let h2 = s.submit(TaskSpec::new("t", "ok", Resources::one(Res::Cpu)), |_| Ok(json!(1)));
        assert_eq!(h2.wait().unwrap(), json!(1));
    }
}
