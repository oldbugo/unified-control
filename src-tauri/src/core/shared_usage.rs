//! One host-owned account reader and rasterizer. Workers receive only bounded
//! guarded frames; they never activate a second account helper in shared mode.
use super::{
    dock_worker::Controller,
    key_render::{render_face, RenderInputs, RenderedFace, KEY_SIZE, WIDE_SIZE},
    profiles::Face,
    usage::{self, ActiveProvider, Provider, UsageService, UsageSnapshot, UsageStatus},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub const LEASE_MS: u64 = 3000;
pub const MAX_SPECS: usize = 64;
const MAX_PNG_BYTES: usize = 4 * 1024 * 1024;
const SAFE: u64 = (1 << 53) - 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RenderSpec {
    pub provider: Provider,
    pub view: String,
    pub width: u32,
    pub height: u32,
}
impl RenderSpec {
    pub fn from_face(face: &Face, index: usize) -> Option<Self> {
        let provider = match face.kind.as_str() {
            "claude" => Provider::Claude,
            "codex" | "reset" | "trend" => Provider::Codex,
            _ => return None,
        };
        let view = match face.kind.as_str() {
            "reset" => "resets",
            "trend" => "trend",
            _ => face.usage_view.as_deref().unwrap_or("rings"),
        };
        let (width, height) = if index == 13 { WIDE_SIZE } else { KEY_SIZE };
        Some(Self {
            provider,
            view: view.into(),
            width,
            height,
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if !usage::USAGE_VIEWS.contains(&self.view.as_str())
            || self.provider == Provider::Claude && self.view == "tokens"
            || ![(196, 196), (458, 196)].contains(&(self.width, self.height))
        {
            return Err("Invalid shared usage render specification".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Demand {
    pub providers: Vec<ActiveProvider>,
    pub renders: Vec<RenderSpec>,
}
impl Demand {
    pub fn validate(&self) -> Result<(), String> {
        if self.providers.len() > 2 || self.renders.len() > MAX_SPECS {
            return Err("Shared usage demand exceeds its limit".into());
        }
        for (index, p) in self.providers.iter().enumerate() {
            if p.provider == Provider::Claude && p.token_history
                || self.providers[..index]
                    .iter()
                    .any(|old| old.provider == p.provider)
            {
                return Err("Invalid shared usage provider demand".into());
            }
        }
        for (index, spec) in self.renders.iter().enumerate() {
            spec.validate()?;
            if self.renders[..index].contains(spec)
                || !self.providers.iter().any(|p| p.provider == spec.provider)
                || spec.view == "tokens"
                    && !self
                        .providers
                        .iter()
                        .any(|p| p.provider == Provider::Codex && p.token_history)
            {
                return Err("Usage render has no selected provider or is duplicated".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub spec: RenderSpec,
    pub png: Vec<u8>,
    pub snapshot: UsageSnapshot,
    pub valid_until_ms: Option<u64>,
}
impl Frame {
    fn validate(&self, now: u64) -> Result<(), String> {
        self.spec.validate()?;
        if self.snapshot.provider != self.spec.provider
            || self.valid_until_ms != self.snapshot.valid_until_ms(&self.spec.view)
            || !usage::valid_png(&self.png, self.spec.width, self.spec.height)
        {
            return Err("Invalid shared usage frame".into());
        }
        let guarded = usage::guard_delivery(self.snapshot.clone(), now)
            .map_err(|_| "Shared usage image expired")?;
        if guarded != self.snapshot {
            return Err("Shared usage image does not match its guarded observation".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub version: u32,
    pub generation: u64,
    pub sequence: u64,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub reader_code: String,
    pub snapshots: Vec<UsageSnapshot>,
    pub frames: Vec<Frame>,
}
fn reader_message(code: &str) -> Option<&'static str> {
    Some(match code {
        "inactive" => "AI usage reader is inactive.",
        "loading" => "Waiting for the shared account reader.",
        "ready" => {
            "AI usage reader is running; each provider still needs a current verified observation."
        }
        "runtime-missing" => "The bundled usage runtime is missing.",
        "reader-missing" => "The bundled usage reader is missing.",
        "reader-start-failed" => "The usage reader could not start.",
        "reader-inactive" => "The usage reader is inactive.",
        "containment-unavailable" => "Usage process containment unavailable.",
        "invalid-snapshot" => "The usage reader returned an invalid snapshot.",
        "invalid-image" => "The usage reader returned an invalid image.",
        "expired-before-delivery" => "The usage image expired before delivery.",
        "request-rejected" => "Usage reader rejected the request.",
        "reader-timeout" => "Usage reader response timed out.",
        "invalid-configuration" => "Invalid usage configuration.",
        "reader-unavailable" => "The shared account reader is unavailable.",
        _ => return None,
    })
}
impl Batch {
    pub fn validate(&self, now: u64) -> Result<(), String> {
        if self.version != 1
            || self.generation == 0
            || self.generation > SAFE
            || self.sequence == 0
            || self.sequence > SAFE
            || self.created_at_ms == 0
            || self.created_at_ms > now
            || self.expires_at_ms <= now
            || self.expires_at_ms <= self.created_at_ms
            || self.expires_at_ms - self.created_at_ms > LEASE_MS
            || self.expires_at_ms > SAFE
            || reader_message(&self.reader_code).is_none()
            || self.snapshots.len() > 2
            || self.frames.len() > MAX_SPECS
            || self.frames.iter().map(|f| f.png.len()).sum::<usize>() > MAX_PNG_BYTES
        {
            return Err("Invalid or expired shared usage batch".into());
        }
        for (i, s) in self.snapshots.iter().enumerate() {
            if self.snapshots[..i]
                .iter()
                .any(|old| old.provider == s.provider)
                || s.clone().guarded(now) != *s
            {
                return Err("Invalid shared usage provider status".into());
            }
        }
        for (i, frame) in self.frames.iter().enumerate() {
            frame.validate(now)?;
            if self.frames[..i].iter().any(|old| old.spec == frame.spec) {
                return Err("Duplicate shared usage frame".into());
            }
            if !self
                .snapshots
                .iter()
                .any(|s| s.provider == frame.spec.provider && s == &frame.snapshot)
            {
                return Err("Shared usage frame differs from final provider status".into());
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Receiver {
    batch: Option<Batch>,
}
impl Receiver {
    pub fn accept(&mut self, batch: Batch, now: u64) -> Result<(), String> {
        batch.validate(now)?;
        if self.batch.as_ref().is_some_and(|old| {
            old.generation != batch.generation
                || old.sequence >= batch.sequence
                || old.created_at_ms > batch.created_at_ms
        }) {
            return Err("Shared usage batch is from another channel or is not newer".into());
        }
        self.batch = Some(batch);
        Ok(())
    }
    pub fn render(&self, spec: &RenderSpec, now: u64) -> Option<RenderedFace> {
        let b = self.batch.as_ref()?;
        if b.expires_at_ms <= now || b.created_at_ms > now {
            return None;
        }
        let f = b.frames.iter().find(|f| f.spec == *spec)?;
        f.validate(now).ok()?;
        Some(RenderedFace {
            width: spec.width,
            height: spec.height,
            png: f.png.clone(),
            unknown: f.snapshot.status != UsageStatus::Ok,
            valid_until_ms: Some(f.valid_until_ms.unwrap_or(u64::MAX).min(b.expires_at_ms)),
        })
    }
    pub fn status(&self, providers: &[ActiveProvider], now: u64) -> Value {
        let current = self
            .batch
            .as_ref()
            .filter(|b| b.created_at_ms <= now && b.expires_at_ms > now);
        let reader_code = current.map_or(
            if providers.is_empty() {
                "inactive"
            } else {
                "reader-unavailable"
            },
            |b| b.reader_code.as_str(),
        );
        let statuses=[Provider::Claude,Provider::Codex].into_iter().map(|provider|{
            let selected=providers.iter().any(|p|p.provider==provider);
            let s=if selected{current.and_then(|b|b.snapshots.iter().find(|s|s.provider==provider)).cloned()
                .unwrap_or_else(||UsageSnapshot::unknown(provider,UsageStatus::Unavailable)).guarded(now)}else{UsageSnapshot::unknown(provider,UsageStatus::Inactive)};
            let code=match s.status{UsageStatus::Ok=>"verified",UsageStatus::Stale=>"stale",UsageStatus::Loading=>"loading",UsageStatus::Inactive=>"inactive",UsageStatus::Error=>"source-error",UsageStatus::Unavailable=>"unverified-or-unavailable"};
            json!({"provider":provider,"selected":selected,"status":s.status,"code":code,"message":s.status.message()})
        }).collect::<Vec<_>>();
        let expires = self.batch.as_ref().map(|b| {
            b.snapshots
                .iter()
                .filter_map(|s| s.valid_until_ms("tokens"))
                .fold(b.expires_at_ms, u64::min)
        });
        json!({"shared":true,"fresh":current.is_some(),"reader":{"code":reader_code,"message":reader_message(reader_code).unwrap_or("The shared account reader is unavailable.")},
            "providers":statuses,"expires_at_ms":expires,"sequence":self.batch.as_ref().map(|b|b.sequence).unwrap_or(0),"local_reader_starts":0})
    }
    pub fn clear(&mut self) {
        self.batch = None;
    }
}

struct Control {
    stopped: bool,
    paused: bool,
    mode: String,
    status: Value,
}
struct Shared {
    control: Mutex<Control>,
    wake: Condvar,
}
pub struct Service {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
    fake: bool,
}
static GENERATION: AtomicU64 = AtomicU64::new(0);
fn now_ms() -> u64 {
    super::scheduler::now_ms()
}
impl Service {
    pub fn start(
        home: PathBuf,
        resource_dir: PathBuf,
        fake: bool,
        dock: Weak<Controller>,
    ) -> Arc<Self> {
        let shared = Arc::new(Shared {
            control: Mutex::new(Control {
                stopped: false,
                paused: false,
                mode: "ok".into(),
                status: json!({"shared":true,"fake":fake,"readerRunning":false,"readerStarts":0,"activeProviders":0,"renders":0,"batches":0}),
            }),
            wake: Condvar::new(),
        });
        let running = shared.clone();
        let thread = thread::spawn(move || run(running, home, resource_dir, fake, dock));
        Arc::new(Self {
            shared,
            thread: Mutex::new(Some(thread)),
            fake,
        })
    }
    pub fn status(&self) -> Value {
        let c = self.shared.control.lock().unwrap();
        let mut value = c.status.clone();
        value["paused"] = json!(c.paused);
        value["stopped"] = json!(c.stopped);
        value
    }
    pub fn test_control(&self, mode: &str, paused: bool) -> Result<Value, String> {
        if !cfg!(debug_assertions) || !self.fake {
            return Err("Usage test controls require a debug fake instance".into());
        }
        if !["ok", "error", "stale", "logout", "unverified"].contains(&mode) {
            return Err("Invalid synthetic usage mode".into());
        }
        {
            let mut c = self.shared.control.lock().unwrap();
            if c.stopped {
                return Err("Shared usage is stopped".into());
            }
            c.mode = mode.into();
            c.paused = paused;
        }
        self.shared.wake.notify_one();
        Ok(self.status())
    }
    pub fn shutdown(&self) {
        {
            let mut c = self.shared.control.lock().unwrap();
            c.stopped = true;
            c.status["readerRunning"] = json!(false);
            c.status["activeProviders"] = json!(0);
        }
        self.shared.wake.notify_one();
        if let Some(thread) = self.thread.lock().unwrap().take() {
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
fn reader(home: &std::path::Path, resources: &std::path::Path) -> UsageService {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = if cfg!(debug_assertions) {
        root.join("../companions/usage-reader/reader.mjs")
    } else {
        resources.join("usage-reader/reader.mjs")
    };
    let node = resources.join("node/node.exe");
    let node = if node.is_file() || !cfg!(debug_assertions) {
        node
    } else {
        std::env::var_os("PATH")
            .and_then(|p| {
                std::env::split_paths(&p)
                    .map(|d| d.join("node.exe"))
                    .find(|p| p.is_file())
            })
            .unwrap_or_else(|| "node.exe".into())
    };
    UsageService::new(node, script, home)
}
fn synthetic(provider: Provider, mode: &str, at: u64) -> UsageSnapshot {
    let mut s = UsageSnapshot::unknown(provider, UsageStatus::Ok);
    s.observed_at = Some(if mode == "stale" {
        at.saturating_sub(300_001)
    } else {
        at
    });
    s.provenance_verified = mode != "unverified";
    s.five_hour = Some(usage::UsageWindow {
        used_percent: 23.,
        resets_at: Some(at / 1000 + 3600),
    });
    s.weekly = Some(usage::UsageWindow {
        used_percent: 42.,
        resets_at: Some(at / 1000 + 86400),
    });
    if mode == "error" {
        s.status = UsageStatus::Error;
    } else if mode == "logout" {
        s.status = UsageStatus::Unavailable;
    }
    s.guarded(at)
}
fn fake_frame(spec: &RenderSpec, snapshot: UsageSnapshot) -> Result<Frame, String> {
    let known = snapshot.status == UsageStatus::Ok;
    let frame = render_face(
        if known { "text" } else { "reset" },
        if known { "AI fixture" } else { "" },
        spec.width == WIDE_SIZE.0,
        &RenderInputs::default(),
    )?;
    let valid_until_ms = snapshot.valid_until_ms(&spec.view);
    Ok(Frame {
        spec: spec.clone(),
        png: frame.png,
        snapshot,
        valid_until_ms,
    })
}
/// A helper acknowledging activation is not yet a stable source. Keep failed
/// process restarts bounded even when each new process exits just after its ack.
struct ReaderRetry {
    next: Instant,
    delay: Duration,
    live_since: Option<Instant>,
}
impl ReaderRetry {
    fn new(now: Instant) -> Self {
        Self {
            next: now,
            delay: Duration::from_secs(1),
            live_since: None,
        }
    }
    fn ready(&self, now: Instant) -> bool {
        now >= self.next
    }
    fn attempted(&mut self, now: Instant) {
        self.next = now + self.delay;
        self.delay = (self.delay * 2).min(Duration::from_secs(30));
        self.live_since = None;
    }
    fn spawned(&mut self, now: Instant) {
        self.live_since = Some(now);
    }
    fn lost(&mut self, now: Instant) {
        self.live_since = None;
        self.next = self.next.max(now + Duration::from_secs(1));
    }
    fn healthy(&mut self, now: Instant) {
        if self
            .live_since
            .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_secs(30))
        {
            self.delay = Duration::from_secs(1);
        }
    }
}
fn run(shared: Arc<Shared>, home: PathBuf, resources: PathBuf, fake: bool, dock: Weak<Controller>) {
    let source = (!fake).then(|| reader(&home, &resources));
    let mut active = Vec::<ActiveProvider>::new();
    let mut demand = Demand::default();
    let mut last_demand = None::<Instant>;
    let mut batch = None::<Batch>;
    let mut delivered = None::<(u64, u64)>;
    let mut sequence = 0u64;
    let generation = now_ms()
        .max(GENERATION.fetch_add(1, Ordering::SeqCst) + 1)
        .max(1);
    let mut next_render = Instant::now();
    let mut health_at = Instant::now();
    let mut retry = ReaderRetry::new(Instant::now());
    let mut starts = 0u64;
    let mut source_epochs = 0u64;
    let mut renders = 0u64;
    let mut previous_mode = String::new();
    let mut code = "inactive".to_string();
    loop {
        let (paused, mode) = {
            let c = shared.control.lock().unwrap();
            if c.stopped {
                break;
            }
            (c.paused, c.mode.clone())
        };
        let Some(dock) = dock.upgrade() else {
            break;
        };
        if mode != previous_mode {
            next_render = Instant::now();
            previous_mode = mode.clone();
        }
        if !active.is_empty() && source.as_ref().is_some_and(|s| !s.is_running()) {
            batch = None;
            next_render = Instant::now();
        }
        {
            let worker = dock.worker_status()["generation"].as_u64().unwrap_or(0);
            let send = batch
                .as_ref()
                .filter(|b| {
                    !paused && b.expires_at_ms > now_ms() && delivered != Some((worker, b.sequence))
                })
                .cloned();
            let sent = send.as_ref().map(|b| b.sequence);
            if let Ok(reply) = dock.usage_exchange(send) {
                if let Ok(next) = serde_json::from_value::<Demand>(reply["demand"].clone()) {
                    if next.validate().is_ok() {
                        if next != demand {
                            next_render = Instant::now();
                        }
                        demand = next;
                        last_demand = Some(Instant::now());
                        if let Some(sent) = sent {
                            delivered = Some((worker, sent));
                        }
                    }
                }
            }
        }
        if last_demand.is_none_or(|at| at.elapsed() >= Duration::from_millis(LEASE_MS)) {
            demand = Demand::default();
        }
        if demand.providers != active {
            if active.is_empty() && !demand.providers.is_empty() {
                source_epochs += 1;
            }
            active = demand.providers.clone();
            batch = None;
            next_render = Instant::now();
            code = if active.is_empty() {
                "inactive"
            } else {
                "ready"
            }
            .into();
            if let Some(source) = &source {
                let was_running = source.is_running();
                if active.is_empty() || was_running || retry.ready(Instant::now()) {
                    if !was_running && !active.is_empty() {
                        retry.attempted(Instant::now());
                    }
                    match source.set_active_providers(&active) {
                        Ok(()) => {
                            if !was_running && source.is_running() {
                                starts += 1;
                                retry.spawned(Instant::now());
                            }
                            health_at = Instant::now() + Duration::from_secs(30);
                        }
                        Err(error) => {
                            code = error.code().into();
                            retry.lost(Instant::now());
                        }
                    }
                }
            }
        }
        if let Some(source) = &source {
            if !active.is_empty() {
                if source.is_running() {
                    if Instant::now() >= health_at {
                        if !source.check_health() {
                            code = "reader-unavailable".into();
                            batch = None;
                            next_render = Instant::now();
                            retry.lost(Instant::now());
                        } else {
                            retry.healthy(Instant::now());
                        }
                        health_at = Instant::now() + Duration::from_secs(30);
                    }
                } else {
                    if code == "ready" {
                        code = "reader-unavailable".into();
                    }
                    if retry.live_since.is_some() {
                        retry.lost(Instant::now());
                    }
                    if retry.ready(Instant::now()) {
                        retry.attempted(Instant::now());
                        match source.set_active_providers(&active) {
                            Ok(()) => {
                                if source.is_running() {
                                    starts += 1;
                                    retry.spawned(Instant::now());
                                }
                                code = "ready".into();
                                health_at = Instant::now() + Duration::from_secs(30);
                                next_render = Instant::now();
                            }
                            Err(error) => {
                                code = error.code().into();
                                retry.lost(Instant::now());
                            }
                        }
                    }
                }
            }
        }
        if Instant::now() >= next_render {
            next_render = Instant::now() + Duration::from_secs(1);
            let at = now_ms();
            let mut snapshots = Vec::new();
            let mut frames = Vec::new();
            let mut bytes = 0usize;
            for selected in &active {
                let s = if fake {
                    synthetic(selected.provider, &mode, at)
                } else if !source.as_ref().unwrap().is_running() {
                    UsageSnapshot::unknown(selected.provider, UsageStatus::Error)
                } else {
                    match source.as_ref().unwrap().snapshot(selected.provider) {
                        Ok(s) => s,
                        Err(error) => {
                            code = error.code().into();
                            UsageSnapshot::unknown(selected.provider, UsageStatus::Error)
                        }
                    }
                };
                snapshots.push(s);
            }
            for spec in &demand.renders {
                // Preserve the precise activation failure rather than replacing
                // it with a later "reader inactive" render error.
                if !fake && !source.as_ref().unwrap().is_running() {
                    break;
                }
                let frame = if fake {
                    fake_frame(spec, synthetic(spec.provider, &mode, at))
                } else {
                    source
                        .as_ref()
                        .unwrap()
                        .render(spec.provider, &spec.view, spec.width, spec.height)
                        .map(|r| Frame {
                            spec: spec.clone(),
                            valid_until_ms: r.snapshot.valid_until_ms(&spec.view),
                            snapshot: r.snapshot,
                            png: r.png,
                        })
                        .map_err(|e| {
                            code = e.code().into();
                            "Shared usage rendering failed".to_string()
                        })
                };
                match frame {
                    Ok(frame) => {
                        bytes = bytes.saturating_add(frame.png.len());
                        if bytes > MAX_PNG_BYTES {
                            break;
                        }
                        renders += 1;
                        frames.push(frame);
                    }
                    Err(_) => {
                        frames.clear();
                        snapshots = active
                            .iter()
                            .map(|p| UsageSnapshot::unknown(p.provider, UsageStatus::Error))
                            .collect();
                        break;
                    }
                }
            }
            // Re-read after the final raster. A logout, account change or new
            // observation mid-batch cannot leave older A/B pixels beside a new
            // provider status. Dropping that provider until the next cycle is
            // conservative and leaves private identity entirely in Node.
            for selected in &active {
                let current = if fake {
                    synthetic(selected.provider, &mode, at)
                } else if !source.as_ref().unwrap().is_running() {
                    UsageSnapshot::unknown(selected.provider, UsageStatus::Error)
                } else {
                    match source.as_ref().unwrap().snapshot(selected.provider) {
                        Ok(s) => s,
                        Err(error) => {
                            code = error.code().into();
                            UsageSnapshot::unknown(selected.provider, UsageStatus::Error)
                        }
                    }
                };
                let mismatch = frames
                    .iter()
                    .any(|f| f.spec.provider == selected.provider && f.snapshot != current);
                if mismatch {
                    frames.retain(|f| f.spec.provider != selected.provider);
                }
                let status = if mismatch && current.status == UsageStatus::Ok {
                    UsageSnapshot::unknown(selected.provider, UsageStatus::Unavailable)
                } else {
                    current
                };
                if let Some(existing) = snapshots
                    .iter_mut()
                    .find(|s| s.provider == selected.provider)
                {
                    *existing = status;
                }
            }
            sequence += 1;
            let candidate = Batch {
                version: 1,
                generation,
                sequence,
                created_at_ms: at,
                expires_at_ms: at.saturating_add(LEASE_MS),
                reader_code: if active.is_empty() {
                    "inactive".into()
                } else {
                    code.clone()
                },
                snapshots,
                frames,
            };
            batch = if candidate.validate(now_ms()).is_ok() {
                Some(candidate)
            } else {
                None
            };
        }
        let reader_running = source.as_ref().is_some_and(|s| s.is_running());
        {
            let mut c = shared.control.lock().unwrap();
            c.status = json!({"shared":true,"fake":fake,"readerRunning":reader_running,
            "readerStarts":starts,"sourceEpochs":source_epochs,"ownerGeneration":generation,"activeProviders":active.len(),"specCount":demand.renders.len(),"renders":renders,"batches":sequence,
            "publishedUntil":batch.as_ref().map(|b|b.expires_at_ms),"mode":if fake{mode.as_str()}else{"account"}});
            if c.stopped {
                break;
            }
            drop(
                shared
                    .wake
                    .wait_timeout(c, Duration::from_millis(200))
                    .unwrap(),
            );
        }
    }
    if let Some(source) = source {
        source.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_activation_ack_does_not_reset_a_crash_loop_backoff() {
        let at = Instant::now();
        let mut retry = ReaderRetry::new(at);
        assert!(retry.ready(at));
        retry.attempted(at);
        retry.spawned(at);
        retry.lost(at + Duration::from_millis(100));
        assert!(!retry.ready(at + Duration::from_secs(1)));
        assert!(retry.ready(at + Duration::from_millis(1100)));
        let second = at + Duration::from_millis(1100);
        retry.attempted(second);
        retry.spawned(second);
        assert_eq!(retry.delay, Duration::from_secs(4));
        retry.healthy(second + Duration::from_secs(29));
        assert_eq!(retry.delay, Duration::from_secs(4));
        retry.healthy(second + Duration::from_secs(30));
        assert_eq!(retry.delay, Duration::from_secs(1));
        let mut now = second + Duration::from_secs(31);
        for _ in 0..10 {
            retry.attempted(now);
            retry.spawned(now);
            retry.lost(now);
            now = retry.next;
        }
        assert_eq!(retry.delay, Duration::from_secs(30));
        assert!(!retry.ready(now - Duration::from_millis(1)));
    }
    fn at() -> u64 {
        now_ms()
    }
    fn spec() -> RenderSpec {
        RenderSpec {
            provider: Provider::Codex,
            view: "rings".into(),
            width: 196,
            height: 196,
        }
    }
    fn batch() -> Batch {
        let now = at();
        let f = fake_frame(&spec(), synthetic(Provider::Codex, "ok", now)).unwrap();
        Batch {
            version: 1,
            generation: 1,
            sequence: 1,
            created_at_ms: now,
            expires_at_ms: now + LEASE_MS,
            reader_code: "ready".into(),
            snapshots: vec![f.snapshot.clone()],
            frames: vec![f],
        }
    }
    #[test]
    fn guarded_frames_expire_at_original_transport_and_source_deadlines() {
        let b = batch();
        let mut r = Receiver::default();
        let at = b.created_at_ms;
        r.accept(b.clone(), at).unwrap();
        let frame = r.render(&spec(), at).unwrap();
        assert!(!frame.unknown);
        assert_eq!(frame.valid_until_ms, Some(b.expires_at_ms));
        assert!(r.render(&spec(), b.expires_at_ms).is_none());
        assert!(r.accept(b.clone(), at).is_err());
        assert_eq!(
            r.status(
                &[ActiveProvider {
                    provider: Provider::Codex,
                    token_history: false
                }],
                b.expires_at_ms
            )["reader"]["code"],
            "reader-unavailable"
        );
        let mut next = b.clone();
        next.sequence = 2;
        next.generation = 2;
        assert!(r.accept(next, at).is_err());
        assert_eq!(
            r.status(&[], b.expires_at_ms)["expires_at_ms"],
            b.expires_at_ms
        );
    }
    #[test]
    fn auth_failure_stale_and_unverified_never_publish_current_numbers() {
        for mode in ["error", "stale", "logout", "unverified"] {
            let mut b = batch();
            let at = b.created_at_ms;
            b.sequence = 2;
            b.snapshots = vec![synthetic(Provider::Codex, mode, at)];
            b.frames = vec![fake_frame(&spec(), b.snapshots[0].clone()).unwrap()];
            let mut r = Receiver::default();
            r.accept(b, at).unwrap();
            assert!(r.render(&spec(), at).unwrap().unknown);
            let p = &r.status(
                &[ActiveProvider {
                    provider: Provider::Codex,
                    token_history: false,
                }],
                at,
            )["providers"][1];
            assert_ne!(p["code"], "verified");
        }
    }
    #[test]
    fn frames_reject_corruption_bad_dimensions_future_reset_and_oversize_demand() {
        let mut b = batch();
        b.frames[0].png.truncate(33);
        assert!(b.validate(at()).is_err());
        b = batch();
        b.frames[0].spec.width = 200;
        assert!(b.validate(at()).is_err());
        b = batch();
        let now = b.created_at_ms;
        b.created_at_ms = now + 1;
        assert!(b.validate(now).is_err());
        b = batch();
        b.frames[0].snapshot.five_hour.as_mut().unwrap().resets_at = Some(1);
        assert!(b.validate(at()).is_err());
        assert!(Demand {
            providers: vec![],
            renders: vec![spec()]
        }
        .validate()
        .is_err());
        let mut d = Demand {
            providers: vec![ActiveProvider {
                provider: Provider::Codex,
                token_history: false,
            }],
            renders: vec![spec(), spec()],
        };
        assert!(d.validate().is_err());
        d.renders = vec![RenderSpec {
            view: "tokens".into(),
            ..spec()
        }];
        assert!(d.validate().is_err());
    }
    #[test]
    fn earlier_numeric_frame_cannot_survive_later_logout_or_new_observation() {
        let mut b = batch();
        let now = b.created_at_ms;
        b.snapshots[0] = UsageSnapshot::unknown(Provider::Codex, UsageStatus::Unavailable);
        assert!(b.validate(now).is_err());
        b = batch();
        let now = b.created_at_ms;
        b.snapshots[0].observed_at = Some(now + 1);
        assert!(b.validate(now + 1).is_err());
    }
}
