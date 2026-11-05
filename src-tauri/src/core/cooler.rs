//! A supervised CoolerScreen process. Only inherited anonymous pipes carry RPC;
//! no network command listener, shell command, USB packet or credentials cross it.
use super::error::{CoreError, CoreResult};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};

const MAX_FRAME: usize = 32 * 1024 * 1024;
type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<CoreResult<Value>>>>>;
pub type EventSink = Arc<dyn Fn(&str, Value) + Send + Sync>;

struct Session {
    child: Child,
    input: Option<Arc<Mutex<ChildStdin>>>,
    pending: Pending,
    alive: Arc<AtomicBool>,
}
struct State {
    enabled: bool,
    session: Option<Session>,
    phase: &'static str,
    message: String,
    engine_version: Option<String>,
    job_busy: bool,
    jobs: HashSet<String>,
    uncertain: HashSet<u64>,
    ready: bool,
    next_start: Instant,
    failures: u32,
    generation: u64,
}
pub struct Cooler {
    state: Mutex<State>,
    operation: RwLock<()>,
    inflight: AtomicUsize,
    ids: AtomicU64,
    stopped: AtomicBool,
    executable: PathBuf,
    home: PathBuf,
    resources: PathBuf,
    fake: bool,
    events: EventSink,
    transfers: Mutex<super::cooler_transfer::Transfers>,
    finishing_transfers: Mutex<HashMap<u64, String>>,
    diagnostics: super::cooler_health::Diagnostics,
    health_blocked: AtomicBool,
    probe_id: AtomicU64,
    probe_active_id: AtomicU64,
    hardware_cache: Mutex<(Instant, Option<u32>, Value)>,
    services_cache: Mutex<(Instant, Option<u32>, Value)>,
}

/// Normal commands may briefly share the pipe with an ephemeral snapshot.
/// Wait through ordinary scheduling contention, but never wait indefinitely on
/// a stalled writer. Refusal happens before any durable operation is admitted.
fn acquire_command_pipe<T>(pipe: &Mutex<T>) -> CoreResult<std::sync::MutexGuard<'_, T>> {
    let deadline = Instant::now() + Duration::from_millis(250);
    loop {
        match pipe.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1))
            }
            _ => {
                return Err(CoreError::coded(
                    "busy",
                    "The cooler engine pipe is busy; retry when it responds.",
                ))
            }
        }
    }
}

impl Cooler {
    pub fn new(home: PathBuf, resources: PathBuf, fake: bool, events: EventSink) -> Arc<Self> {
        let executable = if cfg!(debug_assertions) {
            std::env::var_os("UC_COOLER_WORKER_EXE").map(PathBuf::from)
        } else {
            None
        }
        .unwrap_or_else(|| resources.join("cooler").join("uc-cooler-worker.exe"));
        let this = Arc::new(Self {
            state: Mutex::new(State {
                enabled: false,
                session: None,
                phase: "disabled",
                message: "Cooler screen is disabled".into(),
                engine_version: None,
                job_busy: false,
                jobs: HashSet::new(),
                uncertain: HashSet::new(),
                ready: false,
                next_start: Instant::now(),
                failures: 0,
                generation: 0,
            }),
            operation: RwLock::new(()),
            inflight: AtomicUsize::new(0),
            ids: AtomicU64::new(1),
            stopped: AtomicBool::new(false),
            transfers: Mutex::new(super::cooler_transfer::Transfers::new(
                home.join("transfers"),
            )),
            finishing_transfers: Mutex::new(HashMap::new()),
            diagnostics: super::cooler_health::Diagnostics::new(
                home.join("connection-health.jsonl"),
            ),
            health_blocked: AtomicBool::new(false),
            probe_id: AtomicU64::new(0),
            probe_active_id: AtomicU64::new(0),
            hardware_cache: Mutex::new((Instant::now(), None, Value::Null)),
            services_cache: Mutex::new((Instant::now(), None, Value::Null)),
            executable,
            home,
            resources,
            fake,
            events,
        });
        let weak = Arc::downgrade(&this);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(250));
            let Some(this) = weak.upgrade() else { break };
            if this.stopped.load(Ordering::SeqCst) {
                break;
            }
            this.supervise();
        });
        let weak = Arc::downgrade(&this);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(250));
            let Some(this) = weak.upgrade() else { break };
            if this.stopped.load(Ordering::SeqCst) {
                break;
            }
            this.sample_health();
        });
        this
    }
    pub fn set_diagnostics(&self, enabled: bool) {
        self.diagnostics.set_enabled(enabled);
    }
    pub fn status(&self) -> Value {
        let state = self.state.lock().unwrap();
        let hardware = self.hardware_cache.lock().unwrap();
        let mut hardware_state = hardware.2.clone();
        if hardware.1 != state.session.as_ref().map(|session| session.child.id())
            || !hardware_state.is_object()
        {
            hardware_state = json!({"shared":true,"fresh":false,"accepted_sequence":0,"local_polls":null,"local_legacy_polls":null,"local_metric_polls":null,"cpu_present":false,"ram_present":false,"gpu_fields":0,"demand":super::hardware::Demand::default(),"expires_at_ms":null});
        }
        if !state.enabled
            || !state.ready
            || hardware.0.elapsed() > Duration::from_secs(3)
            || hardware_state["expires_at_ms"]
                .as_u64()
                .is_none_or(|at| at <= super::scheduler::now_ms())
        {
            hardware_state["fresh"] = json!(false);
            hardware_state["cpu_present"] = json!(false);
            hardware_state["ram_present"] = json!(false);
            hardware_state["gpu_fields"] = json!(0);
        }
        json!({"enabled":state.enabled,"state":state.phase,"message":state.message,"engineVersion":state.engine_version,"busy":state.job_busy||!state.jobs.is_empty()||!state.uncertain.is_empty()||self.transfers.lock().unwrap().busy()||self.inflight.load(Ordering::SeqCst)>0,"fake":self.fake,"pid":state.session.as_ref().map(|s|s.child.id()),"diagnosticsEnabled":self.diagnostics.enabled(),"health":self.diagnostics.status(),"hardware":hardware_state})
    }
    pub fn busy(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.job_busy
            || !state.jobs.is_empty()
            || !state.uncertain.is_empty()
            || self.inflight.load(Ordering::SeqCst) > 0
            || self.transfers.lock().unwrap().busy()
            || (state.enabled && state.session.is_some() && !state.ready)
    }
    pub fn configure(self: &Arc<Self>, enabled: bool) -> CoreResult<Value> {
        let _operation = self.operation.write().unwrap();
        if enabled {
            if self.stopped.load(Ordering::SeqCst) {
                return Err(CoreError::new("The app is closing"));
            }
            let mut state = self.state.lock().unwrap();
            if !state.enabled {
                state.enabled = true;
                state.phase = "starting";
                state.message = "Starting CoolerScreen engine".into();
                state.next_start = Instant::now();
                state.failures = 0;
            }
            drop(state);
            self.start_if_due();
        } else {
            self.stop_session()?;
            let mut state = self.state.lock().unwrap();
            state.enabled = false;
            state.phase = "disabled";
            state.message = "Cooler screen is disabled".into();
            state.job_busy = false;
            state.ready = false;
        }
        Ok(self.status())
    }
    fn worker_command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("UC_COOLER_WORKER", "1")
            .env("UC_SHARED_HARDWARE", "1")
            .env("UC_SHARED_SERVICES", "1")
            .env("COOLERSCREEN_HOME", &self.home)
            .env("COOLERSCREEN_SOURCES_DIR", self.home.join("Sources"))
            .env("CS_FFMPEG", self.resources.join("cooler/ffmpeg.exe"))
            .env(
                "COOLERSCREEN_SCRIPTS",
                self.resources.join("cooler/scripts"),
            );
        // Do not inherit test/development source overrides into a real engine.
        for key in [
            "COOLERSCREEN_FAKE_DEVICE",
            "COOLERSCREEN_FAKE_MEDIA",
            "COOLERSCREEN_GALLERY",
            "COOLERSCREEN_LIVE_NO_DEVICE",
            "COOLERSCREEN_DUMP_FRAMES",
        ] {
            command.env_remove(key);
        }
        if self.fake {
            command
                .env("COOLERSCREEN_FAKE_DEVICE", "winusb")
                .env("COOLERSCREEN_FAKE_MEDIA", "1");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
    }
    fn start_if_due(self: &Arc<Self>) {
        let mut state = self.state.lock().unwrap();
        if !state.enabled
            || state.session.is_some()
            || Instant::now() < state.next_start
            || self.stopped.load(Ordering::SeqCst)
        {
            return;
        }
        if !self.executable.is_file() {
            state.phase = "unavailable";
            state.message = "The CoolerScreen engine is missing. Reinstall Unified Control.".into();
            state.next_start = Instant::now() + Duration::from_secs(30);
            return;
        }
        if let Err(_) = std::fs::create_dir_all(&self.home) {
            state.phase = "error";
            state.message = "The cooler settings folder could not be opened.".into();
            state.next_start = Instant::now() + Duration::from_secs(30);
            return;
        }
        let mut command = self.worker_command();
        match command.spawn() {
            Ok(mut child) => {
                // Do not put the engine in the host's kill-on-close job. Parent
                // loss must deliver EOF and let a noncancellable upload settle.
                let input = Arc::new(Mutex::new(child.stdin.take().unwrap()));
                let output = child.stdout.take().unwrap();
                let pending = Arc::new(Mutex::new(HashMap::new()));
                let alive = Arc::new(AtomicBool::new(true));
                let weak = Arc::downgrade(self);
                let waiting = pending.clone();
                let live = alive.clone();
                state.session = Some(Session {
                    child,
                    input: Some(input),
                    pending,
                    alive,
                });
                state.generation = state.generation.saturating_add(1);
                self.health_blocked.store(false, Ordering::SeqCst);
                self.probe_id.store(0, Ordering::SeqCst);
                self.probe_active_id.store(0, Ordering::SeqCst);
                state.ready = false;
                state.phase = "starting";
                state.message = "Starting CoolerScreen engine".into();
                std::thread::spawn(move || {
                    read_worker(BufReader::new(output), weak, waiting, live)
                });
            }
            Err(_) => {
                state.phase = "error";
                state.message = "The CoolerScreen engine could not start.".into();
                state.next_start = Instant::now() + Duration::from_secs(5);
            }
        }
    }
    fn supervise(self: &Arc<Self>) {
        let Ok(_operation) = self.operation.try_write() else {
            return;
        };
        let mut state = self.state.lock().unwrap();
        let ended = state
            .session
            .as_mut()
            .is_some_and(|s| s.child.try_wait().ok().flatten().is_some());
        if !ended
            && state
                .session
                .as_ref()
                .is_some_and(|s| !s.alive.load(Ordering::SeqCst))
        {
            // A broken pipe is not proof the worker or its upload has stopped.
            // EOF asks it to settle admitted work; retain the process/busy guard.
            if let Some(session) = state.session.as_mut() {
                session.input.take();
            }
            state.ready = false;
            state.job_busy = true;
            state.phase = "stopping";
            state.message = "Cooler engine connection lost; waiting for its work to settle.".into();
            return;
        }
        if ended {
            if let Some(session) = state.session.take() {
                fail_pending(&session.pending);
            }
            for (_, id) in self.finishing_transfers.lock().unwrap().drain() {
                let _ = self.transfers.lock().unwrap().complete(&id, true);
            }
            state.ready = false;
            state.job_busy = false;
            state.jobs.clear();
            state.uncertain.clear();
            state.failures = state.failures.saturating_add(1);
            state.phase = "reconnecting";
            state.message = "CoolerScreen engine stopped; retrying independently.".into();
            state.next_start =
                Instant::now() + Duration::from_secs((1u64 << state.failures.min(5)).min(30));
        }
        drop(state);
        self.start_if_due();
    }
    pub fn call(&self, command: &str, args: Value) -> CoreResult<Value> {
        if !args.is_object()
            || command.len() > 64
            || !command.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            || matches!(
                command,
                "shutdown" | "ready" | "hardware_exchange" | "services_exchange"
            )
        {
            return Err(CoreError::new("Invalid cooler command"));
        }
        let _operation = self.operation.read().unwrap();
        if command == "import_abort" {
            return self
                .transfers
                .lock()
                .unwrap()
                .abort(args["transferId"].as_str().unwrap_or(""));
        }
        {
            let state = self.state.lock().unwrap();
            if !state.ready {
                return Err(CoreError::new(format!(
                    "The cooler engine is not ready ({})",
                    state.phase
                )));
            }
        }
        match command {
            "import_begin" => return self.transfers.lock().unwrap().begin(&args),
            "import_append" => return self.transfers.lock().unwrap().append(&args),
            "import_abort" => {
                return self
                    .transfers
                    .lock()
                    .unwrap()
                    .abort(args["transferId"].as_str().unwrap_or(""))
            }
            "import_finish" => {
                let id = args["transferId"]
                    .as_str()
                    .ok_or_else(|| CoreError::new("Invalid file transfer"))?;
                let (path, kind, original_id) = self.transfers.lock().unwrap().finish(id)?;
                let result = if kind == "asset" {
                    self.request_with_transfer(
                        "theme_asset_add",
                        json!({"originalId":original_id,"path":path}),
                        Duration::from_secs(120),
                        Some(id),
                    )
                } else {
                    self.request_with_transfer(
                        "import_path",
                        json!({"path":path}),
                        Duration::from_secs(120),
                        Some(id),
                    )
                };
                // A timeout/broken pipe does not prove the worker stopped reading.
                // Keep the stage busy until its late reply or confirmed exit.
                if !result.as_ref().is_err_and(|e| {
                    matches!(
                        e.code.as_deref(),
                        Some("cooler_timeout" | "cooler_disconnected")
                    )
                }) {
                    self.transfers
                        .lock()
                        .unwrap()
                        .complete(id, result.is_ok())?;
                }
                return result;
            }
            _ => {}
        }
        self.request(command, args, Duration::from_secs(120))
    }
    fn sample_health(&self) {
        let Some(ticket) = self.diagnostics.begin(Instant::now()) else {
            return;
        };
        let worker = {
            let state = self.state.lock().unwrap();
            json!({"enabled":state.enabled,"ready":state.ready,"state":state.phase,"generation":state.generation,"restarts":state.generation.saturating_sub(1)})
        };
        let pnp = if ticket.sample_pnp {
            Some(super::cooler_health::pnp_snapshot(self.fake))
        } else {
            None
        };
        let reply = if worker["enabled"] != true || worker["ready"] != true {
            Err("worker_unavailable")
        } else if let Ok(_operation) = self.operation.try_read() {
            self.health_request()
        } else {
            Err("sample_deferred")
        };
        self.diagnostics
            .finish(ticket, &worker, reply, pnp, Instant::now());
    }
    /// Read-only probes use distinct IDs/pending entries but never mutation
    /// uncertainty or job busy flags. A lost health reply cannot prevent Quit.
    fn health_request(&self) -> Result<Value, &'static str> {
        self.probe_request("health_status", json!({}), Duration::from_secs(2))
    }
    /// Hardware exchange has only ephemeral in-memory effects. Like health
    /// sampling, its timeout must not create mutation uncertainty or block Quit.
    pub fn hardware_exchange(
        &self,
        snapshot: Option<super::hardware::Snapshot>,
    ) -> CoreResult<Value> {
        let _operation = self
            .operation
            .try_read()
            .map_err(|_| CoreError::new("Hardware exchange is deferred"))?;
        let pid = self
            .state
            .lock()
            .unwrap()
            .session
            .as_ref()
            .map(|session| session.child.id());
        let result = self
            .probe_request(
                "hardware_exchange",
                json!({"snapshot":snapshot}),
                Duration::from_millis(500),
            )
            .map_err(|_| CoreError::new("Hardware exchange is deferred"))?;
        let state = self.state.lock().unwrap();
        if !state.ready || state.session.as_ref().map(|session| session.child.id()) != pid {
            return Err(CoreError::new(
                "Hardware exchange belongs to a disconnected worker",
            ));
        }
        *self.hardware_cache.lock().unwrap() = (Instant::now(), pid, result["state"].clone());
        Ok(result)
    }
    /// Shared music/weather updates are ephemeral, independently scoped to the
    /// current cooler process. Inspection never sends a request or renews demand.
    pub fn services_state(&self) -> Value {
        let state = self.state.lock().unwrap();
        let cache = self.services_cache.lock().unwrap();
        let mut value = cache.2.clone();
        if cache.1 != state.session.as_ref().map(|session| session.child.id()) || !value.is_object()
        {
            return json!({"shared":true,"fresh":false,"music_present":false,"weather_present":false,
                "accepted_sequence":0,"expires_at_ms":null,"local_music_starts":null,"local_weather_fetches":null});
        }
        if !state.enabled
            || !state.ready
            || cache.0.elapsed() > Duration::from_secs(3)
            || value["expires_at_ms"]
                .as_u64()
                .is_none_or(|at| at <= super::scheduler::now_ms())
        {
            value["fresh"] = json!(false);
            value["music_present"] = json!(false);
            value["weather_present"] = json!(false);
        }
        value
    }
    pub fn services_exchange(&self, snapshot: Option<Value>) -> CoreResult<Value> {
        let _operation = self
            .operation
            .try_read()
            .map_err(|_| CoreError::new("Services exchange is deferred"))?;
        let pid = self
            .state
            .lock()
            .unwrap()
            .session
            .as_ref()
            .map(|session| session.child.id());
        let result = self
            .probe_request(
                "services_exchange",
                json!({"snapshot":snapshot}),
                Duration::from_millis(500),
            )
            .map_err(|_| CoreError::new("Services exchange is deferred"))?;
        let state = self.state.lock().unwrap();
        if !state.ready || state.session.as_ref().map(|session| session.child.id()) != pid {
            return Err(CoreError::new(
                "Services exchange belongs to a disconnected worker",
            ));
        }
        *self.services_cache.lock().unwrap() = (Instant::now(), pid, result["state"].clone());
        Ok(result)
    }
    fn probe_request(
        &self,
        command: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<Value, &'static str> {
        // Permit one tiny ephemeral probe at a time. A timeout keeps the gate
        // closed until that exact reply, so other RPCs cannot accumulate probes.
        if self.health_blocked.load(Ordering::SeqCst) {
            return Err(if self.probe_active_id.load(Ordering::SeqCst) != 0 {
                "sample_deferred"
            } else {
                "worker_timeout"
            });
        }
        if self.inflight.load(Ordering::SeqCst) > 0 {
            return Err("sample_deferred");
        }
        let id = self.ids.fetch_add(1, Ordering::SeqCst);
        let (input, pending) = {
            let state = self.state.lock().unwrap();
            if !state.enabled || !state.ready {
                return Err("worker_unavailable");
            }
            if !state.uncertain.is_empty() {
                return Err("sample_deferred");
            }
            let Some(session) = state.session.as_ref() else {
                return Err("worker_unavailable");
            };
            let Some(input) = session.input.clone() else {
                return Err("worker_unavailable");
            };
            (input, session.pending.clone())
        };
        let Ok(input_guard) = input.try_lock() else {
            return Err("sample_deferred");
        };
        if self.inflight.load(Ordering::SeqCst) > 0
            || !pending.lock().unwrap().is_empty()
            || !self.state.lock().unwrap().uncertain.is_empty()
        {
            return Err("sample_deferred");
        }
        if self.health_blocked.swap(true, Ordering::SeqCst) {
            return Err("worker_timeout");
        }
        self.probe_id.store(id, Ordering::SeqCst);
        self.probe_active_id.store(id, Ordering::SeqCst);
        struct ActiveProbe<'a>(&'a AtomicU64, u64);
        impl Drop for ActiveProbe<'_> {
            fn drop(&mut self) {
                let _ = self
                    .0
                    .compare_exchange(self.1, 0, Ordering::SeqCst, Ordering::SeqCst);
            }
        }
        let _active = ActiveProbe(&self.probe_active_id, id);
        let (send, recv) = mpsc::channel();
        pending.lock().unwrap().insert(id, send);
        let mut frame = serde_json::to_vec(&json!({"v":1,"id":id,"command":command,"args":args}))
            .map_err(|_| "invalid_probe")?;
        let limit = if command == "services_exchange" {
            8 * 1024 * 1024
        } else {
            2_048
        };
        if frame.len() > limit {
            pending.lock().unwrap().remove(&id);
            self.health_blocked.store(false, Ordering::SeqCst);
            self.probe_id.store(0, Ordering::SeqCst);
            return Err("invalid_probe");
        }
        frame.push(b'\n');
        // A stalled consumer must not make a larger cover transfer block this
        // caller before its timeout starts. The exact-ID gate permits only one
        // writer until reply or process replacement; mutation busy is untouched.
        drop(input_guard);
        let pending_write = pending.clone();
        std::thread::spawn(move || {
            let written = input.lock().map_err(|_| ()).and_then(|mut writer| {
                writer
                    .write_all(&frame)
                    .and_then(|_| writer.flush())
                    .map_err(|_| ())
            });
            if written.is_err() {
                if let Some(send) = pending_write.lock().unwrap().remove(&id) {
                    let _ = send.send(Err(CoreError::new("Worker pipe disconnected")));
                }
            }
        });
        let result = match recv.recv_timeout(timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err("worker_unavailable"),
            Err(_) => Err("worker_timeout"),
        };
        pending.lock().unwrap().remove(&id);
        result
    }
    fn accept_probe_reply(&self, id: u64) {
        if self
            .probe_id
            .compare_exchange(id, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.health_blocked.store(false, Ordering::SeqCst);
        }
    }
    fn request(&self, command: &str, args: Value, timeout: Duration) -> CoreResult<Value> {
        self.request_with_transfer(command, args, timeout, None)
    }
    fn request_with_transfer(
        &self,
        command: &str,
        args: Value,
        timeout: Duration,
        transfer: Option<&str>,
    ) -> CoreResult<Value> {
        let id = self.ids.fetch_add(1, Ordering::SeqCst);
        let (send, recv) = mpsc::channel();
        let (input, pending) = {
            let state = self.state.lock().unwrap();
            if !state.enabled || (!state.ready && command != "shutdown") {
                return Err(CoreError::coded(
                    "cooler_unavailable",
                    state.message.clone(),
                ));
            }
            let session = state
                .session
                .as_ref()
                .ok_or_else(|| CoreError::new("CoolerScreen engine is unavailable"))?;
            (
                session
                    .input
                    .clone()
                    .ok_or_else(|| CoreError::new("The cooler engine is stopping"))?,
                session.pending.clone(),
            )
        };
        let mut frame = serde_json::to_vec(&json!({"v":1,"id":id,"command":command,"args":args}))
            .map_err(|_| CoreError::new("Invalid cooler command"))?;
        if frame.len() > MAX_FRAME {
            return Err(CoreError::new(
                "This file is too large to import through the editor. Use a file path instead.",
            ));
        }
        frame.push(b'\n');
        // A bounded ephemeral writer may still be waiting on a stalled pipe.
        // Refuse before mutation admission instead of waiting on its mutex.
        let mut input = acquire_command_pipe(&input)?;
        pending.lock().unwrap().insert(id, send);
        if let Some(transfer) = transfer {
            self.finishing_transfers
                .lock()
                .unwrap()
                .insert(id, transfer.into());
        }
        self.state.lock().unwrap().uncertain.insert(id);
        self.inflight.fetch_add(1, Ordering::SeqCst);
        let written = input.write_all(&frame).and_then(|_| input.flush());
        drop(input);
        let result = if written.is_ok() {
            recv.recv_timeout(timeout).unwrap_or_else(|_| {
                Err(CoreError::coded(
                    "cooler_timeout",
                    "The cooler engine has not answered yet. Check its status before retrying.",
                ))
            })
        } else {
            Err(CoreError::coded(
                "cooler_disconnected",
                "The cooler engine connection was lost",
            ))
        };
        pending.lock().unwrap().remove(&id);
        self.inflight.fetch_sub(1, Ordering::SeqCst);
        result
    }
    fn stop_session(&self) -> CoreResult<()> {
        if self.busy() {
            return Err(CoreError::coded(
                "busy",
                "CoolerScreen is busy. Wait for its current work to finish.",
            ));
        }
        if self.state.lock().unwrap().session.is_some() {
            self.request("shutdown", json!({}), Duration::from_secs(30))?;
            let mut state = self.state.lock().unwrap();
            if let Some(mut session) = state.session.take() {
                let deadline = Instant::now() + Duration::from_secs(10);
                while session.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(25));
                }
                if session.child.try_wait().ok().flatten().is_none() {
                    state.session = Some(session);
                    return Err(CoreError::coded(
                        "busy",
                        "The cooler engine is still stopping.",
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn shutdown(&self) -> CoreResult<()> {
        let _operation = self.operation.write().unwrap();
        self.stop_session()?;
        self.stopped.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn fail_pending(pending: &Pending) {
    for (_, sender) in pending.lock().unwrap().drain() {
        let _ = sender.send(Err(CoreError::coded(
            "cooler_disconnected",
            "The cooler engine connection was lost",
        )));
    }
}
pub fn other_app_running() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        let handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if handle == INVALID_HANDLE_VALUE {
            return true;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(handle, &mut entry);
        while ok != 0 {
            let len = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("CoolerScreen.exe") {
                found = true;
                break;
            }
            ok = Process32NextW(handle, &mut entry);
        }
        CloseHandle(handle);
        found
    }
    #[cfg(not(windows))]
    {
        false
    }
}
fn read_worker(
    mut reader: impl BufRead,
    owner: Weak<Cooler>,
    pending: Pending,
    alive: Arc<AtomicBool>,
) {
    loop {
        let mut bytes = Vec::new();
        let read = std::io::Read::take(reader.by_ref(), (MAX_FRAME + 1) as u64)
            .read_until(b'\n', &mut bytes);
        if !matches!(read,Ok(n) if n>0&&n<=MAX_FRAME&&bytes.last()==Some(&b'\n')) {
            super::super::device::log("cooler protocol: frame ended or exceeded bound");
            break;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            super::super::device::log("cooler protocol: invalid JSON");
            break;
        };
        if value["v"] != 1 {
            super::super::device::log("cooler protocol: invalid version");
            break;
        }
        if let Some(id) = value["id"].as_u64() {
            if let Some(owner) = owner.upgrade() {
                owner.accept_probe_reply(id);
                owner.state.lock().unwrap().uncertain.remove(&id);
                if let Some(transfer) = owner.finishing_transfers.lock().unwrap().remove(&id) {
                    let _ = owner.transfers.lock().unwrap().complete(&transfer, true);
                }
            }
            if let Some(sender) = pending.lock().unwrap().remove(&id) {
                let result = if value["ok"] == true {
                    Ok(value["result"].clone())
                } else {
                    Err(serde_json::from_value(value["error"].clone())
                        .unwrap_or_else(|_| CoreError::new("The cooler command failed")))
                };
                let _ = sender.send(result);
            }
        } else if let Some(event) = value["event"].as_str() {
            let Some(owner) = owner.upgrade() else { break };
            let payload = value["payload"].clone();
            if event == "ready" {
                let mut state = owner.state.lock().unwrap();
                state.ready = true;
                state.phase = if payload["blocked"] == true {
                    "blocked"
                } else {
                    "running"
                };
                state.message = if payload["blocked"] == true {
                    "Close CoolerScreen from its tray before enabling its engine in Unified Control.".into()
                } else {
                    "CoolerScreen engine is running".into()
                };
                state.engine_version = payload["engineVersion"].as_str().map(String::from);
            }
            if event == "ready" {
                (owner.events)("cs:app", json!({}));
                if payload["blocked"] != true {
                    let owner = owner.clone();
                    std::thread::spawn(move || {
                        let _operation = owner.operation.read().unwrap();
                        let _ = owner.request("activate", json!({}), Duration::from_secs(30));
                    });
                }
            }
            if event == "cs:device" {
                owner.state.lock().unwrap().job_busy = payload["busy"].as_bool().unwrap_or(true);
            }
            if event == "cs:job" {
                if let Some(id) = payload["id"].as_str() {
                    let mut state = owner.state.lock().unwrap();
                    if matches!(payload["state"].as_str(), Some("queued" | "running"))
                        || matches!(
                            payload["status"].as_str(),
                            Some("queued" | "waiting" | "running")
                        )
                    {
                        state.jobs.insert(id.into());
                    } else {
                        state.jobs.remove(id);
                    }
                }
            }
            if matches!(event, "cs:job" | "cs:library" | "cs:device" | "cs:app") {
                (owner.events)(event, payload);
            }
        } else {
            super::super::device::log("cooler protocol: invalid message");
            break;
        }
    }
    alive.store(false, Ordering::SeqCst);
    fail_pending(&pending);
}

#[cfg(all(test, windows))]
mod health_tests {
    use super::*;
    #[test]
    fn unrelated_replies_cannot_reopen_an_unanswered_hardware_probe() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Cooler::new(
            dir.path().join("home"),
            dir.path().join("resources"),
            true,
            Arc::new(|_, _| {}),
        );
        manager.probe_id.store(42, Ordering::SeqCst);
        manager.health_blocked.store(true, Ordering::SeqCst);
        manager.accept_probe_reply(41);
        assert!(manager.health_blocked.load(Ordering::SeqCst));
        manager.accept_probe_reply(42);
        assert!(!manager.health_blocked.load(Ordering::SeqCst));
        assert_eq!(manager.probe_id.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn cached_hardware_flags_use_original_expiry_and_keep_actual_counters() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Cooler::new(
            dir.path().join("home"),
            dir.path().join("resources"),
            true,
            Arc::new(|_, _| {}),
        );
        {
            let mut state = manager.state.lock().unwrap();
            state.enabled = true;
            state.ready = true;
        }
        *manager.hardware_cache.lock().unwrap() = (
            Instant::now(),
            None,
            json!({
                "shared":true,"fresh":true,"cpu_present":true,"ram_present":true,"gpu_fields":255,
                "expires_at_ms":super::super::scheduler::now_ms()-1,"accepted_sequence":7,
                "local_polls":3,"local_legacy_polls":1,"local_metric_polls":2
            }),
        );
        let state = manager.status()["hardware"].clone();
        assert_eq!(state["fresh"], false);
        assert_eq!(state["gpu_fields"], 0);
        assert_eq!(state["cpu_present"], false);
        assert_eq!(state["accepted_sequence"], 7);
        assert_eq!(state["local_polls"], 3);
        assert_eq!(state["local_metric_polls"], 2);
    }
    #[test]
    fn editor_cannot_inject_private_provider_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Cooler::new(
            dir.path().join("home"),
            dir.path().join("resources"),
            true,
            Arc::new(|_, _| {}),
        );
        for command in ["hardware_exchange", "services_exchange"] {
            let error = manager.call(command, json!({"snapshot":null})).unwrap_err();
            assert_eq!(error.message, "Invalid cooler command");
        }
    }
    #[test]
    fn cached_services_expire_without_resetting_actual_source_counters() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Cooler::new(
            dir.path().join("home"),
            dir.path().join("resources"),
            true,
            Arc::new(|_, _| {}),
        );
        {
            let mut state = manager.state.lock().unwrap();
            state.enabled = true;
            state.ready = true;
        }
        *manager.services_cache.lock().unwrap() = (
            Instant::now(),
            None,
            json!({"shared":true,"fresh":true,"music_present":true,"weather_present":true,
                "accepted_sequence":9,"expires_at_ms":super::super::scheduler::now_ms()-1,
                "local_music_starts":0,"local_weather_fetches":0}),
        );
        let state = manager.services_state();
        assert_eq!(state["fresh"], false);
        assert_eq!(state["music_present"], false);
        assert_eq!(state["weather_present"], false);
        assert_eq!(state["accepted_sequence"], 9);
        assert_eq!(state["local_music_starts"], 0);
        assert_eq!(state["local_weather_fetches"], 0);
    }
    #[test]
    fn worker_owns_source_definitions_instead_of_inheriting_a_shared_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let manager = Cooler::new(
            home.clone(),
            dir.path().join("resources"),
            true,
            Arc::new(|_, _| {}),
        );
        let command = manager.worker_command();
        let environment: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment.get(std::ffi::OsStr::new("COOLERSCREEN_HOME")),
            Some(&Some(home.as_os_str()))
        );
        let owned = home.join("Sources");
        assert_eq!(
            environment.get(std::ffi::OsStr::new("COOLERSCREEN_SOURCES_DIR")),
            Some(&Some(owned.as_os_str()))
        );
        // Explicit Some(path) overrides any inherited value; None would remove it.
        assert_eq!(
            environment.get(std::ffi::OsStr::new("COOLERSCREEN_FAKE_DEVICE")),
            Some(&Some(std::ffi::OsStr::new("winusb")))
        );
        manager.shutdown().unwrap();
    }

    #[test]
    fn command_admission_waits_for_short_contention_and_bounds_a_stalled_writer() {
        let pipe = Arc::new(Mutex::new(()));
        let holder = pipe.clone();
        let (ready, waiting) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _guard = holder.lock().unwrap();
            ready.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        });
        waiting.recv().unwrap();
        assert!(acquire_command_pipe(&pipe).is_ok());
        thread.join().unwrap();
        let _held = pipe.lock().unwrap();
        let start = Instant::now();
        assert!(acquire_command_pipe(&pipe).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn large_ephemeral_write_has_a_bounded_timeout_without_mutation_admission() {
        use std::os::windows::process::CommandExt;
        let dir = tempfile::tempdir().unwrap();
        let manager = Cooler::new(
            dir.path().join("home"),
            dir.path().join("resources"),
            true,
            Arc::new(|_, _| {}),
        );
        let mut child = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = Arc::new(Mutex::new(child.stdin.take().unwrap()));
        {
            let mut state = manager.state.lock().unwrap();
            state.enabled = true;
            state.ready = true;
            state.phase = "running";
            state.session = Some(Session {
                child,
                input: Some(input.clone()),
                pending: Arc::new(Mutex::new(HashMap::new())),
                alive: Arc::new(AtomicBool::new(true)),
            });
        }
        let start = Instant::now();
        assert!(manager
            .services_exchange(Some(json!({"padding":"x".repeat(1024*1024)})))
            .is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(!manager.busy());
        assert!(manager.state.lock().unwrap().uncertain.is_empty());
        let writing_deadline = Instant::now() + Duration::from_secs(2);
        while input.try_lock().is_ok() && Instant::now() < writing_deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            input.try_lock().is_err(),
            "fixture writer must be stalled on the non-reading helper"
        );
        let admission = Instant::now();
        assert!(manager
            .request("shutdown", json!({}), Duration::from_millis(100))
            .is_err());
        assert!(admission.elapsed() < Duration::from_secs(1));
        assert!(manager.state.lock().unwrap().uncertain.is_empty());
        manager.stopped.store(true, Ordering::SeqCst);
        let mut session = manager.state.lock().unwrap().session.take().unwrap();
        // Only this hidden non-device test helper is terminated; no app is touched.
        session.child.kill().unwrap();
        session.child.wait().unwrap();
    }
    #[test]
    fn health_timeout_does_not_leave_mutation_or_quit_busy() {
        use std::os::windows::process::CommandExt;
        let dir = tempfile::tempdir().unwrap();
        let manager = Cooler::new(
            dir.path().join("home"),
            dir.path().join("resources"),
            true,
            Arc::new(|_, _| {}),
        );
        // A hidden stdin drain deliberately supplies no RPC response. This is
        // a helper process only: no app, USB, shell file actions or desktop UI.
        let mut child = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::In.ReadToEnd() | Out-Null",
            ])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = Arc::new(Mutex::new(child.stdin.take().unwrap()));
        {
            let mut state = manager.state.lock().unwrap();
            state.enabled = true;
            state.ready = true;
            state.phase = "running";
            state.session = Some(Session {
                child,
                input: Some(input),
                pending: Arc::new(Mutex::new(HashMap::new())),
                alive: Arc::new(AtomicBool::new(true)),
            });
        }
        let started = Instant::now();
        assert_eq!(manager.health_request(), Err("worker_timeout"));
        assert!(started.elapsed() < Duration::from_secs(4));
        assert!(manager.health_blocked.load(Ordering::SeqCst));
        let deferred = Instant::now();
        assert_eq!(manager.health_request(), Err("worker_timeout"));
        assert!(
            deferred.elapsed() < Duration::from_millis(100),
            "a timed-out health probe must suppress further pipe writes"
        );
        assert!(!manager.busy());
        assert_eq!(manager.inflight.load(Ordering::SeqCst), 0);
        assert!(manager.state.lock().unwrap().uncertain.is_empty());
        manager.stopped.store(true, Ordering::SeqCst);
        let mut session = manager.state.lock().unwrap().session.take().unwrap();
        assert!(session.pending.lock().unwrap().is_empty());
        session.input.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        while session.child.try_wait().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            session.child.try_wait().unwrap().is_some(),
            "stdin-drain helper must exit after EOF"
        );
    }
}
