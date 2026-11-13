//! One demand-driven hardware collector. Native calls run only on its own
//! thread; readers and worker exchanges never take a lock held by a source.
use super::sync::{MutexExt, RecoverResult};
use super::sensors::{self, NativeSource};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const MAX_AGE_MS: u64 = 3_000;
const LEASE: Duration = Duration::from_millis(MAX_AGE_MS);
const MAX_MEMORY: u64 = 1 << 50;
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Demand {
    pub cpu: bool,
    pub ram: bool,
    pub gpu: bool,
    pub hz: u8,
}
impl Default for Demand {
    fn default() -> Self {
        Self {
            cpu: false,
            ram: false,
            gpu: false,
            hz: 1,
        }
    }
}
impl Demand {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.hz, 1 | 2 | 4) {
            return Err("Hardware rate must be 1, 2 or 4 Hz".into());
        }
        Ok(())
    }
    pub fn active(&self) -> bool {
        self.cpu || self.ram || self.gpu
    }
    fn union(self, other: Self) -> Self {
        Self {
            cpu: self.cpu || other.cpu,
            ram: self.ram || other.ram,
            gpu: self.gpu || other.gpu,
            hz: if other.active() {
                self.hz.max(other.hz)
            } else {
                self.hz
            },
        }
    }
    fn interval(self) -> Duration {
        Duration::from_millis(1000 / u64::from(self.hz))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Gpu {
    pub name: String,
    pub temp_c: Option<f64>,
    pub load_pct: Option<f64>,
    pub clock_mhz: Option<f64>,
    pub mem_used_bytes: Option<u64>,
    pub mem_total_bytes: Option<u64>,
    pub power_w: Option<f64>,
    pub fan_pct: Option<f64>,
}
fn number(value: Option<f64>, min: f64, max: f64) -> bool {
    value.is_none_or(|v| v.is_finite() && (min..=max).contains(&v))
}
fn memory(used: Option<u64>, total: Option<u64>) -> bool {
    match (used, total) {
        (None, None) => true,
        (Some(u), Some(t)) => t > 0 && t <= MAX_MEMORY && u <= t,
        _ => false,
    }
}
impl Gpu {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.chars().count() > 128
            || self.name.chars().any(char::is_control)
            || !number(self.temp_c, -40., 200.)
            || !number(self.load_pct, 0., 100.)
            || !number(self.clock_mhz, 0., 20_000.)
            || !number(self.power_w, 0., 10_000.)
            || !number(self.fan_pct, 0., 100.)
            || !memory(self.mem_used_bytes, self.mem_total_bytes)
        {
            return Err("Invalid shared GPU reading".into());
        }
        Ok(())
    }
    fn clean(mut self) -> Option<Self> {
        if self.name.chars().count() > 128 || self.name.chars().any(char::is_control) {
            self.name.clear();
        }
        for (field, min, max) in [
            (&mut self.temp_c, -40., 200.),
            (&mut self.load_pct, 0., 100.),
            (&mut self.clock_mhz, 0., 20_000.),
            (&mut self.power_w, 0., 10_000.),
            (&mut self.fan_pct, 0., 100.),
        ] {
            if !number(*field, min, max) {
                *field = None;
            }
        }
        if !memory(self.mem_used_bytes, self.mem_total_bytes) {
            self.mem_used_bytes = None;
            self.mem_total_bytes = None;
        }
        if self.temp_c.is_none()
            && self.load_pct.is_none()
            && self.clock_mhz.is_none()
            && self.mem_used_bytes.is_none()
            && self.power_w.is_none()
            && self.fan_pct.is_none()
        {
            None
        } else {
            Some(self)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub generation: u64,
    pub sequence: u64,
    pub observed_at_ms: u64,
    pub expires_at_ms: u64,
    pub cpu_load_pct: Option<f64>,
    pub ram_used_bytes: Option<u64>,
    pub ram_total_bytes: Option<u64>,
    pub gpu: Option<Gpu>,
}
impl Snapshot {
    pub fn validate(&self, now_ms: u64) -> Result<(), String> {
        if self.version != 1
            || self.generation == 0
            || self.generation > MAX_SAFE_INTEGER
            || self.sequence == 0
            || self.sequence > MAX_SAFE_INTEGER
            || self.observed_at_ms == 0
            || self.expires_at_ms > MAX_SAFE_INTEGER
            || self.observed_at_ms > now_ms
            || self.expires_at_ms <= now_ms
            || self.expires_at_ms <= self.observed_at_ms
            || self.expires_at_ms - self.observed_at_ms > MAX_AGE_MS
            || !number(self.cpu_load_pct, 0., 100.)
            || !memory(self.ram_used_bytes, self.ram_total_bytes)
        {
            return Err("Invalid or expired shared hardware observation".into());
        }
        if let Some(gpu) = &self.gpu {
            gpu.validate()?;
        }
        Ok(())
    }
}

/// Bound to one host channel. Invalid/replayed input never renews its timestamp
/// or sequence; a new worker/channel must construct a new receiver.
#[derive(Default)]
pub struct Receiver {
    latest: Option<Snapshot>,
}
impl Receiver {
    pub fn accept(&mut self, snapshot: Snapshot, now_ms: u64) -> Result<(), String> {
        snapshot.validate(now_ms)?;
        if self.latest.as_ref().is_some_and(|old| {
            old.generation != snapshot.generation
                || snapshot.sequence <= old.sequence
                || snapshot.observed_at_ms < old.observed_at_ms
        }) {
            return Err(
                "Shared hardware observation is from another channel or is not newer".into(),
            );
        }
        self.latest = Some(snapshot);
        Ok(())
    }
    pub fn current(&self, now_ms: u64) -> Option<Snapshot> {
        self.latest
            .as_ref()
            .filter(|s| s.validate(now_ms).is_ok())
            .cloned()
    }
    pub fn state(&self, now_ms: u64) -> Value {
        let current = self.current(now_ms);
        let gpu_fields = current
            .as_ref()
            .and_then(|s| s.gpu.as_ref())
            .map(|g| {
                [
                    (g.temp_c.is_some(), 1u16),
                    (g.load_pct.is_some(), 2),
                    (g.clock_mhz.is_some(), 4),
                    (g.mem_used_bytes.is_some(), 8),
                    (g.mem_total_bytes.is_some(), 16),
                    (g.power_w.is_some(), 32),
                    (g.fan_pct.is_some(), 64),
                    (!g.name.is_empty(), 128),
                ]
                .into_iter()
                .filter_map(|(present, bit)| present.then_some(bit))
                .fold(0u16, |mask, bit| mask | bit)
            })
            .unwrap_or(0);
        json!({"fresh":current.is_some(),"accepted_sequence":self.latest.as_ref().map(|s|s.sequence).unwrap_or(0),
            "expires_at_ms":self.latest.as_ref().map(|s|s.expires_at_ms),
            "cpu_present":current.as_ref().is_some_and(|s|s.cpu_load_pct.is_some()),
            "ram_present":current.as_ref().is_some_and(|s|s.ram_used_bytes.is_some()),"gpu_fields":gpu_fields})
    }
}

/// Raw source seam: no timers, processes, device writes or global singleton.
/// A source is owned by exactly one collector thread.
pub trait Source: Send {
    fn cpu_times(&mut self) -> Option<(u64, u64)>;
    fn memory_bytes(&mut self) -> Option<(u64, u64)>;
    fn gpu(&mut self, now: Duration) -> Option<Gpu>;
    fn release_gpu(&mut self) {}
}
impl Source for NativeSource {
    fn cpu_times(&mut self) -> Option<(u64, u64)> {
        sensors::Source::cpu_times(self)
    }
    fn memory_bytes(&mut self) -> Option<(u64, u64)> {
        sensors::Source::memory_bytes(self)
    }
    fn gpu(&mut self, now: Duration) -> Option<Gpu> {
        self.gpu_details(now).map(|g| Gpu {
            name: g.name,
            temp_c: g.temp_c,
            load_pct: g.load_pct,
            clock_mhz: g.clock_mhz,
            mem_used_bytes: g.mem_used_bytes,
            mem_total_bytes: g.mem_total_bytes,
            power_w: g.power_w,
            fan_pct: g.fan_pct,
        })
    }
    fn release_gpu(&mut self) {
        sensors::Source::release_gpu(self)
    }
}
#[derive(Default)]
struct FakeSource {
    cpu_ticks: u64,
}
impl Source for FakeSource {
    fn cpu_times(&mut self) -> Option<(u64, u64)> {
        self.cpu_ticks += 1;
        Some((self.cpu_ticks * 25, self.cpu_ticks * 100))
    }
    fn memory_bytes(&mut self) -> Option<(u64, u64)> {
        Some((8 << 30, 16 << 30))
    }
    fn gpu(&mut self, _: Duration) -> Option<Gpu> {
        Some(Gpu {
            name: "Simulated GPU".into(),
            temp_c: Some(48.),
            load_pct: Some(37.),
            clock_mhz: Some(1500.),
            mem_used_bytes: Some(4 << 30),
            mem_total_bytes: Some(8 << 30),
            power_w: Some(125.),
            fan_pct: Some(40.),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct Counts {
    cpu: u64,
    ram: u64,
    gpu: u64,
}
#[derive(Clone, Copy, Debug, Default)]
struct Diagnostics {
    reads: Counts,
    samples: u64,
    failures: u64,
    gpu_active: bool,
}
struct Collector<S: Source> {
    source: S,
    demand: Demand,
    previous_cpu: Option<(u64, u64)>,
    last_at: Option<Duration>,
    sequence: u64,
    generation: u64,
    diagnostics: Diagnostics,
}
impl<S: Source> Collector<S> {
    fn new(source: S, generation: u64) -> Self {
        Self {
            source,
            demand: Demand::default(),
            previous_cpu: None,
            last_at: None,
            sequence: 0,
            generation,
            diagnostics: Diagnostics::default(),
        }
    }
    fn demand(&mut self, new: Demand) {
        if !new.cpu {
            self.previous_cpu = None;
        }
        if self.demand.gpu && !new.gpu {
            self.source.release_gpu();
            self.diagnostics.gpu_active = false;
        }
        if !new.active() || !self.demand.active() {
            self.last_at = None;
        }
        self.demand = new;
    }
    fn due_in(&self, now: Duration) -> Option<Duration> {
        if !self.demand.active() {
            None
        } else {
            Some(self.last_at.map_or(Duration::ZERO, |at| {
                self.demand
                    .interval()
                    .saturating_sub(now.saturating_sub(at))
            }))
        }
    }
    fn sample(&mut self, now: Duration, utc_ms: u64, failed: bool) -> Option<Snapshot> {
        if self.due_in(now)? != Duration::ZERO {
            return None;
        }
        self.last_at = Some(now);
        self.sequence = self.sequence.checked_add(1)?;
        self.diagnostics.samples += 1;
        let mut s = Snapshot {
            version: 1,
            generation: self.generation,
            sequence: self.sequence,
            observed_at_ms: utc_ms,
            expires_at_ms: utc_ms.checked_add(MAX_AGE_MS)?,
            cpu_load_pct: None,
            ram_used_bytes: None,
            ram_total_bytes: None,
            gpu: None,
        };
        if self.demand.cpu {
            self.diagnostics.reads.cpu += 1;
            let current = if failed {
                None
            } else {
                self.source.cpu_times()
            }
            .filter(|(idle, total)| *total > 0 && idle <= total);
            s.cpu_load_pct = match (self.previous_cpu, current) {
                (Some((i0, t0)), Some((i, t))) if i >= i0 && t > t0 => {
                    let (idle, total) = (i - i0, t - t0);
                    (idle <= total).then(|| 100. * (1. - idle as f64 / total as f64))
                }
                _ => None,
            };
            if current.is_none() {
                self.diagnostics.failures += 1;
            }
            self.previous_cpu = current;
        }
        if self.demand.ram {
            self.diagnostics.reads.ram += 1;
            match if failed {
                None
            } else {
                self.source.memory_bytes()
            } {
                Some((used, total)) if memory(Some(used), Some(total)) => {
                    s.ram_used_bytes = Some(used);
                    s.ram_total_bytes = Some(total);
                }
                _ => self.diagnostics.failures += 1,
            }
        }
        if self.demand.gpu {
            self.diagnostics.reads.gpu += 1;
            self.diagnostics.gpu_active = true;
            s.gpu = if failed {
                None
            } else {
                self.source.gpu(now).and_then(Gpu::clean)
            };
            if s.gpu.is_none() {
                self.diagnostics.failures += 1;
            }
        }
        Some(s)
    }
    fn stop(&mut self) {
        self.demand(Demand::default());
    }
}
impl Source for Box<dyn Source> {
    fn cpu_times(&mut self) -> Option<(u64, u64)> {
        (**self).cpu_times()
    }
    fn memory_bytes(&mut self) -> Option<(u64, u64)> {
        (**self).memory_bytes()
    }
    fn gpu(&mut self, now: Duration) -> Option<Gpu> {
        (**self).gpu(now)
    }
    fn release_gpu(&mut self) {
        (**self).release_gpu()
    }
}

#[derive(Clone, Copy)]
struct Interest {
    demand: Demand,
    until: Instant,
}
struct State {
    interests: [Option<Interest>; 2],
    revision: u64,
    stopped: bool,
    test_failure: bool,
    snapshot: Option<Snapshot>,
    diagnostics: Diagnostics,
    generation: u64,
}
impl State {
    fn union(&self, now: Instant) -> Demand {
        self.interests
            .iter()
            .flatten()
            .filter(|i| i.until > now)
            .fold(Demand::default(), |d, i| d.union(i.demand))
    }
    fn next_expiry(&self, now: Instant) -> Option<Duration> {
        self.interests
            .iter()
            .flatten()
            .filter(|i| i.until > now)
            .map(|i| i.until.saturating_duration_since(now))
            .min()
    }
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}
pub struct Service {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
    fake: bool,
}
fn consumer_index(consumer: &str) -> Result<usize, String> {
    match consumer {
        "dock" => Ok(0),
        "cooler" => Ok(1),
        _ => Err("Unknown shared hardware consumer".into()),
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
fn generation() -> u64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = now_ms().max(1);
    let mut previous = LAST.load(Ordering::SeqCst);
    loop {
        let next = now.max(previous.saturating_add(1));
        match LAST.compare_exchange(previous, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return next,
            Err(actual) => previous = actual,
        }
    }
}
impl Service {
    pub fn new(fake: bool) -> Arc<Self> {
        if fake {
            Self::with_source(FakeSource::default(), true)
        } else {
            Self::with_source(NativeSource::default(), false)
        }
    }
    pub fn with_source(source: impl Source + 'static, fake: bool) -> Arc<Self> {
        let generation = generation();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                interests: [None, None],
                revision: 0,
                stopped: false,
                test_failure: false,
                snapshot: None,
                diagnostics: Diagnostics::default(),
                generation,
            }),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        let thread = thread::spawn(move || run(worker, Box::new(source), generation));
        Arc::new(Self {
            shared,
            thread: Mutex::new(Some(thread)),
            fake,
        })
    }
    pub fn update_demand(&self, consumer: &str, demand: Demand) -> Result<(), String> {
        demand.validate()?;
        let index = consumer_index(consumer)?;
        let now = Instant::now();
        let mut state = self.shared.state.lock_or_recover();
        if state.stopped {
            return Err("Shared hardware is stopped".into());
        }
        let before = state.union(now);
        state.interests[index] = demand.active().then_some(Interest {
            demand,
            until: now + LEASE,
        });
        if before != state.union(now) {
            state.snapshot = None;
            state.revision = state.revision.wrapping_add(1);
        }
        drop(state);
        self.shared.wake.notify_one();
        Ok(())
    }
    pub fn remove_demand(&self, consumer: &str) {
        let _ = self.update_demand(consumer, Demand::default());
    }
    pub fn snapshot(&self) -> Option<Snapshot> {
        let state = self.shared.state.lock_or_recover();
        if state.stopped || !state.union(Instant::now()).active() {
            return None;
        }
        state
            .snapshot
            .as_ref()
            .filter(|s| s.validate(now_ms()).is_ok())
            .cloned()
    }
    pub fn status(&self) -> Value {
        let state = self.shared.state.lock_or_recover();
        let now = Instant::now();
        let union = state.union(now);
        let demand = |index: usize| {
            state.interests[index]
                .filter(|i| i.until > now)
                .map(|i| i.demand)
        };
        json!({"shared":true,"fake":self.fake,"active":!state.stopped&&union.active(),"stopped":state.stopped,
            "hz":if union.active(){union.hz}else{0},"demands":{"dock":demand(0),"cooler":demand(1)},
            "reads":state.diagnostics.reads,"samples":state.diagnostics.samples,"sourceFailures":state.diagnostics.failures,
            "gpuActive":state.diagnostics.gpu_active,"generation":state.generation,
            "sequence":state.snapshot.as_ref().map(|s|s.sequence).unwrap_or(0),
            "observedAtMs":state.snapshot.as_ref().map(|s|s.observed_at_ms),"expiresAtMs":state.snapshot.as_ref().map(|s|s.expires_at_ms),
            "testFailure":state.test_failure})
    }
    pub fn set_test_failure(&self, enabled: bool) -> Result<(), String> {
        if !cfg!(debug_assertions) || !self.fake {
            return Err("Hardware failure injection requires a debug fake source".into());
        }
        let mut state = self.shared.state.lock_or_recover();
        if state.stopped {
            return Err("Shared hardware is stopped".into());
        }
        if state.test_failure != enabled {
            state.test_failure = enabled;
            state.revision = state.revision.wrapping_add(1);
            state.snapshot = None;
        }
        drop(state);
        self.shared.wake.notify_one();
        Ok(())
    }
    pub fn shutdown(&self) {
        {
            let mut state = self.shared.state.lock_or_recover();
            state.stopped = true;
            state.interests = [None, None];
            state.snapshot = None;
            state.revision = state.revision.wrapping_add(1);
        }
        self.shared.wake.notify_one();
        if let Some(thread) = self.thread.lock_or_recover().take() {
            // No native source can hold UI/status locks. Do not force-stop a DLL
            // call or wait without a bound when the driver itself is stuck.
            let until = Instant::now() + Duration::from_millis(250);
            while !thread.is_finished() && Instant::now() < until {
                thread::sleep(Duration::from_millis(1));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn run(shared: Arc<Shared>, source: Box<dyn Source>, generation: u64) {
    let started = Instant::now();
    let mut collector = Collector::new(source, generation);
    let mut last_failure = false;
    loop {
        let (demand, revision, failed) = {
            let s = shared.state.lock_or_recover();
            if s.stopped {
                drop(s);
                collector.stop();
                break;
            }
            (s.union(Instant::now()), s.revision, s.test_failure)
        };
        collector.demand(demand);
        if failed != last_failure {
            collector.last_at = None;
            last_failure = failed;
        }
        // A crash inside one sensor read loses that sample only; the collector keeps running.
        let sampled = super::sync::guarded("hardware sample", || {
            collector.sample(started.elapsed(), now_ms(), failed)
        })
        .flatten();
        let mut state = shared.state.lock_or_recover();
        if state.stopped {
            drop(state);
            collector.stop();
            break;
        }
        state.diagnostics = collector.diagnostics;
        // A demand change during a slow call invalidates the result rather than
        // publishing values for an old lease or renewing an old observation.
        let current = state.union(Instant::now());
        if current != demand || state.revision != revision {
            continue;
        }
        if !demand.active() {
            state.snapshot = None;
        } else if let Some(s) = sampled {
            state.snapshot = Some(s);
        }
        let due = collector.due_in(started.elapsed());
        let expiry = state.next_expiry(Instant::now());
        let wait = match (due, expiry) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        match wait {
            Some(duration) => {
                let _ = shared
                    .wake
                    .wait_timeout(state, duration.max(Duration::from_millis(1)))
                    .recover();
            }
            None => {
                drop(shared.wake.wait(state).recover());
            }
        }
    }
    shared.state.lock_or_recover().diagnostics = collector.diagnostics;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::atomic::AtomicUsize};
    #[derive(Default)]
    struct Calls {
        cpu: AtomicUsize,
        ram: AtomicUsize,
        gpu: AtomicUsize,
        release: AtomicUsize,
    }
    struct Fake {
        calls: Arc<Calls>,
        cpu: VecDeque<Option<(u64, u64)>>,
        fail: bool,
    }
    impl Source for Fake {
        fn cpu_times(&mut self) -> Option<(u64, u64)> {
            self.calls.cpu.fetch_add(1, Ordering::SeqCst);
            self.cpu.pop_front().flatten()
        }
        fn memory_bytes(&mut self) -> Option<(u64, u64)> {
            self.calls.ram.fetch_add(1, Ordering::SeqCst);
            (!self.fail).then_some((3, 4))
        }
        fn gpu(&mut self, _: Duration) -> Option<Gpu> {
            self.calls.gpu.fetch_add(1, Ordering::SeqCst);
            (!self.fail).then(|| Gpu {
                name: "Fixture GPU".into(),
                load_pct: Some(41.),
                ..Default::default()
            })
        }
        fn release_gpu(&mut self) {
            self.calls.release.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn fake() -> (Arc<Calls>, Fake) {
        let calls = Arc::new(Calls::default());
        (
            calls.clone(),
            Fake {
                calls,
                cpu: VecDeque::from([
                    Some((10, 100)),
                    Some((30, 200)),
                    None,
                    Some((40, 300)),
                    Some((60, 400)),
                ]),
                fail: false,
            },
        )
    }
    fn demand(cpu: bool, ram: bool, gpu: bool, hz: u8) -> Demand {
        Demand { cpu, ram, gpu, hz }
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            version: 1,
            generation: 9,
            sequence: 1,
            observed_at_ms: 100,
            expires_at_ms: 3100,
            cpu_load_pct: Some(50.),
            ram_used_bytes: Some(1),
            ram_total_bytes: Some(2),
            gpu: None,
        }
    }
    #[test]
    fn demand_union_and_rates_validate() {
        assert_eq!(
            demand(true, false, false, 1).union(demand(false, true, true, 4)),
            demand(true, true, true, 4)
        );
        for hz in [0, 3, 5, 255] {
            assert!(demand(true, false, false, hz).validate().is_err());
        }
        assert!(serde_json::from_str::<Demand>(
            r#"{"cpu":true,"ram":false,"gpu":false,"hz":1,"other":1}"#
        )
        .is_err());
    }
    #[test]
    fn one_source_no_interest_no_reads_and_rate_is_not_multiplied_by_consumers() {
        let (calls, source) = fake();
        let mut c = Collector::new(source, 1);
        assert!(c.sample(Duration::ZERO, 100, false).is_none());
        assert_eq!(calls.cpu.load(Ordering::SeqCst), 0);
        c.demand(demand(true, true, true, 4));
        let first = c.sample(Duration::ZERO, 100, false).unwrap();
        assert!(first.cpu_load_pct.is_none());
        assert_eq!(first.ram_used_bytes, Some(3));
        assert!(c.sample(Duration::from_millis(249), 349, false).is_none());
        assert_eq!(
            c.sample(Duration::from_millis(250), 350, false)
                .unwrap()
                .cpu_load_pct,
            Some(80.)
        );
        assert_eq!(calls.cpu.load(Ordering::SeqCst), 2);
        assert_eq!(calls.ram.load(Ordering::SeqCst), 2);
        assert_eq!(calls.gpu.load(Ordering::SeqCst), 2);
        c.demand(demand(true, false, false, 1));
        assert_eq!(calls.release.load(Ordering::SeqCst), 1);
        assert!(c.sample(Duration::from_millis(500), 600, false).is_none());
        c.stop();
        assert!(c.sample(Duration::from_secs(10), 10100, false).is_none());
        assert_eq!(calls.ram.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn failed_fields_are_unknown_without_reusing_old_value_or_cpu_delta() {
        let (_, source) = fake();
        let mut c = Collector::new(source, 1);
        c.demand(demand(true, true, true, 1));
        c.sample(Duration::ZERO, 100, false).unwrap();
        let good = c.sample(Duration::from_secs(1), 1100, false).unwrap();
        assert_eq!(good.cpu_load_pct, Some(80.));
        let bad = c.sample(Duration::from_secs(2), 2100, true).unwrap();
        assert!(bad.cpu_load_pct.is_none() && bad.gpu.is_none() && bad.ram_used_bytes.is_none());
        assert!(bad.sequence > good.sequence);
        assert_eq!(good.observed_at_ms, 1100);
        assert_eq!(bad.observed_at_ms, 2100);
        assert!(c
            .sample(Duration::from_secs(3), 3100, false)
            .unwrap()
            .cpu_load_pct
            .is_none());
    }
    #[test]
    fn receiver_rejects_future_expired_duplicate_old_and_other_generation() {
        let mut receiver = Receiver::default();
        let first = snapshot();
        receiver.accept(first.clone(), 100).unwrap();
        assert!(receiver.accept(first.clone(), 101).is_err());
        assert_eq!(receiver.current(200).unwrap().observed_at_ms, 100);
        assert_eq!(receiver.state(200)["expires_at_ms"], 3100);
        let mut next = first.clone();
        next.sequence = 2;
        next.observed_at_ms = 201;
        assert!(receiver.accept(next.clone(), 200).is_err());
        next.observed_at_ms = 100;
        next.generation = 10;
        assert!(receiver.accept(next, 200).is_err());
        assert!(receiver.current(3100).is_none());
        assert!(receiver.accept(first, 3100).is_err());
        assert_eq!(receiver.state(3100)["expires_at_ms"], 3100);
    }
    #[test]
    fn malformed_numbers_names_memory_and_unknown_fields_rejected() {
        let mut s = snapshot();
        s.cpu_load_pct = Some(f64::NAN);
        assert!(s.validate(100).is_err());
        s.cpu_load_pct = None;
        s.ram_used_bytes = Some(3);
        assert!(s.validate(100).is_err());
        s.ram_used_bytes = None;
        assert!(s.validate(100).is_err());
        s.ram_total_bytes = None;
        s.expires_at_ms = 3101;
        assert!(s.validate(100).is_err());
        let gpu = Gpu {
            name: "Bad\nlabel".into(),
            ..Default::default()
        };
        assert!(gpu.validate().is_err());
        let mut value = serde_json::to_value(snapshot()).unwrap();
        value["extra"] = json!(1);
        assert!(serde_json::from_value::<Snapshot>(value).is_err());
    }
    #[test]
    fn protocol_boundaries_preserve_full_gpu_details_and_reject_outside_values() {
        let gpu = Gpu {
            name: "G".repeat(128),
            temp_c: Some(-40.),
            load_pct: Some(100.),
            clock_mhz: Some(20_000.),
            mem_used_bytes: Some(MAX_MEMORY),
            mem_total_bytes: Some(MAX_MEMORY),
            power_w: Some(10_000.),
            fan_pct: Some(100.),
        };
        let mut s = snapshot();
        s.gpu = Some(gpu.clone());
        s.cpu_load_pct = Some(0.);
        let wire = serde_json::to_value(&s).unwrap();
        assert_eq!(serde_json::from_value::<Snapshot>(wire).unwrap(), s);
        assert!(s.validate(100).is_ok());
        let mut receiver = Receiver::default();
        receiver.accept(s.clone(), 100).unwrap();
        assert_eq!(receiver.state(100)["gpu_fields"], 255);
        assert_eq!(receiver.state(3100)["gpu_fields"], 0);
        for invalid in [
            Gpu {
                temp_c: Some(-40.1),
                ..gpu.clone()
            },
            Gpu {
                temp_c: Some(200.1),
                ..gpu.clone()
            },
            Gpu {
                clock_mhz: Some(20_000.1),
                ..gpu.clone()
            },
            Gpu {
                power_w: Some(10_000.1),
                ..gpu.clone()
            },
            Gpu {
                fan_pct: Some(100.1),
                ..gpu.clone()
            },
            Gpu {
                mem_total_bytes: Some(MAX_MEMORY + 1),
                ..gpu.clone()
            },
            Gpu {
                mem_used_bytes: None,
                ..gpu.clone()
            },
            Gpu {
                name: "G".repeat(129),
                ..gpu.clone()
            },
        ] {
            assert!(invalid.validate().is_err());
        }
        s.generation = MAX_SAFE_INTEGER + 1;
        assert!(s.validate(100).is_err());
        s.generation = 1;
        s.sequence = MAX_SAFE_INTEGER + 1;
        assert!(s.validate(100).is_err());
        s.sequence = 1;
        s.observed_at_ms = 0;
        assert!(s.validate(100).is_err());
        let partial = Gpu {
            load_pct: Some(45.),
            temp_c: Some(f64::NAN),
            mem_used_bytes: Some(10),
            ..Default::default()
        }
        .clean()
        .unwrap();
        assert_eq!(partial.load_pct, Some(45.));
        assert!(partial.temp_c.is_none() && partial.mem_used_bytes.is_none());
    }
    #[test]
    fn changing_union_cannot_exceed_fastest_requested_sample_rate() {
        let (calls, source) = fake();
        let mut c = Collector::new(source, 1);
        c.demand(demand(true, false, false, 1));
        c.sample(Duration::ZERO, 100, false).unwrap();
        c.demand(demand(true, true, false, 1));
        assert!(c.sample(Duration::from_millis(100), 200, false).is_none());
        assert_eq!(calls.cpu.load(Ordering::SeqCst), 1);
        assert_eq!(calls.ram.load(Ordering::SeqCst), 0);
        c.demand(demand(true, true, true, 4));
        assert!(c.sample(Duration::from_millis(200), 300, false).is_none());
        assert!(c.sample(Duration::from_millis(250), 350, false).is_some());
        assert_eq!(calls.cpu.load(Ordering::SeqCst), 2);
        assert_eq!(calls.ram.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn stalled_source_never_holds_status_demand_or_snapshot_locks() {
        struct Stalled {
            entered: std::sync::mpsc::SyncSender<()>,
            gate: Arc<(Mutex<bool>, Condvar)>,
        }
        impl Source for Stalled {
            fn cpu_times(&mut self) -> Option<(u64, u64)> {
                None
            }
            fn memory_bytes(&mut self) -> Option<(u64, u64)> {
                None
            }
            fn gpu(&mut self, _: Duration) -> Option<Gpu> {
                let _ = self.entered.try_send(());
                let (lock, wake) = &*self.gate;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = wake.wait(open).unwrap();
                }
                Some(Gpu {
                    load_pct: Some(10.),
                    ..Default::default()
                })
            }
        }
        let (entered, received) = std::sync::mpsc::sync_channel(1);
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let s = Service::with_source(
            Stalled {
                entered,
                gate: gate.clone(),
            },
            true,
        );
        s.update_demand("dock", demand(false, false, true, 4))
            .unwrap();
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        let began = Instant::now();
        assert!(s.snapshot().is_none());
        assert_eq!(s.status()["active"], true);
        s.remove_demand("dock");
        assert_eq!(s.status()["active"], false);
        assert!(s.snapshot().is_none());
        let elapsed = began.elapsed();
        // Always release the fake source, including if a timing assertion fails.
        *gate.0.lock().unwrap() = true;
        gate.1.notify_one();
        assert!(elapsed < Duration::from_millis(500));
        s.shutdown();
        assert!(s.snapshot().is_none());
        assert_eq!(s.status()["gpuActive"], false);
    }
    #[test]
    fn stale_consumer_leases_remove_interest_without_refreshing_snapshot() {
        let now = Instant::now();
        let mut s = State {
            interests: [
                Some(Interest {
                    demand: demand(true, false, false, 1),
                    until: now + LEASE,
                }),
                Some(Interest {
                    demand: demand(false, true, true, 4),
                    until: now + Duration::from_secs(1),
                }),
            ],
            revision: 0,
            stopped: false,
            test_failure: false,
            snapshot: Some(snapshot()),
            diagnostics: Diagnostics::default(),
            generation: 1,
        };
        assert_eq!(s.union(now), demand(true, true, true, 4));
        assert_eq!(
            s.union(now + Duration::from_secs(1)),
            demand(true, false, false, 1)
        );
        assert!(!s.union(now + LEASE).active());
        s.interests[0] = None;
        assert!(!s.union(now + Duration::from_secs(1)).active());
    }
    #[test]
    fn service_fake_is_quiet_until_demand_and_failure_recovery_uses_new_sequences() {
        let s = Service::new(true);
        thread::sleep(Duration::from_millis(20));
        assert_eq!(s.status()["samples"], 0);
        assert!(s.snapshot().is_none());
        assert!(s.update_demand("other", Demand::default()).is_err());
        s.update_demand("dock", demand(true, true, true, 4))
            .unwrap();
        let wait = |test: &dyn Fn() -> bool| {
            let until = Instant::now() + Duration::from_secs(2);
            while !test() {
                assert!(Instant::now() < until);
                thread::sleep(Duration::from_millis(2));
            }
        };
        wait(&|| s.snapshot().is_some());
        let good = s.snapshot().unwrap();
        assert_eq!(good.gpu.unwrap().power_w, Some(125.));
        s.set_test_failure(true).unwrap();
        wait(&|| {
            s.snapshot().is_some_and(|v| {
                v.sequence > good.sequence && v.gpu.is_none() && v.ram_total_bytes.is_none()
            })
        });
        let bad = s.snapshot().unwrap();
        s.set_test_failure(false).unwrap();
        wait(&|| {
            s.snapshot()
                .is_some_and(|v| v.sequence > bad.sequence && v.gpu.is_some())
        });
        s.remove_demand("dock");
        assert!(s.snapshot().is_none());
        wait(&|| s.status()["gpuActive"] == false);
        let reads = s.status()["reads"].clone();
        thread::sleep(Duration::from_millis(30));
        assert_eq!(reads, s.status()["reads"]);
        s.shutdown();
        assert!(s.snapshot().is_none());
        assert!(s
            .update_demand("dock", demand(true, false, false, 1))
            .is_err());
    }
}
