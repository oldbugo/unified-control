//! UC-owned music/weather source process. The process has no device engine,
//! window, account reader or arbitrary-command endpoint. Only leased demand and
//! bounded typed observations cross its inherited private pipes.
use super::cooler::Cooler;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex, Weak,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_FRAME: usize = 8 * 1024 * 1024;
const MAX_PNG: usize = 2 * 1024 * 1024;
const MAX_SAFE: u64 = 9_007_199_254_740_991;
const LEASE: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(125);
const SOURCE_POLL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Demand {
    music: Option<MusicDemand>,
    weather: Option<Place>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct MusicDemand {
    preferred_app: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Place {
    name: String,
    lat: f64,
    lon: f64,
    tz: String,
}
fn text(value: &str, max: usize) -> bool {
    value.chars().count() <= max && !value.chars().any(char::is_control)
}
impl Place {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && text(&self.name, 80)
            && self.lat.is_finite()
            && (-90. ..=90.).contains(&self.lat)
            && self.lon.is_finite()
            && (-180. ..=180.).contains(&self.lon)
            && self.tz.len() <= 64
            && self
                .tz
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_+-".contains(&b))
    }
}
impl Demand {
    fn valid(&self) -> bool {
        self.music
            .as_ref()
            .is_none_or(|m| m.preferred_app.as_ref().is_none_or(|s| text(s, 128)))
            && self.weather.as_ref().is_none_or(Place::valid)
    }
    fn active(&self) -> bool {
        self.music.is_some() || self.weather.is_some()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MusicStatus {
    Inactive,
    Loading,
    Ok,
    None,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WeatherStatus {
    Inactive,
    Loading,
    Ok,
    Stale,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cover {
    id: u64,
    png: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MusicData {
    app: String,
    title: String,
    artist: String,
    playing: bool,
    position_s: f64,
    duration_s: f64,
    state_changed_at_ms: u64,
    track_id: u64,
    cover: Option<Cover>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MusicObservation {
    status: MusicStatus,
    observed_at_ms: Option<u64>,
    preferred_app: Option<String>,
    data: Option<MusicData>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hour {
    hour: u32,
    temp_c: f64,
    code: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Day {
    weekday: u32,
    hi_c: f64,
    lo_c: f64,
    code: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WeatherData {
    place: String,
    temp_c: f64,
    feels_c: f64,
    code: u32,
    is_day: bool,
    hours: Vec<Hour>,
    days: Vec<Day>,
    stale: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WeatherObservation {
    status: WeatherStatus,
    observed_at_ms: Option<u64>,
    place: Option<Place>,
    data: Option<WeatherData>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    generation: u64,
    sequence: u64,
    observed_at_ms: u64,
    expires_at_ms: u64,
    music: MusicObservation,
    weather: WeatherObservation,
}
fn stamp(at: u64, now: u64) -> bool {
    at > 0 && at <= now && at <= MAX_SAFE
}
fn temperature(value: f64) -> bool {
    value.is_finite() && (-150. ..=100.).contains(&value)
}
fn png_valid(bytes: &[u8]) -> bool {
    if bytes.len() < 24
        || bytes.len() > MAX_PNG
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || &bytes[12..16] != b"IHDR"
    {
        return false;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    w > 0 && h > 0 && w <= 640 && h <= 640 && u64::from(w) * u64::from(h) <= 409_600
}
impl Snapshot {
    fn validate(&self, generation: u64, sequence: u64, now: u64) -> Result<(), String> {
        if self.version != 1
            || self.generation != generation
            || self.sequence != sequence
            || generation == 0
            || generation > MAX_SAFE
            || sequence == 0
            || sequence > MAX_SAFE
            || !stamp(self.observed_at_ms, now)
            || self.expires_at_ms > MAX_SAFE
            || self.expires_at_ms <= now
            || self.expires_at_ms <= self.observed_at_ms
            || self.expires_at_ms - self.observed_at_ms > 3_000
        {
            return Err("invalid_snapshot".into());
        }
        if self
            .music
            .preferred_app
            .as_ref()
            .is_some_and(|value| !text(value, 128))
            || self.music.observed_at_ms.is_some_and(|at| !stamp(at, now))
            || self
                .weather
                .observed_at_ms
                .is_some_and(|at| !stamp(at, now))
        {
            return Err("invalid_observation_time".into());
        }
        if let Some(m) = &self.music.data {
            if !matches!(self.music.status, MusicStatus::Ok)
                || self.music.observed_at_ms.is_none()
                || !text(&m.app, 256)
                || !text(&m.title, 512)
                || !text(&m.artist, 512)
                || !m.position_s.is_finite()
                || !(0. ..=604_800.).contains(&m.position_s)
                || !m.duration_s.is_finite()
                || !(0. ..=604_800.).contains(&m.duration_s)
                || !stamp(m.state_changed_at_ms, now)
                || m.track_id == 0
                || m.track_id > MAX_SAFE
                || m.cover
                    .as_ref()
                    .is_some_and(|c| c.id == 0 || c.id > MAX_SAFE || !png_valid(&c.png))
            {
                return Err("invalid_music".into());
            }
        } else if matches!(self.music.status, MusicStatus::Ok) {
            return Err("invalid_music".into());
        }
        if self.weather.place.as_ref().is_some_and(|p| !p.valid()) {
            return Err("invalid_place".into());
        }
        if let Some(w) = &self.weather.data {
            if !matches!(
                self.weather.status,
                WeatherStatus::Ok | WeatherStatus::Stale
            ) || self.weather.observed_at_ms.is_none()
                || self.weather.place.is_none()
                || !text(&w.place, 80)
                || !temperature(w.temp_c)
                || !temperature(w.feels_c)
                || self
                    .weather
                    .place
                    .as_ref()
                    .is_some_and(|place| w.place != place.name)
                || w.stale != matches!(self.weather.status, WeatherStatus::Stale)
                || w.code > 99
                || w.hours.len() > 168
                || w.days.len() > 7
                || w.hours
                    .iter()
                    .any(|h| h.hour > 23 || !temperature(h.temp_c) || h.code > 99)
                || w.days.iter().any(|d| {
                    d.weekday > 6 || !temperature(d.hi_c) || !temperature(d.lo_c) || d.code > 99
                })
            {
                return Err("invalid_weather".into());
            }
        } else if matches!(
            self.weather.status,
            WeatherStatus::Ok | WeatherStatus::Stale
        ) {
            return Err("invalid_weather".into());
        }
        Ok(())
    }
    fn for_demand(mut self, demand: &Demand) -> Self {
        if demand.music.is_none()
            || self.music.preferred_app
                != demand
                    .music
                    .as_ref()
                    .and_then(|music| music.preferred_app.clone())
        {
            self.music = MusicObservation {
                status: if demand.music.is_some() {
                    MusicStatus::Loading
                } else {
                    MusicStatus::Inactive
                },
                observed_at_ms: None,
                preferred_app: demand
                    .music
                    .as_ref()
                    .and_then(|music| music.preferred_app.clone()),
                data: None,
            };
        }
        if self.weather.place != demand.weather {
            self.weather = WeatherObservation {
                status: if demand.weather.is_some() {
                    WeatherStatus::Loading
                } else {
                    WeatherStatus::Inactive
                },
                observed_at_ms: None,
                place: demand.weather.clone(),
                data: None,
            };
        }
        self
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceState {
    music_starts: u64,
    music_reads: u64,
    weather_fetches: u64,
    music_active: bool,
    weather_active: bool,
    fake: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    snapshot: Snapshot,
    state: SourceState,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    v: u32,
    id: u64,
    ok: bool,
    result: Option<Value>,
    error: Option<Value>,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum SourceCommand {
    Exchange,
    TestControl,
    Shutdown,
}
#[derive(Serialize)]
struct Request<T: Serialize> {
    v: u32,
    id: u64,
    command: SourceCommand,
    args: T,
}
#[derive(Serialize)]
struct ExchangeArgs<'a> {
    generation: u64,
    sequence: u64,
    demand: &'a Demand,
}
#[derive(Serialize)]
struct TestArgs {
    music: &'static str,
    weather: &'static str,
}

fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut bytes = Vec::new();
    let n = reader
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "source_pipe_read")?;
    if n == 0 {
        return Ok(None);
    }
    if n > MAX_FRAME || bytes.last() != Some(&b'\n') {
        return Err("source_frame_limit".into());
    }
    Ok(Some(bytes))
}
struct Session {
    child: Child,
    input: Option<mpsc::SyncSender<Vec<u8>>>,
    output: mpsc::Receiver<Result<Reply, String>>,
    ids: u64,
    applied_failure: bool,
    demand: Option<Demand>,
}
impl Session {
    fn spawn(executable: &PathBuf, home: &PathBuf, fake: bool) -> Result<Self, String> {
        std::fs::create_dir_all(home).map_err(|_| "source_home_unavailable")?;
        let mut command = Command::new(executable);
        command
            .arg("--uc-sources-worker")
            .env("UC_SOURCES_WORKER", "1")
            .env("COOLERSCREEN_HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for key in [
            "UC_COOLER_WORKER",
            "UC_SOURCES_FAKE",
            "COOLERSCREEN_FAKE_DEVICE",
            "COOLERSCREEN_FAKE_MEDIA",
            "COOLERSCREEN_GALLERY",
            "COOLERSCREEN_SOURCES_DIR",
            "COOLERSCREEN_LIVE_NO_DEVICE",
            "COOLERSCREEN_DUMP_FRAMES",
        ] {
            command.env_remove(key);
        }
        if fake {
            command.env("UC_SOURCES_FAKE", "1");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| "source_start_failed")?;
        if cfg!(windows) && !super::jobobject::adopt(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("source_containment_failed".into());
        }
        let mut input = child.stdin.take().ok_or("source_pipe_unavailable")?;
        let output = child.stdout.take().ok_or("source_pipe_unavailable")?;
        let (send, receive) = mpsc::sync_channel::<Vec<u8>>(1);
        thread::spawn(move || {
            for bytes in receive {
                if input.write_all(&bytes).and_then(|_| input.flush()).is_err() {
                    break;
                }
            }
        });
        let (send_output, receive_output) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let result = match read_frame(&mut reader) {
                    Ok(Some(bytes)) => serde_json::from_slice::<Reply>(&bytes)
                        .map_err(|_| "invalid_source_reply".into()),
                    Ok(None) => Err("source_disconnected".into()),
                    Err(error) => Err(error),
                };
                let failed = result.is_err();
                if send_output.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input: Some(send),
            output: receive_output,
            ids: 0,
            applied_failure: false,
            demand: None,
        })
    }
    fn request<T: Serialize>(&mut self, command: SourceCommand, args: T) -> Result<Value, String> {
        self.ids = self
            .ids
            .checked_add(1)
            .filter(|id| *id <= MAX_SAFE)
            .ok_or("source_id_limit")?;
        let mut bytes = serde_json::to_vec(&Request {
            v: 1,
            id: self.ids,
            command,
            args,
        })
        .map_err(|_| "invalid_source_request")?;
        if bytes.len() >= MAX_FRAME {
            return Err("source_frame_limit".into());
        }
        bytes.push(b'\n');
        self.input
            .as_ref()
            .ok_or("source_disconnected")?
            .try_send(bytes)
            .map_err(|_| "source_write_deferred")?;
        let reply = self
            .output
            .recv_timeout(Duration::from_millis(1500))
            .map_err(|_| "source_reply_timeout")??;
        if reply.v != 1 || reply.id != self.ids || !reply.ok || reply.error.is_some() {
            return Err("source_request_failed".into());
        }
        reply.result.ok_or_else(|| "invalid_source_reply".into())
    }
    /// Source mode only reads media sessions and HTTP forecasts; it owns no
    /// device writes or user-launched apps. A hung owned source may be ended.
    fn stop(&mut self) -> Result<(), String> {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return Ok(());
        }
        let _ = self.request(SourceCommand::Shutdown, json!({}));
        self.input.take();
        let until = Instant::now() + Duration::from_millis(750);
        while Instant::now() < until {
            if self
                .child
                .try_wait()
                .map_err(|_| "source_wait_failed")?
                .is_some()
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        self.child.kill().map_err(|_| "source_stop_failed")?;
        let until = Instant::now() + Duration::from_secs(1);
        while Instant::now() < until {
            if self
                .child
                .try_wait()
                .map_err(|_| "source_wait_failed")?
                .is_some()
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        Err("source_stop_pending".into())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.input.take();
        // This is our read-only provider, never a device engine or user app.
        // Exception unwinding must not leave an orphan collector.
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
    }
}

struct State {
    phase: &'static str,
    pid: Option<u32>,
    demand: Demand,
    source: Option<SourceState>,
    snapshot: Option<Snapshot>,
    process_starts: u64,
    exchanges: u64,
    deliveries: u64,
    failures: u64,
    sequence: u64,
    last_error: Option<&'static str>,
}
pub struct Service {
    state: Mutex<State>,
    stopped: AtomicBool,
    failure: AtomicBool,
    paused: AtomicBool,
    fake: bool,
    generation: u64,
    thread: Mutex<Option<JoinHandle<Result<(), String>>>>,
    retained: Mutex<Option<Session>>,
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(MAX_SAFE as u128) as u64
}
fn generation() -> u64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = now_ms().max(1);
    let mut old = LAST.load(Ordering::SeqCst);
    loop {
        let new = now.max(old.saturating_add(1)).min(MAX_SAFE);
        match LAST.compare_exchange(old, new, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return new,
            Err(value) => old = value,
        }
    }
}
impl Service {
    pub fn start(home: PathBuf, resources: PathBuf, fake: bool, cooler: Weak<Cooler>) -> Arc<Self> {
        let fake = cfg!(debug_assertions) && fake;
        let executable = if cfg!(debug_assertions) {
            std::env::var_os("UC_COOLER_WORKER_EXE").map(PathBuf::from)
        } else {
            None
        }
        .unwrap_or_else(|| resources.join("cooler/uc-cooler-worker.exe"));
        let this = Arc::new(Self {
            state: Mutex::new(State {
                phase: "inactive",
                pid: None,
                demand: Demand::default(),
                source: None,
                snapshot: None,
                process_starts: 0,
                exchanges: 0,
                deliveries: 0,
                failures: 0,
                sequence: 0,
                last_error: None,
            }),
            stopped: AtomicBool::new(false),
            failure: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            fake,
            generation: generation(),
            thread: Mutex::new(None),
            retained: Mutex::new(None),
        });
        let weak = Arc::downgrade(&this);
        let worker = thread::spawn(move || run(weak, cooler, home, executable, fake));
        *this.thread.lock().unwrap() = Some(worker);
        this
    }
    pub fn set_test_control(&self, failure: bool, paused: bool) -> Result<(), String> {
        if !cfg!(debug_assertions) || !self.fake {
            return Err("Source test controls require a debug fake instance".into());
        }
        if self.stopped.load(Ordering::SeqCst) {
            return Err("Shared sources are stopping".into());
        }
        self.failure.store(failure, Ordering::SeqCst);
        self.paused.store(paused, Ordering::SeqCst);
        Ok(())
    }
    pub fn status(&self) -> Value {
        let state = self.state.lock().unwrap();
        let now = now_ms();
        let current = state.snapshot.as_ref().filter(|s| s.expires_at_ms > now);
        json!({"shared":true,"fake":self.fake,"state":state.phase,"running":state.pid.is_some()&&matches!(state.phase,"starting"|"running"|"idle"),"pid":state.pid,
            "generation":self.generation,"sequence":state.sequence,"process_starts":state.process_starts,"exchanges":state.exchanges,
            "deliveries":state.deliveries,"failures":state.failures,"last_error":state.last_error,
            "music_demand":state.demand.music.is_some(),"weather_demand":state.demand.weather.is_some(),
            "source":state.source,"fresh":current.is_some(),"expires_at_ms":current.map(|s|s.expires_at_ms),
            "music_present":current.is_some_and(|s|matches!(s.music.status,MusicStatus::Ok)&&s.music.data.is_some()&&s.music.observed_at_ms.is_some_and(|at|now.saturating_sub(at)<3_000)),
            "weather_present":current.is_some_and(|s|s.weather.data.is_some()),
            "test_failure":self.failure.load(Ordering::SeqCst),"paused_delivery":self.paused.load(Ordering::SeqCst)})
    }
    pub fn shutdown(&self) -> Result<(), String> {
        self.stopped.store(true, Ordering::SeqCst);
        let mut outcome = Ok(());
        if let Some(worker) = self.thread.lock().unwrap().take() {
            outcome = worker
                .join()
                .map_err(|_| "Shared source supervisor stopped unexpectedly".to_string())?;
        }
        let mut retained = self.retained.lock().unwrap();
        if let Some(child) = retained.as_mut() {
            child.stop()?;
            retained.take();
            let mut state = self.state.lock().unwrap();
            state.phase = "stopped";
            state.pid = None;
            state.snapshot = None;
            state.source = None;
            state.demand = Demand::default();
            outcome = Ok(());
        }
        outcome
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        // The last temporary Arc can be released by the supervisor itself.
        // Never join the current thread from Drop; explicit host shutdown joins.
        self.stopped.store(true, Ordering::SeqCst);
    }
}

/// Keep ownership until an actual exit. A failed stop cannot permit another
/// collector or leave cached observations marked current.
fn retire(session: &mut Option<Session>, service: &Service) -> Result<(), String> {
    if let Some(child) = session.as_mut() {
        if let Err(error) = child.stop() {
            let mut state = service.state.lock().unwrap();
            state.phase = "stopping";
            state.pid = Some(child.child.id());
            state.snapshot = None;
            state.source = None;
            state.failures += 1;
            state.last_error = Some("source_stop_pending");
            return Err(error);
        }
        session.take();
    }
    let mut state = service.state.lock().unwrap();
    state.pid = None;
    state.source = None;
    state.snapshot = None;
    Ok(())
}

fn source_exchange(
    child: &mut Session,
    service: &Service,
    demand: &Demand,
    sequence: u64,
) -> Result<Snapshot, String> {
    let failed = service.failure.load(Ordering::SeqCst);
    if service.fake && child.applied_failure != failed {
        child.request(
            SourceCommand::TestControl,
            TestArgs {
                music: if failed { "error" } else { "ok" },
                weather: if failed { "error" } else { "ok" },
            },
        )?;
        child.applied_failure = failed;
    }
    let value = child.request(
        SourceCommand::Exchange,
        ExchangeArgs {
            generation: service.generation,
            sequence,
            demand,
        },
    )?;
    let exchange: Exchange =
        serde_json::from_value(value).map_err(|_| "invalid_source_snapshot")?;
    exchange
        .snapshot
        .validate(service.generation, sequence, now_ms())?;
    if exchange.state.fake != service.fake
        || [
            exchange.state.music_starts,
            exchange.state.music_reads,
            exchange.state.weather_fetches,
        ]
        .into_iter()
        .any(|value| value > MAX_SAFE)
    {
        return Err("invalid_source_mode".into());
    }
    let snapshot = exchange.snapshot.for_demand(demand);
    child.demand = Some(demand.clone());
    let mut state = service.state.lock().unwrap();
    state.phase = if demand.active() { "running" } else { "idle" };
    state.sequence = sequence;
    state.exchanges += 1;
    state.source = Some(exchange.state);
    state.snapshot = demand.active().then(|| snapshot.clone());
    state.last_error = None;
    Ok(snapshot)
}

fn run(
    owner: Weak<Service>,
    cooler: Weak<Cooler>,
    home: PathBuf,
    executable: PathBuf,
    fake: bool,
) -> Result<(), String> {
    let mut session: Option<Session> = None;
    let mut demand = Demand::default();
    let mut latest: Option<Snapshot> = None;
    let mut delivered = None;
    let mut lease = Instant::now();
    let mut next_source = Instant::now();
    let mut retry = Instant::now();
    let mut backoff = Duration::from_millis(500);
    let mut sequence = 0u64;
    let mut retiring = false;
    loop {
        let Some(service) = owner.upgrade() else {
            break;
        };
        if service.stopped.load(Ordering::SeqCst) {
            break;
        }
        if retiring {
            if retire(&mut session, &service).is_err() {
                drop(service);
                thread::sleep(POLL);
                continue;
            }
            retiring = false;
        }
        let Some(cooler) = cooler.upgrade() else {
            break;
        };
        let status = cooler.status();
        if status["enabled"] == true && status["state"] == "running" {
            let candidate = latest
                .as_ref()
                .filter(|s| s.expires_at_ms > now_ms())
                .cloned();
            let key = candidate
                .as_ref()
                .map(|s| (status["pid"].as_u64(), s.sequence));
            let outgoing = if service.paused.load(Ordering::SeqCst) || key == delivered {
                None
            } else {
                candidate
            };
            if let Ok(reply) = cooler
                .services_exchange(outgoing.as_ref().map(|s| serde_json::to_value(s).unwrap()))
            {
                if let Ok(next) = serde_json::from_value::<Demand>(reply["demand"].clone()) {
                    if next.valid() {
                        lease = Instant::now();
                        if demand != next {
                            latest = None;
                            next_source = Instant::now();
                            demand = next;
                        }
                    }
                }
                if outgoing.is_some() {
                    delivered = key;
                    service.state.lock().unwrap().deliveries += 1;
                }
            }
        } else {
            demand = Demand::default();
            latest = None;
            delivered = None;
        }
        if lease.elapsed() > LEASE {
            demand = Demand::default();
            latest = None;
            delivered = None;
        }
        {
            let mut state = service.state.lock().unwrap();
            state.demand = demand.clone();
            if latest.is_none() {
                state.snapshot = None;
            }
        }
        if session
            .as_mut()
            .is_some_and(|child| !matches!(child.child.try_wait(), Ok(None)))
        {
            latest = None;
            delivered = None;
            retry = Instant::now() + backoff;
            backoff = (backoff * 2).min(Duration::from_secs(30));
            if retire(&mut session, &service).is_err() {
                retiring = true;
                drop(service);
                thread::sleep(POLL);
                continue;
            }
            let mut state = service.state.lock().unwrap();
            state.phase = if demand.active() {
                "restarting"
            } else {
                "inactive"
            };
            state.failures += 1;
            state.last_error = Some("source_disconnected");
        }
        if !demand.active() {
            latest = None;
            delivered = None;
            // The existing bounded weather cache/retry state lives in this
            // process. Stop its interests once, then keep memory idle until
            // UC exits; no watcher or forecast polling remains active.
            if let Some(child) = session
                .as_mut()
                .filter(|child| child.demand.as_ref().is_none_or(Demand::active))
            {
                sequence = sequence
                    .checked_add(1)
                    .filter(|value| *value <= MAX_SAFE)
                    .ok_or("Source sequence limit reached")?;
                if source_exchange(child, &service, &Demand::default(), sequence).is_err() {
                    if retire(&mut session, &service).is_err() {
                        retiring = true;
                        drop(service);
                        thread::sleep(POLL);
                        continue;
                    }
                    let mut state = service.state.lock().unwrap();
                    state.failures += 1;
                    state.last_error = Some("source_idle_failed");
                }
            }
            let mut state = service.state.lock().unwrap();
            state.phase = if session.is_some() {
                "idle"
            } else {
                "inactive"
            };
            state.snapshot = None;
        } else {
            if session.is_none() && Instant::now() >= retry {
                match Session::spawn(&executable, &home, fake) {
                    Ok(child) => {
                        let mut state = service.state.lock().unwrap();
                        state.pid = Some(child.child.id());
                        state.process_starts += 1;
                        state.phase = "starting";
                        drop(state);
                        session = Some(child);
                        next_source = Instant::now();
                    }
                    Err(_) => {
                        retry = Instant::now() + backoff;
                        backoff = (backoff * 2).min(Duration::from_secs(30));
                        let mut state = service.state.lock().unwrap();
                        state.phase = "unavailable";
                        state.failures += 1;
                        state.last_error = Some("source_start_failed");
                    }
                }
            }
            if let Some(child) = session.as_mut().filter(|_| Instant::now() >= next_source) {
                next_source = Instant::now() + SOURCE_POLL;
                sequence = sequence
                    .checked_add(1)
                    .filter(|s| *s <= MAX_SAFE)
                    .ok_or("Source sequence limit reached")?;
                let result = source_exchange(child, &service, &demand, sequence);
                match result {
                    Ok(snapshot) => {
                        latest = Some(snapshot);
                        backoff = Duration::from_millis(500);
                    }
                    Err(_) => {
                        latest = None;
                        delivered = None;
                        retry = Instant::now() + backoff;
                        backoff = (backoff * 2).min(Duration::from_secs(30));
                        if retire(&mut session, &service).is_err() {
                            retiring = true;
                            drop(service);
                            thread::sleep(POLL);
                            continue;
                        }
                        let mut state = service.state.lock().unwrap();
                        state.pid = None;
                        state.source = None;
                        state.snapshot = None;
                        state.phase = "restarting";
                        state.failures += 1;
                        state.last_error = Some("source_exchange_failed");
                    }
                }
            }
        }
        drop(service);
        thread::sleep(POLL);
    }
    if let Some(service) = owner.upgrade() {
        if let Err(error) = retire(&mut session, &service) {
            *service.retained.lock().unwrap() = session.take();
            return Err(error);
        }
    } else if let Some(mut child) = session {
        child.stop()?;
    }
    if let Some(service) = owner.upgrade() {
        let mut state = service.state.lock().unwrap();
        state.phase = "stopped";
        state.pid = None;
        state.source = None;
        state.snapshot = None;
        state.demand = Demand::default();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            version: 1,
            generation: 7,
            sequence: 9,
            observed_at_ms: 1000,
            expires_at_ms: 4000,
            music: MusicObservation {
                status: MusicStatus::Ok,
                observed_at_ms: Some(1000),
                preferred_app: None,
                data: Some(MusicData {
                    app: "fixture".into(),
                    title: "Track".into(),
                    artist: "Artist".into(),
                    playing: false,
                    position_s: 12.,
                    duration_s: 90.,
                    state_changed_at_ms: 500,
                    track_id: 9,
                    cover: None,
                }),
            },
            weather: WeatherObservation {
                status: WeatherStatus::Inactive,
                observed_at_ms: None,
                place: None,
                data: None,
            },
        }
    }
    #[test]
    fn wire_is_bounded_typed_and_preserves_original_observation_times() {
        let mut s = snapshot();
        s.validate(7, 9, 1000).unwrap();
        assert_eq!(s.music.observed_at_ms, Some(1000));
        assert_eq!(s.music.data.as_ref().unwrap().state_changed_at_ms, 500);
        assert!(s.validate(7, 9, 4000).is_err());
        assert!(s.validate(8, 9, 1000).is_err());
        assert!(s.validate(7, 10, 1000).is_err());
        s.music.data.as_mut().unwrap().position_s = f64::NAN;
        assert!(s.validate(7, 9, 1000).is_err());
        assert!(serde_json::from_value::<Demand>(
            json!({"music":null,"weather":null,"shell":"bad"})
        )
        .is_err());
        assert!(read_frame(&mut std::io::Cursor::new(b"{}")).is_err());
    }
    #[test]
    fn changed_place_never_relabels_a_previous_forecast() {
        let mut s = snapshot();
        let old = Place {
            name: "Old".into(),
            lat: 1.,
            lon: 2.,
            tz: "UTC".into(),
        };
        let new = Place {
            name: "New".into(),
            lat: 3.,
            lon: 4.,
            tz: "UTC".into(),
        };
        s.weather = WeatherObservation {
            status: WeatherStatus::Stale,
            observed_at_ms: Some(500),
            place: Some(old),
            data: Some(WeatherData {
                place: "Old".into(),
                temp_c: 12.,
                feels_c: 10.,
                code: 1,
                is_day: true,
                hours: vec![],
                days: vec![],
                stale: true,
            }),
        };
        let changed = s.for_demand(&Demand {
            music: None,
            weather: Some(new.clone()),
        });
        assert_eq!(changed.weather.place, Some(new));
        assert!(changed.weather.data.is_none());
        assert!(changed.music.data.is_none());
    }
    #[test]
    fn canonical_music_bounds_and_preference_echo_are_enforced() {
        let mut s = snapshot();
        s.music.preferred_app = Some("p".repeat(128));
        let data = s.music.data.as_mut().unwrap();
        data.app = "a".repeat(256);
        data.title = "t".repeat(512);
        data.artist = "r".repeat(512);
        data.position_s = 604_800.;
        data.duration_s = 604_800.;
        s.validate(7, 9, 1000).unwrap();
        let changed = s.clone().for_demand(&Demand {
            music: Some(MusicDemand {
                preferred_app: Some("another".into()),
            }),
            weather: None,
        });
        assert!(changed.music.data.is_none());
        assert_eq!(changed.music.preferred_app.as_deref(), Some("another"));
        s.music.data.as_mut().unwrap().position_s = 604_801.;
        assert!(s.validate(7, 9, 1000).is_err());
    }
}
