//! Supervised dock process. Anonymous inherited pipes carry bounded typed JSON.
//! The host owns durable profiles; this process owns transport, renders and actions.
use super::{
    dock_service::{self, DockHooks, DockStatus, DockTimings},
    profiles::{Face, ProfileState, ProfileStore},
    runtime_resources::RuntimeResources,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

const VERSION: u32 = 1;
const MAX_FRAME: usize = 64 * 1024 * 1024;
type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>>;
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Op {
    Init {
        state: ProfileState,
        home: PathBuf,
        resources: PathBuf,
        fake: bool,
        #[serde(default)]
        shared_hardware: bool,
        #[serde(default)]
        shared_usage: bool,
    },
    Profiles {
        state: ProfileState,
    },
    Enabled {
        enabled: bool,
    },
    Brightness {
        value: u8,
    },
    Configure {
        enabled: bool,
        brightness: u8,
    },
    Previews {
        faces: Vec<Face>,
        clock: String,
    },
    Button {
        index: usize,
        pressed: bool,
    },
    Studio {
        running: bool,
    },
    Hardware {
        snapshot: Option<super::hardware::Snapshot>,
    },
    Usage {
        batch: Option<super::shared_usage::Batch>,
    },
    ActionResult {
        action_id: u64,
        result: Result<(), String>,
    },
    Shutdown {},
    Quiesce {},
    Resume {},
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    id: u64,
    request: Op,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Message {
    CoolerAction {
        version: u32,
        action_id: u64,
        action: super::actions::CoolerAction,
    },
    Reply {
        version: u32,
        id: u64,
        result: Result<Value, String>,
    },
    Status {
        version: u32,
        status: DockStatus,
        busy: bool,
        usage: Value,
        #[serde(default)]
        hardware: Value,
    },
}
fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut bytes = Vec::new();
    let n = reader
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "Dock worker pipe read failed")?;
    if n == 0 {
        return Ok(None);
    }
    if bytes.len() > MAX_FRAME || bytes.last() != Some(&b'\n') {
        return Err("Dock worker frame exceeds limit or is incomplete".into());
    }
    Ok(Some(bytes))
}
fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "Dock worker frame encoding failed")?;
    if bytes.len() >= MAX_FRAME {
        return Err("Dock worker frame exceeds limit".into());
    }
    writer
        .write_all(&bytes)
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|_| "Dock worker pipe write failed".into())
}
fn unavailable(enabled: bool, fake: bool) -> DockStatus {
    DockStatus {
        state: if enabled { "restarting" } else { "disabled" }.into(),
        message: "Dock worker is unavailable; current readings are invalid".into(),
        enabled,
        fake,
        current_profile: None,
        current_page: None,
        pages_sent: 0,
        frames_sent: 0,
        keep_alives_sent: 0,
        probe_writes: 0,
        actions: 0,
        errors: 0,
        timing: DockTimings::default(),
    }
}
fn inactive_usage() -> Value {
    json!({"reader":{"code":"inactive","message":"Dock worker is unavailable; AI usage is inactive."},"providers":[{"provider":"claude","selected":false,"status":"inactive","code":"inactive","message":"AI usage is inactive."},{"provider":"codex","selected":false,"status":"inactive","code":"inactive","message":"AI usage is inactive."}]})
}
struct Cache {
    status: DockStatus,
    usage: Value,
    busy: bool,
    alive: bool,
    uncertain: bool,
    retiring: bool,
    generation: u64,
    restarts: u64,
    pid: Option<u32>,
    next_start: Instant,
    last_status: Instant,
    profile_revision: Option<String>,
    hardware: Value,
    hardware_pending: Option<u64>,
}
struct Session {
    child: Child,
    input: Option<Arc<mpsc::SyncSender<Request>>>,
    pending: Pending,
    alive: Arc<AtomicBool>,
}
fn claim_action_id(highest: &mut u64, id: u64) -> bool {
    if id == 0 || id > 9_007_199_254_740_991 || id <= *highest {
        return false;
    }
    *highest = id;
    true
}
type CoolerHandler = Arc<dyn Fn(super::actions::CoolerAction) -> Result<(), String> + Send + Sync>;
#[derive(Default)]
struct CrossState {
    handler: Option<CoolerHandler>,
    pending: usize,
    closed: bool,
}
struct CrossJob {
    action: super::actions::CoolerAction,
    action_id: u64,
    input: Arc<mpsc::SyncSender<Request>>,
}
// Only the result is retried while the original session channel has room;
// the already-executed target effect is never submitted again.
fn enqueue_completion(input: &mpsc::SyncSender<Request>, mut result: Request) -> bool {
    let until = Instant::now() + Duration::from_millis(500);
    loop {
        match input.try_send(result) {
            Ok(()) => return true,
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
            Err(mpsc::TrySendError::Full(unsent)) => result = unsent,
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
struct CrossActions {
    state: Arc<Mutex<CrossState>>,
    queue: mpsc::SyncSender<CrossJob>,
}
impl CrossActions {
    fn new() -> Self {
        let state = Arc::new(Mutex::new(CrossState::default()));
        let shared = state.clone();
        let (queue, jobs) = mpsc::sync_channel::<CrossJob>(4);
        std::thread::spawn(move || {
            for job in jobs {
                let handler = shared.lock().unwrap().handler.clone();
                let result = handler
                    .ok_or_else(|| "Cooler host routing is unavailable".to_string())
                    .and_then(|handler| {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            handler(job.action)
                        }))
                        .unwrap_or_else(|_| {
                            Err("Cooler host routing failed; outcome is uncertain".into())
                        })
                    });
                // Delivery belongs to the original session only. Never acquire the
                // host operation lock here: Shutdown may be draining the executor.
                enqueue_completion(
                    &job.input,
                    Request {
                        version: VERSION,
                        id: 0,
                        request: Op::ActionResult {
                            action_id: job.action_id,
                            result,
                        },
                    },
                );
                shared.lock().unwrap().pending -= 1;
            }
        });
        Self { state, queue }
    }
    fn admit(
        &self,
        action: super::actions::CoolerAction,
        action_id: u64,
        input: Arc<mpsc::SyncSender<Request>>,
    ) -> Result<(), String> {
        action.validate()?;
        let mut state = self.state.lock().unwrap();
        if state.closed || state.handler.is_none() {
            return Err("Cooler action admission is closed".into());
        }
        state.pending += 1;
        if self
            .queue
            .try_send(CrossJob {
                action,
                action_id,
                input,
            })
            .is_err()
        {
            state.pending -= 1;
            return Err("Cooler action queue is full".into());
        }
        Ok(())
    }
}
struct CrossBarrier {
    state: Arc<Mutex<CrossState>>,
    keep: bool,
}
impl CrossBarrier {
    fn close(state: Arc<Mutex<CrossState>>) -> Result<Self, String> {
        {
            let mut s = state.lock().unwrap();
            if s.pending > 0 {
                return Err("A cross-device action is still running".into());
            }
            s.closed = true;
        }
        Ok(Self { state, keep: false })
    }
}
impl Drop for CrossBarrier {
    fn drop(&mut self) {
        if !self.keep {
            self.state.lock().unwrap().closed = false;
        }
    }
}
pub struct Controller {
    profiles: Arc<ProfileStore>,
    home: PathBuf,
    resources: PathBuf,
    fake: bool,
    session: Mutex<Option<Session>>,
    cache: Arc<Mutex<Cache>>,
    operation: Mutex<()>,
    enabled: AtomicBool,
    brightness: AtomicUsize,
    stopped: AtomicBool,
    inflight: AtomicUsize,
    ids: AtomicU64,
    retry: Mutex<(Instant, Duration)>,
    quiesced: AtomicBool,
    profiles_dirty: AtomicBool,
    profiles_sync: Mutex<()>,
    cross: Arc<CrossActions>,
}
impl Controller {
    pub fn new(
        profiles: Arc<ProfileStore>,
        home: PathBuf,
        resources: PathBuf,
        fake: bool,
    ) -> Arc<Self> {
        let fake = cfg!(debug_assertions) && fake;
        let this = Arc::new(Self {
            profiles,
            home,
            resources,
            fake,
            session: Mutex::new(None),
            cache: Arc::new(Mutex::new(Cache {
                status: unavailable(false, fake),
                usage: inactive_usage(),
                busy: false,
                alive: false,
                uncertain: false,
                retiring: false,
                generation: 0,
                restarts: 0,
                pid: None,
                next_start: Instant::now(),
                last_status: Instant::now(),
                profile_revision: None,
                hardware: Value::Null,
                hardware_pending: None,
            })),
            operation: Mutex::new(()),
            enabled: AtomicBool::new(false),
            brightness: AtomicUsize::new(50),
            stopped: AtomicBool::new(false),
            inflight: AtomicUsize::new(0),
            ids: AtomicU64::new(1),
            retry: Mutex::new((Instant::now(), Duration::from_millis(250))),
            quiesced: AtomicBool::new(false),
            profiles_dirty: AtomicBool::new(false),
            profiles_sync: Mutex::new(()),
            cross: Arc::new(CrossActions::new()),
        });
        let weak = Arc::downgrade(&this);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            let Some(this) = weak.upgrade() else { break };
            if this.stopped.load(Ordering::SeqCst) {
                break;
            }
            if this.enabled.load(Ordering::SeqCst) && !this.cache.lock().unwrap().alive {
                let _ = this.ensure_started();
            }
            if this.profiles_dirty.load(Ordering::SeqCst)
                && this.cache.lock().unwrap().alive
                && !this.quiesced.load(Ordering::SeqCst)
            {
                let _ = this.sync_profiles();
            }
        });
        this
    }
    pub fn set_cooler_handler(&self, handler: CoolerHandler) {
        self.cross.state.lock().unwrap().handler = Some(handler);
    }
    pub fn cross_action_busy(&self) -> bool {
        self.cross.state.lock().unwrap().pending > 0
    }
    fn ensure_started(&self) -> Result<(), String> {
        let _operation = self.operation.lock().unwrap();
        if self.stopped.load(Ordering::SeqCst) || self.quiesced.load(Ordering::SeqCst) {
            return Err("Dock worker admission is closed".into());
        }
        {
            let c = self.cache.lock().unwrap();
            if c.alive {
                if c.retiring || c.uncertain || c.last_status.elapsed() > Duration::from_secs(3) {
                    return Err("Dock worker is unresponsive; outcome is uncertain".into());
                }
                return Ok(());
            }
            if Instant::now() < c.next_start {
                return Err("Dock worker is waiting to restart".into());
            }
        }
        let mut retry = self.retry.lock().unwrap();
        if Instant::now() < retry.0 {
            return Err("Dock worker is waiting to restart".into());
        }
        retry.0 = Instant::now() + retry.1;
        retry.1 = (retry.1 * 2).min(Duration::from_secs(30));
        let mut slot = self.session.lock().unwrap();
        if let Some(mut old) = slot.take() {
            drop(old.input.take());
            if old
                .child
                .try_wait()
                .map_err(|_| "Dock worker wait failed")?
                .is_none()
            {
                *slot = Some(old);
                return Err("Dock worker is still stopping".into());
            }
        }
        let (snapshot, snapshot_revision) = self
            .profiles
            .state_with_revision()
            .map_err(|e| e.to_string())?;
        let mut command = Command::new(
            std::env::current_exe().map_err(|_| "Dock worker executable unavailable")?,
        );
        command
            .arg("--dock-worker")
            .env("UC_DOCK_WORKER", "1")
            .env("UC_HOME", self.home.join("devices/dock"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(crate::core::process::CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|_| "Dock worker could not start")?;
        let mut input = child.stdin.take().ok_or("Dock worker input unavailable")?;
        let output = child
            .stdout
            .take()
            .ok_or("Dock worker output unavailable")?;
        let alive = Arc::new(AtomicBool::new(true));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (cache, live, waiting) = (self.cache.clone(), alive.clone(), pending.clone());
        let generation = {
            let mut c = cache.lock().unwrap();
            c.generation += 1;
            c.restarts = c.generation.saturating_sub(1);
            c.pid = Some(child.id());
            c.alive = true;
            c.busy = true;
            c.uncertain = false;
            c.retiring = false;
            c.hardware_pending = None;
            c.status = unavailable(self.enabled.load(Ordering::SeqCst), self.fake);
            c.usage = inactive_usage();
            c.hardware = Value::Null;
            c.last_status = Instant::now();
            c.profile_revision = None;
            c.generation
        };
        let (input_tx, input_rx) = mpsc::sync_channel::<Request>(4);
        let input_tx = Arc::new(input_tx);
        let callback_input = Arc::downgrade(&input_tx);
        let cross = self.cross.clone();
        std::thread::spawn(move || {
            let mut highest_action = 0;
            let mut reader = BufReader::new(output);
            loop {
                let Ok(Some(bytes)) = read_frame(&mut reader) else {
                    break;
                };
                let Ok(message) = serde_json::from_slice::<Message>(&bytes) else {
                    break;
                };
                match message {
                    Message::CoolerAction {
                        version,
                        action_id,
                        action,
                    } if version == VERSION => {
                        let Some(callback_input) = callback_input.upgrade() else {
                            break;
                        };
                        // A duplicate must not complete the original wait with a
                        // spurious failure while its operation is still running.
                        let admitting = {
                            let c = cache.lock().unwrap();
                            c.generation == generation && c.alive && !c.uncertain && !c.retiring
                        };
                        if !admitting || !claim_action_id(&mut highest_action, action_id) {
                            continue;
                        }
                        let result = cross.admit(action, action_id, callback_input.clone());
                        if let Err(error) = result {
                            let _ = callback_input.try_send(Request {
                                version: VERSION,
                                id: 0,
                                request: Op::ActionResult {
                                    action_id,
                                    result: Err(error),
                                },
                            });
                        }
                    }
                    Message::Reply {
                        version,
                        id,
                        result,
                    } if version == VERSION => {
                        let mut pending = waiting.lock().unwrap();
                        if let Some(tx) = pending.remove(&id) {
                            let _ = tx.send(result);
                        }
                        let mut c = cache.lock().unwrap();
                        if c.generation == generation && c.hardware_pending == Some(id) {
                            c.hardware_pending = None;
                        }
                        if c.generation == generation && pending.is_empty() && !c.retiring {
                            c.uncertain = false;
                        }
                    }
                    Message::Status {
                        version,
                        status,
                        busy,
                        usage,
                        hardware,
                    } if version == VERSION => {
                        let mut c = cache.lock().unwrap();
                        if c.generation == generation {
                            c.status = status;
                            c.busy = busy;
                            c.usage = usage;
                            c.hardware = hardware;
                            c.last_status = Instant::now();
                        }
                    }
                    _ => break,
                }
            }
            live.store(false, Ordering::SeqCst);
            {
                let mut c = cache.lock().unwrap();
                if c.generation == generation {
                    c.alive = false;
                    c.busy = false;
                    c.status = unavailable(c.status.enabled, c.status.fake);
                    c.usage = inactive_usage();
                    c.hardware = Value::Null;
                    c.hardware_pending = None;
                    c.pid = None;
                    c.next_start = Instant::now() + Duration::from_millis(500);
                }
            }
            for (_, tx) in waiting.lock().unwrap().drain() {
                let _ = tx.send(Err(
                    "Dock worker disconnected; request was not replayed".into()
                ));
            }
        });
        let (cache, live, waiting) = (self.cache.clone(), alive.clone(), pending.clone());
        std::thread::spawn(move || {
            for request in input_rx {
                if write_frame(&mut input, &request).is_err() {
                    live.store(false, Ordering::SeqCst);
                    {
                        let mut c = cache.lock().unwrap();
                        if c.generation == generation {
                            c.alive = false;
                            c.uncertain = true;
                            c.usage = inactive_usage();
                            c.next_start = Instant::now() + Duration::from_millis(500);
                        }
                    }
                    for (_, tx) in waiting.lock().unwrap().drain() {
                        let _ = tx.send(Err(
                            "Dock worker pipe write failed; outcome is uncertain".into()
                        ));
                    }
                    break;
                }
            }
            // Dropping the sole writer delivers EOF, closing action admission.
        });
        *slot = Some(Session {
            child,
            input: Some(input_tx),
            pending,
            alive,
        });
        drop(slot);
        drop(retry);
        let initialized = (|| {
            self.rpc_inner(Op::Init {
                state: snapshot,
                home: self.home.clone(),
                resources: self.resources.clone(),
                fake: self.fake,
                shared_hardware: true,
                shared_usage: true,
            })?;
            self.cache.lock().unwrap().profile_revision = Some(snapshot_revision);
            self.rpc_inner(Op::Configure {
                enabled: self.enabled.load(Ordering::SeqCst),
                brightness: self.brightness.load(Ordering::SeqCst) as u8,
            })?;
            Ok(())
        })();
        if initialized.is_err() {
            if let Some(s) = self.session.lock().unwrap().as_mut() {
                drop(s.input.take());
            }
        }
        initialized
    }
    fn rpc_inner(&self, request: Op) -> Result<Value, String> {
        let ephemeral = matches!(request, Op::Hardware { .. } | Op::Usage { .. });
        if !matches!(request, Op::Shutdown {}) {
            let c = self.cache.lock().unwrap();
            if c.uncertain || c.retiring || c.last_status.elapsed() > Duration::from_secs(3) {
                return Err("Dock worker is unresponsive; outcome is uncertain".into());
            }
        }
        if !ephemeral {
            self.inflight.fetch_add(1, Ordering::SeqCst);
        }
        struct Guard<'a>(&'a AtomicUsize, bool);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                if self.1 {
                    self.0.fetch_sub(1, Ordering::SeqCst);
                }
            }
        }
        let _guard = Guard(&self.inflight, !ephemeral);
        let id = self.ids.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        {
            let mut slot = self.session.lock().unwrap();
            let s = slot.as_mut().ok_or("Dock worker is unavailable")?;
            if !s.alive.load(Ordering::SeqCst) {
                return Err("Dock worker is disconnected".into());
            }
            let sender = s.input.as_ref().ok_or("Dock worker pipe is closed")?;
            s.pending.lock().unwrap().insert(id, tx);
            if ephemeral {
                self.cache.lock().unwrap().hardware_pending = Some(id);
            }
            if sender
                .try_send(Request {
                    version: VERSION,
                    id,
                    request,
                })
                .is_err()
            {
                s.pending.lock().unwrap().remove(&id);
                if ephemeral {
                    self.cache.lock().unwrap().hardware_pending = None;
                } else {
                    drop(s.input.take());
                }
                return Err("Dock worker request queue is full or closed".into());
            }
        }
        // A slow active action is never killed. Keep its admission pending until
        // an acknowledged result or pipe loss; callers run off the UI thread.
        match rx.recv_timeout(if ephemeral {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(15)
        }) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if ephemeral {
                    if let Some(session) = self.session.lock().unwrap().as_ref() {
                        session.pending.lock().unwrap().remove(&id);
                    }
                    // Keep the single probe gate until the actual late reply.
                    // Ephemeral readings cannot create action uncertainty.
                    return Err("Hardware exchange is deferred".into());
                }
                {
                    let mut c = self.cache.lock().unwrap();
                    c.uncertain = true;
                    c.retiring = true;
                }
                if let Some(s) = self.session.lock().unwrap().as_mut() {
                    drop(s.input.take());
                }
                Err(
                    "Dock worker request timed out; outcome is uncertain and will not be replayed"
                        .into(),
                )
            }
            Err(_) => Err("Dock worker reply channel closed".into()),
        }
    }
    fn rpc(&self, request: Op) -> Result<Value, String> {
        self.ensure_started()?;
        let _operation = self.operation.lock().unwrap();
        if self.stopped.load(Ordering::SeqCst) || self.quiesced.load(Ordering::SeqCst) {
            return Err("Dock worker is stopping".into());
        }
        self.rpc_inner(request)
    }
    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.configure(enabled, self.brightness.load(Ordering::SeqCst) as u8)
    }
    pub fn configure(&self, enabled: bool, brightness: u8) -> Result<(), String> {
        if brightness > 100 {
            return Err("Brightness must be between 0 and 100".into());
        }
        if self.stopped.load(Ordering::SeqCst) {
            return Err("Dock worker is stopping".into());
        }
        if !enabled && self.cross_action_busy() {
            return Err("A cross-device action is still running".into());
        }
        if !enabled && self.session.lock().unwrap().is_none() {
            self.enabled.store(false, Ordering::SeqCst);
            self.brightness.store(brightness as usize, Ordering::SeqCst);
            return Ok(());
        }
        self.rpc(Op::Configure {
            enabled,
            brightness,
        })?;
        self.enabled.store(enabled, Ordering::SeqCst);
        self.brightness.store(brightness as usize, Ordering::SeqCst);
        Ok(())
    }
    pub fn start_configured(&self, enabled: bool, brightness: u8) {
        self.brightness
            .store(brightness.min(100) as usize, Ordering::SeqCst);
        self.enabled.store(enabled, Ordering::SeqCst);
    }
    pub fn set_brightness(&self, value: u8) -> Result<(), String> {
        self.configure(self.enabled.load(Ordering::SeqCst), value)
    }
    pub fn notify_profiles_changed(&self) {
        let _sync = self.profiles_sync.lock().unwrap();
        self.profiles_dirty.store(true, Ordering::SeqCst);
        let _ = self.sync_profiles_locked();
    }
    fn sync_profiles(&self) -> Result<(), String> {
        let _sync = self.profiles_sync.lock().unwrap();
        self.sync_profiles_locked()
    }
    fn sync_profiles_locked(&self) -> Result<(), String> {
        if !self.profiles_dirty.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.ensure_started()?;
        let _operation = self.operation.lock().unwrap();
        if self.quiesced.load(Ordering::SeqCst) || self.stopped.load(Ordering::SeqCst) {
            return Err("Dock worker admission is closed".into());
        }
        let (state, revision) = self
            .profiles
            .state_with_revision()
            .map_err(|e| e.to_string())?;
        if self.cache.lock().unwrap().profile_revision.as_ref() == Some(&revision) {
            self.profiles_dirty.store(false, Ordering::SeqCst);
            return Ok(());
        }
        self.rpc_inner(Op::Profiles { state })?;
        self.cache.lock().unwrap().profile_revision = Some(revision.clone());
        if self
            .profiles
            .state_with_revision()
            .map_err(|e| e.to_string())?
            .1
            == revision
        {
            self.profiles_dirty.store(false, Ordering::SeqCst);
        }
        Ok(())
    }
    pub fn status(&self) -> DockStatus {
        let c = self.cache.lock().unwrap();
        if !c.alive || c.uncertain || c.last_status.elapsed() > Duration::from_secs(3) {
            unavailable(self.enabled.load(Ordering::SeqCst), self.fake)
        } else {
            c.status.clone()
        }
    }
    pub fn usage_status(&self) -> Value {
        let c = self.cache.lock().unwrap();
        let mut usage = c.usage.clone();
        if !c.alive
            || c.uncertain
            || c.last_status.elapsed() > Duration::from_secs(3)
            || usage["shared"] == true
                && usage["expires_at_ms"]
                    .as_u64()
                    .is_none_or(|at| at <= super::scheduler::now_ms())
        {
            if !usage.is_object() {
                return inactive_usage();
            }
            usage["reader"] = inactive_usage()["reader"].clone();
            usage["fresh"] = json!(false);
            if let Some(providers) = usage["providers"].as_array_mut() {
                for provider in providers {
                    if provider["selected"] == true {
                        provider["status"] = json!("unavailable");
                        provider["code"] = json!("unverified-or-unavailable");
                        provider["message"] =
                            json!(super::usage::UsageStatus::Unavailable.message());
                    }
                }
            }
        }
        usage
    }
    pub fn usage_state(&self) -> Value {
        self.usage_status()
    }
    pub fn worker_status(&self) -> Value {
        let c = self.cache.lock().unwrap();
        json!({"pid":c.pid,"generation":c.generation,"restarts":c.restarts,"busy":c.busy,"crossActionBusy":self.cross_action_busy(),"state":if c.alive{"running"}else{"unavailable"}})
    }
    pub fn hardware_state(&self) -> Value {
        let cache = self.cache.lock().unwrap();
        let mut state = cache.hardware.clone();
        if !state.is_object() {
            state = json!({"shared":true,"fresh":false,"local_polls":null});
        }
        if !cache.alive
            || cache.uncertain
            || cache.retiring
            || cache.last_status.elapsed() > Duration::from_secs(3)
            || state["expires_at_ms"]
                .as_u64()
                .is_none_or(|at| at <= super::scheduler::now_ms())
        {
            state["fresh"] = json!(false);
            state["cpu_present"] = json!(false);
            state["ram_present"] = json!(false);
            state["gpu_fields"] = json!(0);
            state["demand"] = json!(super::hardware::Demand::default());
        }
        state
    }
    /// Readings never start a disabled/missing worker, and do not queue behind
    /// editor work. The bridge coalesces to the latest snapshot on its next turn.
    pub fn hardware_exchange(
        &self,
        snapshot: Option<super::hardware::Snapshot>,
    ) -> Result<Value, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "Dock worker is busy")?;
        if self.stopped.load(Ordering::SeqCst) || self.quiesced.load(Ordering::SeqCst) {
            return Err("Dock worker admission is closed".into());
        }
        if self.cache.lock().unwrap().hardware_pending.is_some() {
            return Err("Hardware exchange is waiting for a reply".into());
        }
        self.rpc_inner(Op::Hardware { snapshot })
    }
    pub fn usage_exchange(
        &self,
        batch: Option<super::shared_usage::Batch>,
    ) -> Result<Value, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "Dock worker is busy")?;
        if self.stopped.load(Ordering::SeqCst) || self.quiesced.load(Ordering::SeqCst) {
            return Err("Dock worker admission is closed".into());
        }
        if self.cache.lock().unwrap().hardware_pending.is_some() {
            return Err("Shared reading exchange is waiting for a reply".into());
        }
        self.rpc_inner(Op::Usage { batch })
    }
    pub fn busy(&self) -> bool {
        if self.cross_action_busy() {
            return true;
        }
        let cache_busy = {
            let c = self.cache.lock().unwrap();
            c.busy
                || c.alive
                    && (c.retiring
                        || c.uncertain
                        || c.last_status.elapsed() > Duration::from_secs(3))
        };
        if self.inflight.load(Ordering::SeqCst) > 0 || cache_busy {
            return true;
        }
        let mut slot = self.session.lock().unwrap();
        slot.as_mut().is_some_and(|s| {
            !s.pending.lock().unwrap().is_empty()
                || !s.alive.load(Ordering::SeqCst) && !matches!(s.child.try_wait(), Ok(Some(_)))
        })
    }
    pub fn previews(&self, faces: Vec<Face>, clock: String) -> Result<Value, String> {
        self.ensure_started()?;
        let _operation = self.operation.lock().unwrap();
        if self.stopped.load(Ordering::SeqCst) || self.quiesced.load(Ordering::SeqCst) {
            return Err("Dock worker admission is closed".into());
        }
        let generation = self.cache.lock().unwrap().generation;
        let frames = self.rpc_inner(Op::Previews { faces, clock })?;
        {
            let c = self.cache.lock().unwrap();
            if !c.alive || c.uncertain || c.generation != generation {
                return Err("Dock preview belongs to a disconnected worker".into());
            }
        }
        let frames: Vec<PreviewFrame> =
            serde_json::from_value(frames).map_err(|_| "Invalid dock worker preview")?;
        preview_output(frames, super::scheduler::now_ms())
    }
    pub fn simulate_button(&self, index: usize, pressed: bool) -> Result<(), String> {
        self.rpc(Op::Button { index, pressed }).map(|_| ())
    }
    pub fn set_fake_studio(&self, running: bool) -> Result<(), String> {
        self.rpc(Op::Studio { running }).map(|_| ())
    }
    pub fn prepare_exit(&self) -> Result<(), String> {
        let mut barrier = CrossBarrier::close(self.cross.state.clone())?;
        let _operation = self.operation.lock().unwrap();
        // A cached busy flag can describe a render which has already finished.
        // The child owns admission; only uncertain transport state is a host
        // refusal before asking it to establish the authoritative barrier.
        {
            let c = self.cache.lock().unwrap();
            if c.uncertain
                || c.retiring
                || c.alive && c.last_status.elapsed() > Duration::from_secs(3)
            {
                return Err("Dock worker state is uncertain; try again when idle".into());
            }
        }
        {
            let mut session = self.session.lock().unwrap();
            if session.as_mut().is_some_and(|s| {
                !s.pending.lock().unwrap().is_empty()
                    || !s.alive.load(Ordering::SeqCst) && !matches!(s.child.try_wait(), Ok(Some(_)))
            }) {
                return Err("Dock worker is recovering; try again when idle".into());
            }
        }
        if self.cache.lock().unwrap().alive {
            self.rpc_inner(Op::Quiesce {})?;
        } else if self.enabled.load(Ordering::SeqCst) {
            return Err("Dock worker is recovering; try again when idle".into());
        }
        self.quiesced.store(true, Ordering::SeqCst);
        barrier.keep = true;
        Ok(())
    }
    pub fn cancel_exit(&self) {
        self.cross.state.lock().unwrap().closed = false;
        let _operation = self.operation.lock().unwrap();
        if self.cache.lock().unwrap().alive {
            let _ = self.rpc_inner(Op::Resume {});
        }
        self.quiesced.store(false, Ordering::SeqCst);
    }
    pub fn shutdown(&self) {
        self.cross.state.lock().unwrap().closed = true;
        let _operation = self.operation.lock().unwrap();
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        self.enabled.store(false, Ordering::SeqCst);
        if self.cache.lock().unwrap().alive {
            let _ = self.rpc_inner(Op::Shutdown {});
        }
        if let Some(mut s) = self.session.lock().unwrap().take() {
            drop(s.input);
            let _ = s.child.wait();
        }
        let mut c = self.cache.lock().unwrap();
        c.alive = false;
        c.busy = false;
        c.pid = None;
        c.status = unavailable(false, self.fake);
        c.usage = inactive_usage();
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Called before any GUI, single-instance, tray or device initialization.
pub fn worker_main() -> Result<(), String> {
    if std::env::var_os("UC_DOCK_WORKER").as_deref() != Some(std::ffi::OsStr::new("1")) {
        return Err("Dock worker requires inherited-pipe launch".into());
    }
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let mut reader = BufReader::new(std::io::stdin());
    let bytes = read_frame(&mut reader)?.ok_or("Dock worker init missing")?;
    let first: Request = serde_json::from_slice(&bytes).map_err(|_| "Invalid dock worker init")?;
    if first.version != VERSION {
        return Err("Unsupported dock worker protocol".into());
    }
    let Op::Init {
        state,
        home,
        resources,
        fake,
        shared_hardware,
        shared_usage,
    } = first.request
    else {
        return Err("Dock worker init must be first".into());
    };
    if fake && !cfg!(debug_assertions) {
        return Err("Fake dock requires a debug build".into());
    }
    let profiles = Arc::new(ProfileStore::replica(state).map_err(|e| e.to_string())?);
    let resources = if shared_hardware {
        RuntimeResources::new_shared_services(&home, resources, fake, shared_usage)
    } else {
        RuntimeResources::new(&home, resources, fake)
    };
    let action_waiters: Arc<Mutex<HashMap<u64, mpsc::Sender<Result<(), String>>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let (waiters, callback_output) = (action_waiters.clone(), output.clone());
    let callback_live = Arc::new(AtomicBool::new(true));
    let callback_alive = callback_live.clone();
    let action_ids = AtomicU64::new(1);
    resources.set_cooler_handler(Arc::new(move |action| {
        let action_id = action_ids.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        {
            let mut pending = waiters.lock().unwrap();
            if !callback_alive.load(Ordering::SeqCst) {
                return Err("Cooler host disconnected".into());
            }
            pending.insert(action_id, tx);
        }
        if let Err(error) = write_frame(
            &mut *callback_output.lock().unwrap(),
            &Message::CoolerAction {
                version: VERSION,
                action_id,
                action,
            },
        ) {
            waiters.lock().unwrap().remove(&action_id);
            return Err(error);
        }
        let result = rx
            .recv_timeout(Duration::from_secs(125))
            .unwrap_or_else(|_| {
                Err("Cooler action outcome is uncertain; it was not replayed".into())
            });
        waiters.lock().unwrap().remove(&action_id);
        result
    }));
    let controller = Arc::new(dock_service::Controller::new(
        profiles.clone(),
        resources.clone(),
        fake,
    ));
    write_frame(
        &mut *output.lock().unwrap(),
        &Message::Reply {
            version: VERSION,
            id: first.id,
            result: Ok(Value::Null),
        },
    )?;
    let stop = Arc::new(AtomicBool::new(false));
    let (c, r, o, s) = (
        controller.clone(),
        resources.clone(),
        output.clone(),
        stop.clone(),
    );
    let reporter = std::thread::spawn(move || {
        while !s.load(Ordering::SeqCst) {
            if write_frame(
                &mut *o.lock().unwrap(),
                &Message::Status {
                    version: VERSION,
                    status: c.status(),
                    busy: c.busy(),
                    usage: r.usage_status(),
                    hardware: r.hardware_state(),
                },
            )
            .is_err()
            {
                c.shutdown();
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    let (tx, rx) = mpsc::sync_channel(32);
    let c = controller.clone();
    let pipe_alive = Arc::new(AtomicBool::new(true));
    let live = pipe_alive.clone();
    let input = std::thread::spawn(move || {
        loop {
            let Ok(Some(bytes)) = read_frame(&mut reader) else {
                break;
            };
            let Ok(request) = serde_json::from_slice::<Request>(&bytes) else {
                break;
            };
            if request.version != VERSION {
                break;
            }
            if let Op::ActionResult { action_id, result } = request.request {
                if let Some(waiter) = action_waiters.lock().unwrap().remove(&action_id) {
                    let _ = waiter.send(result);
                }
                continue;
            }
            if tx.try_send(request).is_err() {
                break;
            }
        }
        live.store(false, Ordering::SeqCst);
        callback_live.store(false, Ordering::SeqCst);
        for (_, waiter) in action_waiters.lock().unwrap().drain() {
            let _ = waiter.send(Err(
                "Cooler host disconnected; action was not replayed".into()
            ));
        }
        c.shutdown();
    });
    for request in rx {
        if !pipe_alive.load(Ordering::SeqCst) {
            break;
        }
        let shutting = matches!(request.request, Op::Shutdown {});
        let result = dispatch(request.request, &controller, &profiles, &resources);
        if write_frame(
            &mut *output.lock().unwrap(),
            &Message::Reply {
                version: VERSION,
                id: request.id,
                result,
            },
        )
        .is_err()
            || shutting
        {
            break;
        }
    }
    controller.shutdown();
    resources.shutdown();
    stop.store(true, Ordering::SeqCst);
    let _ = reporter.join();
    // The host closes its input after Shutdown acknowledgement. Do not wait on
    // that reader here: process exit owns stdin and the detached reader.
    drop(input);
    Ok(())
}
fn dispatch(
    op: Op,
    controller: &dock_service::Controller,
    profiles: &ProfileStore,
    resources: &RuntimeResources,
) -> Result<Value, String> {
    match op {
        Op::ActionResult { .. } => return Err("Unexpected cooler action completion".into()),
        Op::Init { .. } => return Err("Dock worker is already initialized".into()),
        Op::Profiles { state } => {
            profiles.replace_replica(state).map_err(|e| e.to_string())?;
            controller.sync_profiles_changed()?;
        }
        Op::Enabled { enabled } => {
            if !enabled {
                controller.prepare_exit()?;
            }
            controller.set_enabled(enabled)?;
            controller.cancel_exit();
        }
        Op::Configure {
            enabled,
            brightness,
        } => {
            if brightness > 100 {
                return Err("Brightness must be between 0 and 100".into());
            }
            if !enabled {
                controller.prepare_exit()?;
            }
            controller.set_brightness(brightness)?;
            controller.set_enabled(enabled)?;
            controller.cancel_exit();
        }
        Op::Quiesce {} => controller.prepare_exit()?,
        Op::Resume {} => controller.cancel_exit(),
        Op::Brightness { value } => controller.set_brightness(value)?,
        Op::Button { index, pressed } => controller.simulate_button(index, pressed)?,
        Op::Studio { running } => controller.set_fake_studio(running)?,
        Op::Hardware { snapshot } => return resources.hardware_exchange(snapshot),
        Op::Usage { batch } => return resources.usage_exchange(batch),
        Op::Shutdown {} => {
            controller.shutdown();
            resources.shutdown();
        }
        Op::Previews { faces, clock } => {
            if faces.len() != 14 || clock.len() > 16 || clock.chars().any(char::is_control) {
                return Err("Invalid dock preview request".into());
            }
            let mut state = ProfileState::default();
            for (key, face) in state.profiles[0].pages[0].keys.iter_mut().zip(&faces) {
                key.a = face.clone();
            }
            state.validate().map_err(|e| e.to_string())?;
            let mut inputs = resources.inputs();
            inputs.clock = Some(clock);
            let mut frames = Vec::with_capacity(14);
            for (index, face) in faces.iter().enumerate() {
                let frame = resources.render_one(face, index, &inputs)?;
                frames.push(PreviewFrame {
                    index,
                    width: frame.width,
                    height: frame.height,
                    png: frame.png,
                    unknown: frame.unknown,
                    valid_until_ms: frame.valid_until_ms,
                });
            }
            guard_previews(&mut frames, super::scheduler::now_ms())?;
            return serde_json::to_value(frames).map_err(|_| "Dock preview encoding failed".into());
        }
    }
    Ok(Value::Null)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewFrame {
    index: usize,
    width: u32,
    height: u32,
    png: Vec<u8>,
    unknown: bool,
    valid_until_ms: Option<u64>,
}
fn guard_previews(frames: &mut [PreviewFrame], now: u64) -> Result<(), String> {
    if frames.len() != 14 {
        return Err("Invalid dock preview count".into());
    }
    for (index, frame) in frames.iter_mut().enumerate() {
        let size = if index == 13 {
            super::key_render::WIDE_SIZE
        } else {
            super::key_render::KEY_SIZE
        };
        if frame.index != index || (frame.width, frame.height) != size {
            return Err("Invalid dock preview dimensions".into());
        }
        let mut rendered = super::key_render::RenderedFace {
            width: frame.width,
            height: frame.height,
            png: std::mem::take(&mut frame.png),
            unknown: frame.unknown,
            valid_until_ms: frame.valid_until_ms,
        };
        rendered.guard_delivery(now)?;
        frame.png = rendered.png;
        frame.unknown = rendered.unknown;
        frame.valid_until_ms = rendered.valid_until_ms;
    }
    Ok(())
}
fn preview_output(mut frames: Vec<PreviewFrame>, now: u64) -> Result<Value, String> {
    guard_previews(&mut frames, now)?;
    Ok(Value::Array(frames.into_iter().map(|f|json!({"index":f.index,"width":f.width,"height":f.height,"png":f.png,"unknown":f.unknown})).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saturated_original_session_bounds_completion_without_repeating_target_effect() {
        let cross = CrossActions::new();
        let effects = Arc::new(AtomicUsize::new(0));
        let observed = effects.clone();
        cross.state.lock().unwrap().handler = Some(Arc::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));
        let (input, output) = mpsc::sync_channel(1);
        input
            .send(Request {
                version: VERSION,
                id: 7,
                request: Op::Resume {},
            })
            .unwrap();
        let input = Arc::new(input);
        let started = Instant::now();
        cross
            .admit(
                super::super::actions::CoolerAction::ThemeNext {},
                1,
                input.clone(),
            )
            .unwrap();
        while cross.state.lock().unwrap().pending > 0 {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "completion enqueue must be bounded"
            );
            std::thread::yield_now();
        }
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        assert_eq!(output.try_recv().unwrap().id, 7);
        assert!(
            output.try_recv().is_err(),
            "expired completion must not be replayed later"
        );
        cross
            .admit(super::super::actions::CoolerAction::ThemeNext {}, 2, input)
            .unwrap();
        assert!(matches!(
            output.recv_timeout(Duration::from_secs(3)).unwrap().request,
            Op::ActionResult {
                action_id: 2,
                result: Ok(())
            }
        ));
        assert_eq!(
            effects.load(Ordering::SeqCst),
            2,
            "a fresh request executes once after delivery congestion"
        );
    }
    #[test]
    fn cross_action_ids_never_replay_or_complete_a_new_request() {
        let mut highest = 0;
        assert!(!claim_action_id(&mut highest, 0));
        assert!(claim_action_id(&mut highest, 2));
        assert!(!claim_action_id(&mut highest, 2));
        assert!(!claim_action_id(&mut highest, 1));
        assert!(!claim_action_id(&mut highest, u64::MAX));
        assert!(claim_action_id(&mut highest, 3));
        // A fresh session has its own channel and generation-scoped watermark.
        let mut replacement = 0;
        assert!(claim_action_id(&mut replacement, 1));
        assert_eq!(highest, 3);
    }
    #[test]
    fn cross_action_survives_requester_disconnect_and_holds_exit_barrier() {
        let cross = CrossActions::new();
        let (entered_tx, entered) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let released = Mutex::new(released);
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        cross.state.lock().unwrap().handler = Some(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            entered_tx.send(()).unwrap();
            released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            Ok(())
        }));
        let (input, output) = mpsc::sync_channel(4);
        cross
            .admit(
                super::super::actions::CoolerAction::ThemeNext {},
                42,
                Arc::new(input),
            )
            .unwrap();
        entered.recv_timeout(Duration::from_secs(3)).unwrap();
        drop(output); // requesting dock crashed, target operation still owns admission
        assert!(CrossBarrier::close(cross.state.clone()).is_err());
        assert_eq!(cross.state.lock().unwrap().pending, 1);
        release.send(()).unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while cross.state.lock().unwrap().pending != 0 {
            assert!(Instant::now() < end);
            std::thread::yield_now();
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let barrier = CrossBarrier::close(cross.state.clone()).unwrap();
        let (input, _output) = mpsc::sync_channel(4);
        assert!(cross
            .admit(
                super::super::actions::CoolerAction::ThemeNext {},
                43,
                Arc::new(input)
            )
            .is_err());
        drop(barrier);
        assert!(!cross.state.lock().unwrap().closed);
    }
    #[test]
    fn cross_action_result_has_original_id_and_strict_wire_schema() {
        let cross = CrossActions::new();
        cross.state.lock().unwrap().handler = Some(Arc::new(|_| Err("target refused".into())));
        let (input, output) = mpsc::sync_channel(4);
        cross
            .admit(
                super::super::actions::CoolerAction::ThemePrevious {},
                99,
                Arc::new(input),
            )
            .unwrap();
        let message = output.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(matches!(
            message.request,
            Op::ActionResult {
                action_id: 99,
                result: Err(_)
            }
        ));
        assert!(serde_json::from_value::<Message>(json!({"type":"cooler_action","version":VERSION,"action_id":1,"action":{"op":"theme_next","extra":true}})).is_err());
    }

    fn controller() -> (tempfile::TempDir, Arc<Controller>) {
        let home = tempfile::tempdir().unwrap();
        let profiles = Arc::new(ProfileStore::new(home.path()));
        let c = Controller::new(
            profiles,
            home.path().into(),
            home.path().join("resources"),
            true,
        );
        (home, c)
    }
    fn frames(deadline: u64) -> Vec<PreviewFrame> {
        (0..14)
            .map(|index| {
                let (width, height) = if index == 13 {
                    super::super::key_render::WIDE_SIZE
                } else {
                    super::super::key_render::KEY_SIZE
                };
                PreviewFrame {
                    index,
                    width,
                    height,
                    png: vec![1, 2, 3],
                    unknown: false,
                    valid_until_ms: Some(deadline),
                }
            })
            .collect()
    }
    #[test]
    fn construction_and_disabled_configuration_do_not_start_processes_or_profiles() {
        let (home, c) = controller();
        c.configure(false, 35).unwrap();
        assert_eq!(c.worker_status()["generation"], 0);
        assert!(c.worker_status()["pid"].is_null());
        assert!(!home.path().join("profiles").exists());
        assert!(!c.busy());
        c.shutdown();
    }
    #[test]
    fn protocol_rejects_unknown_operations_fields_and_unterminated_frames() {
        assert!(serde_json::from_str::<Request>(
            r#"{"version":1,"id":1,"request":{"op":"shell","value":"anything"}}"#
        )
        .is_err());
        assert!(serde_json::from_str::<Request>(
            r#"{"version":1,"id":1,"request":{"op":"shutdown","extra":1}}"#
        )
        .is_err());
        assert!(read_frame(&mut std::io::Cursor::new(b"{}")).is_err());
        let mut input = std::io::Cursor::new(b"{}\n[]\n");
        assert_eq!(read_frame(&mut input).unwrap().unwrap(), b"{}\n");
        assert_eq!(read_frame(&mut input).unwrap().unwrap(), b"[]\n");
        assert!(read_frame(&mut input).unwrap().is_none());
    }
    #[test]
    fn stalled_heartbeat_and_uncertain_outcome_hide_verified_state_and_usage() {
        let (_home, c) = controller();
        c.enabled.store(true, Ordering::SeqCst);
        {
            let mut cache = c.cache.lock().unwrap();
            cache.alive = true;
            cache.status.state = "connected".into();
            cache.status.current_page = Some("page-private".into());
            cache.usage = json!({"reader":{"code":"ready"}});
            cache.last_status = Instant::now() - Duration::from_secs(4);
        }
        assert_eq!(c.status().state, "restarting");
        assert!(c.status().current_page.is_none());
        assert_eq!(c.usage_status()["reader"]["code"], "inactive");
        assert!(c.busy());
        assert!(c.prepare_exit().is_err());
        {
            let mut cache = c.cache.lock().unwrap();
            cache.last_status = Instant::now();
            cache.uncertain = true;
        }
        assert!(c.status().current_page.is_none());
        assert_eq!(c.usage_status()["reader"]["code"], "inactive");
        c.cache.lock().unwrap().alive = false;
        c.enabled.store(false, Ordering::SeqCst);
    }
    #[test]
    fn current_pixels_expiring_during_ipc_are_replaced_at_host_receipt() {
        let wire = serde_json::to_value(frames(200)).unwrap();
        let before = preview_output(serde_json::from_value(wire.clone()).unwrap(), 199).unwrap();
        assert_eq!(before[0]["unknown"], false);
        let after = preview_output(serde_json::from_value(wire).unwrap(), 200).unwrap();
        assert!(after
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["unknown"] == true));
        assert!(after
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["png"].as_array().unwrap().len() > 3));
        assert!(after[0].get("valid_until_ms").is_none());
    }
    #[test]
    fn malformed_preview_batch_cannot_reach_editor() {
        let mut batch = frames(u64::MAX);
        batch[13].width = 120;
        assert!(preview_output(batch, 1).is_err());
        assert!(preview_output(Vec::new(), 1).is_err());
    }
    #[test]
    fn failed_configuration_does_not_change_last_acknowledged_settings() {
        let (_home, c) = controller();
        c.configure(false, 30).unwrap();
        assert!(c.configure(true, 101).is_err());
        assert!(!c.enabled.load(Ordering::SeqCst));
        assert_eq!(c.brightness.load(Ordering::SeqCst), 30);
        c.quiesced.store(true, Ordering::SeqCst);
        assert!(c.configure(true, 60).is_err());
        assert!(!c.enabled.load(Ordering::SeqCst));
        assert_eq!(c.brightness.load(Ordering::SeqCst), 30);
    }
    #[test]
    fn deferred_profiles_retry_latest_and_skip_already_applied_revision() {
        let (_home, c) = controller();
        let (mut state, _) = c.profiles.state_with_revision().unwrap();
        state.profiles[0].name = "Latest host edit".into();
        let (_, revision) = c.profiles.save_with_revision(&state).unwrap();
        c.quiesced.store(true, Ordering::SeqCst);
        c.notify_profiles_changed();
        assert!(c.profiles_dirty.load(Ordering::SeqCst));
        c.quiesced.store(false, Ordering::SeqCst);
        {
            let mut cache = c.cache.lock().unwrap();
            cache.alive = true;
            cache.last_status = Instant::now();
            cache.profile_revision = Some(revision);
        }
        c.sync_profiles().unwrap();
        assert!(!c.profiles_dirty.load(Ordering::SeqCst));
        assert_eq!(c.worker_status()["generation"], 0);
        c.cache.lock().unwrap().alive = false;
    }
}
