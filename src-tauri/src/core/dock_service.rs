//! Explicitly enabled dock controller. One owner thread holds the transport and
//! native ownership mutex; a separate serial executor finishes active actions.
//! Construction starts disabled and performs no profile, process or device I/O.
use super::{
    dock_runtime::{elapsed_us, PageRuntime, PendingAction, RenderTimings},
    key_render::{render_face, RenderInputs, RenderedFace},
    profiles::{Face, ProfileState, ProfileStore},
    sync::{MutexExt, RwLockExt},
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex, RwLock,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use ulanzi_dock::{
    dock::{Dock, Transport},
    fake::FakeDock,
    protocol::{ButtonAction, Event, Packet, SmallWindowData, SmallWindowMode},
};

pub trait DockHooks: Send + Sync {
    fn inputs(&self) -> RenderInputs;
    fn render(
        &self,
        face: &Face,
        index: usize,
        input: &RenderInputs,
    ) -> Result<RenderedFace, String>;
    fn active_profile(&self, state: &ProfileState, enabled: bool);
    fn execute(&self, action: &PendingAction) -> Result<(), String>;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockStatus {
    pub state: String,
    pub message: String,
    pub enabled: bool,
    pub fake: bool,
    pub current_profile: Option<String>,
    pub current_page: Option<String>,
    pub pages_sent: u64,
    pub frames_sent: u64,
    pub keep_alives_sent: u64,
    pub probe_writes: u64,
    pub actions: u64,
    pub errors: u64,
    pub timing: DockTimings,
}
/// Fixed-size numerical diagnostics only. Receipt starts at the host's completed
/// poll, not at the physical press; preceding events are not a queue-depth claim.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockTimings {
    pub polls: u64,
    pub non_button_events: u64,
    pub last_poll_us: u64,
    pub last_poll_gap_us: u64,
    pub max_poll_gap_us: u64,
    pub navigations_started: u64,
    pub navigations_completed: u64,
    pub navigations_aborted: u64,
    pub last_inputs_us: u64,
    pub last_render: RenderTimings,
    pub last_navigation: Option<NavigationTiming>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationTiming {
    pub receipt_to_delivery_us: u64,
    pub handling_us: u64,
    pub wait_for_render_us: u64,
    pub inputs_us: u64,
    pub render: RenderTimings,
    pub preceding_non_button_events: u64,
    pub poll_gap_before_receipt_us: u64,
}
struct PendingNavigation {
    received: Instant,
    handled: Instant,
    preceding_non_button_events: u64,
    poll_gap_before_receipt_us: u64,
}
struct Shared {
    status: Mutex<DockStatus>,
    enabled: AtomicBool,
    brightness: AtomicU8,
    revision: AtomicU64,
    applied_revision: AtomicU64,
    stop: AtomicBool,
    fake_studio: AtomicBool,
    writing: AtomicBool,
    action_busy: AtomicBool,
    queued_actions: AtomicUsize,
    action_epoch: AtomicU64,
    fake: bool,
    admission: RwLock<()>,
    quiesced: AtomicBool,
}
impl Shared {
    fn status(&self) -> DockStatus {
        self.status.lock_or_recover().clone()
    }
    fn set_state(&self, state: &str, message: &str) {
        let mut s = self.status.lock_or_recover();
        if s.state != state || s.message != message {
            crate::device::log(&format!("dock state={state}: {message}"));
        }
        s.state = state.into();
        s.message = message.into();
        s.enabled = self.enabled.load(Ordering::SeqCst);
    }
    fn studio(&self) -> io::Result<bool> {
        if self.fake {
            Ok(self.fake_studio.load(Ordering::SeqCst))
        } else {
            native::studio_running()
        }
    }
    fn safe_to_act(&self) -> bool {
        if self.stop.load(Ordering::SeqCst)
            || self.quiesced.load(Ordering::SeqCst)
            || !self.enabled.load(Ordering::SeqCst)
        {
            return false;
        }
        // Re-read admission after the potentially slower process snapshot.
        matches!(self.studio(), Ok(false))
            && !self.stop.load(Ordering::SeqCst)
            && self.enabled.load(Ordering::SeqCst)
    }
}
enum Control {
    Wake,
    Button {
        index: u8,
        pressed: bool,
    },
    #[cfg(test)]
    Disconnect,
}
struct Job {
    action: PendingAction,
    index: usize,
    page: String,
    generation: u64,
    epoch: u64,
}
struct Completion {
    index: usize,
    page: String,
    generation: u64,
    failed: bool,
    executed: bool,
}

pub struct Controller {
    shared: Arc<Shared>,
    tx: SyncSender<Control>,
    threads: Mutex<Option<(JoinHandle<()>, JoinHandle<()>)>>,
}
impl Controller {
    pub fn new(profiles: Arc<ProfileStore>, hooks: Arc<dyn DockHooks>, fake: bool) -> Self {
        let shared = Arc::new(Shared {
            status: Mutex::new(DockStatus {
                state: "disabled".into(),
                message: "Dock is disabled".into(),
                enabled: false,
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
            }),
            enabled: AtomicBool::new(false),
            brightness: AtomicU8::new(50),
            revision: AtomicU64::new(0),
            applied_revision: AtomicU64::new(u64::MAX),
            stop: AtomicBool::new(false),
            fake_studio: AtomicBool::new(false),
            writing: AtomicBool::new(false),
            action_busy: AtomicBool::new(false),
            queued_actions: AtomicUsize::new(0),
            action_epoch: AtomicU64::new(0),
            fake,
            admission: RwLock::new(()),
            quiesced: AtomicBool::new(false),
        });
        let (tx, rx) = mpsc::sync_channel(32);
        let (actions, action_rx) = mpsc::sync_channel(16);
        let (completed, complete_rx) = mpsc::sync_channel(16);
        let (s, h) = (shared.clone(), hooks.clone());
        let executor = thread::spawn(move || action_loop(s, h, action_rx, completed));
        let s = shared.clone();
        let worker =
            thread::spawn(move || Worker::new(s, profiles, hooks, actions, complete_rx).run(rx));
        Self {
            shared,
            tx,
            threads: Mutex::new(Some((worker, executor))),
        }
    }
    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        if self.shared.stop.load(Ordering::SeqCst) {
            return Err("Dock controller is stopping".into());
        }
        self.shared.enabled.store(enabled, Ordering::SeqCst);
        if !enabled {
            self.shared.action_epoch.fetch_add(1, Ordering::SeqCst);
        }
        self.shared.status.lock_or_recover().enabled = enabled;
        let _ = self.tx.try_send(Control::Wake);
        Ok(())
    }
    pub fn set_brightness(&self, value: u8) -> Result<(), String> {
        if value > 100 {
            return Err("Brightness must be between 0 and 100".into());
        }
        if self.shared.stop.load(Ordering::SeqCst) {
            return Err("Dock controller is stopping".into());
        }
        self.shared.brightness.store(value, Ordering::SeqCst);
        let _ = self.tx.try_send(Control::Wake);
        Ok(())
    }
    pub fn notify_profiles_changed(&self) {
        self.shared.revision.fetch_add(1, Ordering::SeqCst);
        let _ = self.tx.try_send(Control::Wake);
    }
    /// A worker RPC acknowledges a profile update only after the active runtime
    /// consumes it. Disabled/blocked engines have no admitted input to race.
    pub fn sync_profiles_changed(&self) -> Result<(), String> {
        let revision = self
            .shared
            .revision
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        let _ = self.tx.try_send(Control::Wake);
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if self.shared.applied_revision.load(Ordering::SeqCst) == revision
                || !self.shared.enabled.load(Ordering::SeqCst)
                || matches!(self.status().state.as_str(), "blocked" | "error")
            {
                return Ok(());
            }
            if self.shared.stop.load(Ordering::SeqCst) {
                return Err("Dock worker is stopping".into());
            }
            if Instant::now() >= until {
                return Err("Dock layout is saved but runtime synchronization is pending".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    pub fn status(&self) -> DockStatus {
        self.shared.status()
    }
    pub fn busy(&self) -> bool {
        self.shared.writing.load(Ordering::SeqCst)
            || self.shared.action_busy.load(Ordering::SeqCst)
            || self.shared.queued_actions.load(Ordering::SeqCst) > 0
    }
    /// Close admission under the same lock used by render/transfer and actions.
    /// A busy refusal leaves admission unchanged; no active work is truncated.
    pub fn prepare_exit(&self) -> Result<(), String> {
        let until = Instant::now() + Duration::from_millis(500);
        loop {
            if self.shared.action_busy.load(Ordering::SeqCst)
                || self.shared.queued_actions.load(Ordering::SeqCst) > 0
            {
                return Err("Dock is busy; try again when idle".into());
            }
            if let Some(_admission) = self.shared.admission.try_write_or_recover() {
                if self.shared.action_busy.load(Ordering::SeqCst)
                    || self.shared.queued_actions.load(Ordering::SeqCst) > 0
                {
                    return Err("Dock is busy; try again when idle".into());
                }
                self.shared.quiesced.store(true, Ordering::SeqCst);
                return Ok(());
            }
            if Instant::now() >= until {
                return Err("Dock is busy; try again when idle".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn cancel_exit(&self) {
        self.shared.quiesced.store(false, Ordering::SeqCst);
    }
    pub fn simulate_button(&self, index: usize, pressed: bool) -> Result<(), String> {
        if self.shared.quiesced.load(Ordering::SeqCst) || self.shared.stop.load(Ordering::SeqCst) {
            return Err("Dock action admission is closed".into());
        }
        if !self.shared.fake {
            return Err("Simulated buttons are available only with a fake dock".into());
        }
        if index >= 14 {
            return Err("Display index is invalid".into());
        }
        if self.shared.status().state != "connected" {
            return Err("The fake dock is not connected".into());
        }
        self.tx
            .try_send(Control::Button {
                index: index as u8,
                pressed,
            })
            .map_err(|_| "Dock input queue is full or closed".into())
    }
    pub fn set_fake_studio(&self, running: bool) -> Result<(), String> {
        if !self.shared.fake {
            return Err("Injected Studio state is available only with a fake dock".into());
        }
        self.shared.fake_studio.store(running, Ordering::SeqCst);
        let _ = self.tx.try_send(Control::Wake);
        Ok(())
    }
    /// Stops admission immediately; waits for any active device transfer and
    /// action transaction. It never kills a helper or truncates monitor recovery.
    pub fn shutdown(&self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.enabled.store(false, Ordering::SeqCst);
        let _ = self.tx.try_send(Control::Wake);
        if let Some((worker, executor)) = self.threads.lock_or_recover().take() {
            let _ = worker.join();
            let _ = executor.join();
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn action_loop(
    shared: Arc<Shared>,
    hooks: Arc<dyn DockHooks>,
    rx: Receiver<Job>,
    completed: SyncSender<Completion>,
) {
    while let Ok(job) = rx.recv() {
        let _admission = shared.admission.read_or_recover();
        shared.action_busy.store(true, Ordering::SeqCst);
        shared.queued_actions.fetch_sub(1, Ordering::SeqCst);
        let executed = shared.action_epoch.load(Ordering::SeqCst) == job.epoch
            && shared.safe_to_act()
            && shared.action_epoch.load(Ordering::SeqCst) == job.epoch;
        if executed && cfg!(debug_assertions) && shared.fake {
            let delay = std::env::var("UC_DOCK_TEST_ACTION_DELAY_MS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
                .min(10_000);
            thread::sleep(Duration::from_millis(delay));
        }
        let failed = executed && hooks.execute(&job.action).is_err();
        // Do not log action values or errors supplied by integrations.
        let _ = completed.try_send(Completion {
            index: job.index,
            page: job.page,
            generation: job.generation,
            failed,
            executed,
        });
        shared.action_busy.store(false, Ordering::SeqCst);
    }
}
enum Link {
    Fake(FakeDock),
    #[cfg(test)]
    ProbeFixture {
        dock: FakeDock,
        replies: bool,
    },
    #[cfg(windows)]
    Native(ulanzi_dock::windows::WindowsHid),
}
impl Transport for Link {
    fn write(&mut self, p: &Packet) -> io::Result<()> {
        match self {
            Self::Fake(f) => f.write(p),
            #[cfg(test)]
            Self::ProbeFixture { dock, .. } => dock.write(p),
            #[cfg(windows)]
            Self::Native(n) => n.write(p),
        }
    }
    fn read(&mut self, t: Duration) -> io::Result<Option<Packet>> {
        match self {
            Self::Fake(f) => f.read(t),
            #[cfg(test)]
            Self::ProbeFixture { dock, replies } => {
                if *replies {
                    dock.read(t)
                } else {
                    while dock.read(Duration::ZERO)?.is_some() {}
                    Ok(None)
                }
            }
            #[cfg(windows)]
            Self::Native(n) => n.read(t),
        }
    }
}
struct Worker {
    shared: Arc<Shared>,
    profiles: Arc<ProfileStore>,
    hooks: Arc<dyn DockHooks>,
    actions: SyncSender<Job>,
    completed: Receiver<Completion>,
    runtime: Option<PageRuntime>,
    dock: Option<Dock<Link>>,
    owner: Option<native::Owner>,
    generation: u64,
    revision: u64,
    retry_at: Instant,
    retry_delay: Duration,
    last_activity: Instant,
    next_render: Instant,
    next_probe: Instant,
    last_fake_heartbeat: Instant,
    brightness: Option<u8>,
    flash: [Option<Instant>; 14],
    providers_enabled: bool,
    next_diagnostic: Instant,
    inbound: u64,
    heartbeats: u64,
    acknowledgements: u64,
    next_keep_alive: Option<Instant>,
    last_brightness_write: Option<Instant>,
    probe_sent: bool,
    timing: DockTimings,
    last_poll_finished: Option<Instant>,
    non_button_since_button: u64,
    pending_navigation: Option<PendingNavigation>,
}
impl Worker {
    fn new(
        shared: Arc<Shared>,
        profiles: Arc<ProfileStore>,
        hooks: Arc<dyn DockHooks>,
        actions: SyncSender<Job>,
        completed: Receiver<Completion>,
    ) -> Self {
        let now = Instant::now();
        Self {
            shared,
            profiles,
            hooks,
            actions,
            completed,
            runtime: None,
            dock: None,
            owner: None,
            generation: 0,
            revision: u64::MAX,
            retry_at: now,
            retry_delay: Duration::from_secs(1),
            last_activity: now,
            next_render: now,
            next_probe: now,
            last_fake_heartbeat: now,
            brightness: None,
            flash: [None; 14],
            providers_enabled: false,
            next_diagnostic: now,
            inbound: 0,
            heartbeats: 0,
            acknowledgements: 0,
            next_keep_alive: None,
            last_brightness_write: None,
            probe_sent: false,
            timing: DockTimings::default(),
            last_poll_finished: None,
            non_button_since_button: 0,
            pending_navigation: None,
        }
    }
    fn activity(&mut self, enabled: bool) {
        if let Some(r) = &self.runtime {
            self.hooks.active_profile(r.state(), enabled);
        }
        self.providers_enabled = enabled;
    }
    fn release(&mut self) {
        self.abort_navigation();
        self.last_poll_finished = None;
        self.non_button_since_button = 0;
        if self.dock.is_some() {
            self.shared.action_epoch.fetch_add(1, Ordering::SeqCst);
        }
        self.dock = None;
        self.owner = None;
        self.brightness = None;
        self.next_keep_alive = None;
        self.last_brightness_write = None;
        self.probe_sent = false;
        self.flash = [None; 14];
        if let Some(r) = &mut self.runtime {
            r.reset_input();
            r.invalidate_images();
        }
        if self.providers_enabled {
            self.activity(false);
        }
    }
    fn abort_navigation(&mut self) {
        if self.pending_navigation.take().is_some() {
            self.timing.navigations_aborted = self.timing.navigations_aborted.saturating_add(1);
        }
    }
    fn failed(&mut self, now: Instant, message: &str) {
        self.release();
        self.shared.status.lock_or_recover().errors += 1;
        self.shared.set_state("error", message);
        self.retry_at = now + self.retry_delay;
        self.retry_delay = (self.retry_delay * 2).min(Duration::from_secs(30));
    }
    /// The documented D200 small-window keepalive, with background mode chosen
    /// explicitly so the existing wide PNG is retained (the default is a clock).
    /// A full page must have supplied SmallViewMode:2 and its wide Icon first.
    fn keep_alive(&mut self, now: Instant) -> bool {
        if !self.next_keep_alive.is_some_and(|due| now >= due) {
            return true;
        }
        if !self.shared.safe_to_act() {
            self.release();
            return false;
        }
        let Some(dock) = &mut self.dock else {
            self.next_keep_alive = None;
            return true;
        };
        self.shared.writing.store(true, Ordering::SeqCst);
        let result = dock.set_small_window(&SmallWindowData {
            mode: SmallWindowMode::Background,
            ..SmallWindowData::default()
        });
        self.shared.writing.store(false, Ordering::SeqCst);
        if result.is_err() {
            self.failed(now, "Dock keepalive write failed; reconnecting");
            return false;
        }
        self.shared.status.lock_or_recover().keep_alives_sent += 1;
        self.next_keep_alive = Some(now + Duration::from_secs(5));
        true
    }
    /// A quiet keepalive-mode device need not send passive heartbeats. Ask for
    /// the already selected brightness again, whose known reply is 0x030a.
    /// Sending is not receiving: only poll_event may renew last_activity.
    fn liveness_probe(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.last_activity) < Duration::from_secs(25)
            || self.probe_sent
        {
            return true;
        }
        // A recent user brightness write already asked for the same reply.
        // Do not add a second request while that response is still pending.
        if self.last_brightness_write.is_some_and(|at| {
            at > self.last_activity && now.saturating_duration_since(at) < Duration::from_secs(25)
        }) {
            return true;
        }
        if !self.shared.safe_to_act() {
            self.release();
            return false;
        }
        let Some(dock) = &mut self.dock else {
            return true;
        };
        let brightness = self.shared.brightness.load(Ordering::SeqCst);
        self.shared.writing.store(true, Ordering::SeqCst);
        let result = dock.set_brightness(brightness);
        self.shared.writing.store(false, Ordering::SeqCst);
        if result.is_err() {
            self.failed(now, "Dock response probe failed; reconnecting");
            return false;
        }
        self.last_brightness_write = Some(now);
        self.probe_sent = true;
        self.shared.status.lock_or_recover().probe_writes += 1;
        true
    }
    fn run(mut self, rx: Receiver<Control>) {
        while !self.shared.stop.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(cmd) => self.control(cmd),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            for _ in 0..32 {
                if self.shared.stop.load(Ordering::SeqCst) {
                    break;
                }
                match rx.try_recv() {
                    Ok(cmd) => self.control(cmd),
                    Err(_) => break,
                }
            }
            if self.shared.stop.load(Ordering::SeqCst) {
                break;
            }
            self.step(Instant::now());
        }
        self.release();
        self.shared.set_state("disabled", "Dock is stopped");
        // Returning drops the only action sender. The executor finishes its
        // active transaction, skips queued jobs, and then exits.
    }
    fn control(&mut self, cmd: Control) {
        match cmd {
            Control::Wake => {}
            Control::Button { index, pressed } => {
                if self.shared.enabled.load(Ordering::SeqCst) {
                    if let Some(dock) = &mut self.dock {
                        if let Link::Fake(fake) = dock.transport_mut() {
                            if pressed {
                                fake.press(index)
                            } else {
                                fake.release(index)
                            }
                        }
                    }
                }
            }
            #[cfg(test)]
            Control::Disconnect => {
                self.failed(Instant::now(), "Simulated disconnect");
            }
        }
    }
    fn update_profile(&mut self, now: Instant) -> bool {
        let revision = self.shared.revision.load(Ordering::SeqCst);
        if self.runtime.is_some() && revision == self.revision {
            return true;
        }
        match self
            .profiles
            .state()
            .map_err(|e| e.to_string())
            .and_then(|s| {
                if let Some(runtime) = &mut self.runtime {
                    runtime.update_state(s)
                } else {
                    self.runtime = Some(PageRuntime::new(s)?);
                    Ok(())
                }
            }) {
            Ok(()) => {
                self.abort_navigation();
                self.shared.action_epoch.fetch_add(1, Ordering::SeqCst);
                self.revision = revision;
                self.shared
                    .applied_revision
                    .store(revision, Ordering::SeqCst);
                self.next_render = now;
                self.flash = [None; 14];
                self.activity(self.dock.is_some());
                true
            }
            Err(_) => {
                self.failed(now, "Dock layout could not be loaded; check the editor");
                false
            }
        }
    }
    fn step(&mut self, now: Instant) {
        let shared = self.shared.clone();
        let _admission = shared.admission.read_or_recover();
        if shared.quiesced.load(Ordering::SeqCst) {
            return;
        }
        self.step_inner(now);
        self.shared.status.lock_or_recover().timing = self.timing.clone();
    }
    /// The dock owner's state machine step. Each `bool` helper returns `false`
    /// when the step must end early (the original early `return`).
    fn step_inner(&mut self, now: Instant) {
        self.log_health(now);
        self.collect_completions(now);
        if !self.shared.enabled.load(Ordering::SeqCst) {
            self.release();
            self.retry_at = now;
            self.retry_delay = Duration::from_secs(1);
            self.shared.set_state("disabled", "Dock is disabled");
            return;
        }
        if !self.check_studio(now) {
            return;
        }
        if now < self.retry_at {
            return;
        }
        if !self.update_profile(now) {
            return;
        }
        if !self.connect_if_needed(now) {
            return;
        }
        if self.shared.stop.load(Ordering::SeqCst) || !self.shared.enabled.load(Ordering::SeqCst) {
            self.release();
            return;
        }
        if !self.sync_brightness(now) {
            return;
        }
        // Service a due keepalive before a potentially slow account render.
        if !self.keep_alive(now) {
            return;
        }
        if !self.liveness_probe(now) {
            return;
        }
        if now >= self.next_render && !self.render_page(now) {
            return;
        }
        // The initial keepalive follows successful full-page delivery, never
        // preceding the PNG/SmallViewMode baseline it refers to.
        if !self.keep_alive(now) {
            return;
        }
        self.fake_heartbeat(now);
        self.poll_input(now);
    }
    /// Periodic health and timing lines in device.log.
    fn log_health(&mut self, now: Instant) {
        if self.shared.enabled.load(Ordering::SeqCst) && now >= self.next_diagnostic {
            let s = self.shared.status();
            crate::device::log(&format!("dock health state={} frames={} pages={} errors={} inbound={} heartbeat={} ack={} keepalive={} probe={} last_rx_ms={}",s.state,s.frames_sent,s.pages_sent,s.errors,self.inbound,self.heartbeats,self.acknowledgements,s.keep_alives_sent,s.probe_writes,now.saturating_duration_since(self.last_activity).as_millis()));
            let t = &self.timing;
            let n = t.last_navigation.as_ref().cloned().unwrap_or_default();
            crate::device::log(&format!("dock timing polls={} nonbutton={} max_poll_gap_us={} nav_started={} nav_completed={} nav_aborted={} nav_receipt_to_delivery_us={} nav_handling_us={} nav_wait_for_render_us={} nav_inputs_us={} nav_faces={} nav_face_render_us={} nav_prepare_us={} nav_zip_and_transfer_us={} nav_preceding_nonbutton={} nav_poll_gap_us={}",t.polls,t.non_button_events,t.max_poll_gap_us,t.navigations_started,t.navigations_completed,t.navigations_aborted,n.receipt_to_delivery_us,n.handling_us,n.wait_for_render_us,n.inputs_us,n.render.faces_rendered,n.render.face_render_us,n.render.delivery_prepare_us,n.render.zip_and_transfer_us,n.preceding_non_button_events,n.poll_gap_before_receipt_us));
            self.next_diagnostic = now + Duration::from_secs(10);
        }
    }
    /// Account for key actions the executor has finished.
    fn collect_completions(&mut self, now: Instant) {
        while let Ok(done) = self.completed.try_recv() {
            let mut status = self.shared.status.lock_or_recover();
            if done.executed {
                status.actions += 1;
            }
            if done.failed {
                status.errors += 1;
                status.message = "A key action failed; review its settings".into();
            }
            drop(status);
            if done.failed
                && done.generation == self.generation
                && self
                    .runtime
                    .as_ref()
                    .is_some_and(|r| r.state().current_page == done.page)
            {
                self.flash[done.index] = Some(now + Duration::from_millis(900));
                self.next_render = now;
            }
        }
    }
    /// Ulanzi Studio ownership check.
    fn check_studio(&mut self, now: Instant) -> bool {
        // Probe before each connection and at most 250ms apart while connected.
        if self.dock.is_none() || now >= self.next_probe {
            self.next_probe = now + Duration::from_millis(250);
            match self.shared.studio() {
                Ok(false) => {}
                Ok(true) => {
                    self.release();
                    self.retry_at = now;
                    self.shared.set_state(
                        "blocked",
                        "Ulanzi Studio is running. Quit it from its tray to use the dock here",
                    );
                    return false;
                }
                Err(_) => {
                    self.release();
                    self.shared.set_state(
                        "blocked",
                        "Could not check Ulanzi Studio; dock access is paused",
                    );
                    return false;
                }
            }
        }
        true
    }
    /// Open the dock link when none is held.
    fn connect_if_needed(&mut self, now: Instant) -> bool {
        if self.dock.is_none() {
            self.shared
                .set_state("connecting", "Connecting to the dock");
            self.shared.writing.store(true, Ordering::SeqCst);
            let connection = if self.shared.fake {
                Ok((Link::Fake(FakeDock::new()), None))
            } else {
                native::connect().map(|(link, owner)| (link, Some(owner)))
            };
            self.shared.writing.store(false, Ordering::SeqCst);
            match connection {
                Ok((link, owner)) => {
                    self.dock = Some(Dock::new(link));
                    self.owner = owner;
                    self.generation += 1;
                    self.brightness = None;
                    self.next_keep_alive = None;
                    self.last_brightness_write = None;
                    self.probe_sent = false;
                    self.retry_delay = Duration::from_secs(1);
                    self.last_activity = now;
                    self.last_fake_heartbeat = now;
                    self.next_render = now;
                    let r = self.runtime.as_mut().unwrap();
                    r.reset_input();
                    r.invalidate_images();
                    self.activity(true);
                    self.shared.set_state(
                        "connected",
                        if self.shared.fake {
                            "Simulated dock connected"
                        } else {
                            "Dock connected"
                        },
                    );
                }
                Err(_) => {
                    self.failed(now, "Dock unavailable or owned by another app; retrying");
                    return false;
                }
            }
        }
        true
    }
    /// Write the selected brightness when it differs from the dock's.
    fn sync_brightness(&mut self, now: Instant) -> bool {
        let value = self.shared.brightness.load(Ordering::SeqCst);
        if self.brightness != Some(value) {
            self.shared.writing.store(true, Ordering::SeqCst);
            let result = self.dock.as_mut().unwrap().set_brightness(value);
            self.shared.writing.store(false, Ordering::SeqCst);
            if result.is_err() {
                self.failed(now, "Dock brightness write failed; reconnecting");
                return false;
            }
            self.brightness = Some(value);
            self.last_brightness_write = Some(now);
        }
        true
    }
    /// Render and deliver the current page, then schedule the next render.
    fn render_page(&mut self, now: Instant) -> bool {
        if !self.shared.enabled.load(Ordering::SeqCst) || self.shared.stop.load(Ordering::SeqCst) {
            return false;
        }
        self.shared.writing.store(true, Ordering::SeqCst);
        let render_started = Instant::now();
        let inputs_started = Instant::now();
        let inputs = self.hooks.inputs();
        self.timing.last_inputs_us = elapsed_us(inputs_started);
        let hooks = self.hooks.clone();
        let flash = self.flash;
        let shared = self.shared.clone();
        let revision = self.revision;
        let result = self.runtime.as_mut().unwrap().render_and_send_measured(
            self.dock.as_mut().unwrap(),
            &inputs,
            |index, face, input| {
                if flash[index].is_some_and(|until| now < until) {
                    render_face("text", "ERROR", index == 13, &RenderInputs::default())
                } else {
                    hooks.render(face, index, input)
                }
            },
            || shared.safe_to_act() && shared.revision.load(Ordering::SeqCst) == revision,
            &mut self.timing.last_render,
        );
        self.shared.writing.store(false, Ordering::SeqCst);
        if !self.shared.enabled.load(Ordering::SeqCst)
            || self.shared.stop.load(Ordering::SeqCst)
            || !matches!(self.shared.studio(), Ok(false))
        {
            self.release();
            return false;
        }
        match result {
            Ok(count) => self.record_delivery(now, render_started, count),
            Err(_) => {
                self.failed(now, "Dock picture update failed; reconnecting");
                return false;
            }
        }
        self.next_render = now + Duration::from_secs(1);
        if let Some(until) = self
            .flash
            .iter()
            .flatten()
            .filter(|until| **until > now)
            .min()
        {
            self.next_render = self.next_render.min(*until);
        }
        true
    }
    /// Status and navigation timing after a render delivered `count` frames.
    fn record_delivery(&mut self, now: Instant, render_started: Instant, count: usize) {
        let mut s = self.shared.status.lock_or_recover();
        s.frames_sent += count as u64;
        if count == 14 {
            s.pages_sent += 1;
            // Publish the displayed page only after its full transfer
            // succeeds, so editor following never leads the dock.
            let state = self.runtime.as_ref().unwrap().state();
            s.current_profile = Some(state.current_profile.clone());
            s.current_page = Some(state.current_page.clone());
            if self.next_keep_alive.is_none() {
                self.next_keep_alive = Some(now);
            }
            if let Some(pending) = self.pending_navigation.take() {
                self.timing.navigations_completed =
                    self.timing.navigations_completed.saturating_add(1);
                self.timing.last_navigation = Some(NavigationTiming {
                    receipt_to_delivery_us: elapsed_us(pending.received),
                    handling_us: pending
                        .handled
                        .saturating_duration_since(pending.received)
                        .as_micros()
                        .min(u64::MAX as u128) as u64,
                    wait_for_render_us: render_started
                        .saturating_duration_since(pending.handled)
                        .as_micros()
                        .min(u64::MAX as u128) as u64,
                    inputs_us: self.timing.last_inputs_us,
                    render: self.timing.last_render.clone(),
                    preceding_non_button_events: pending.preceding_non_button_events,
                    poll_gap_before_receipt_us: pending.poll_gap_before_receipt_us,
                });
            }
        }
    }
    /// The fake dock sends a heartbeat every ten seconds.
    fn fake_heartbeat(&mut self, now: Instant) {
        if self.shared.fake
            && now.duration_since(self.last_fake_heartbeat) >= Duration::from_secs(10)
        {
            if let Link::Fake(f) = self.dock.as_mut().unwrap().transport_mut() {
                f.heartbeat();
            }
            self.last_fake_heartbeat = now;
        }
    }
    /// Poll one inbound event and dispatch it (or detect a silent/lost dock).
    fn poll_input(&mut self, now: Instant) {
        let poll_started = Instant::now();
        self.timing.last_poll_gap_us = self.last_poll_finished.map_or(0, |at| {
            poll_started
                .saturating_duration_since(at)
                .as_micros()
                .min(u64::MAX as u128) as u64
        });
        self.timing.max_poll_gap_us = self
            .timing
            .max_poll_gap_us
            .max(self.timing.last_poll_gap_us);
        let result = self.dock.as_mut().unwrap().poll_event(if self.shared.fake {
            Duration::ZERO
        } else {
            Duration::from_millis(10)
        });
        let received = Instant::now();
        self.timing.last_poll_us = elapsed_us(poll_started);
        self.timing.polls = self.timing.polls.saturating_add(1);
        self.last_poll_finished = Some(received);
        match result {
            Ok(Some(event)) => self.handle_inbound(now, received, event),
            Ok(None) => {
                if now.duration_since(self.last_activity) >= Duration::from_secs(35) {
                    self.failed(now, "Dock stopped responding; reconnecting");
                }
            }
            Err(error) => {
                crate::device::log(&format!("dock input failure kind={:?}", error.kind()));
                self.failed(now, "Dock disconnected; reconnecting");
            }
        }
    }
    /// Count, recheck Studio ownership, then route a received event to the
    /// page runtime and the serial action executor.
    fn handle_inbound(&mut self, now: Instant, received: Instant, event: Event) {
        if !matches!(event, Event::Button(_)) {
            self.timing.non_button_events = self.timing.non_button_events.saturating_add(1);
            self.non_button_since_button = self.non_button_since_button.saturating_add(1);
        }
        let preceding_non_button_events = self.non_button_since_button;
        if matches!(event, Event::Button(_)) {
            self.non_button_since_button = 0;
        }
        self.inbound += 1;
        match &event {
            Event::Heartbeat(_) => self.heartbeats += 1,
            Event::In010b(_) | Event::BrightnessAck(_) => self.acknowledgements += 1,
            Event::DeviceInfo(_) => {
                crate::device::log("dock inbound device-info (payload omitted)")
            }
            _ => {}
        }
        self.last_activity = now;
        self.probe_sent = false;
        // Studio can start between periodic probes. Recheck before handling
        // a physical event and again inside the serial action executor.
        if !matches!(self.shared.studio(), Ok(false)) {
            self.release();
            self.shared.set_state(
                "blocked",
                "Ulanzi Studio is running or cannot be checked; dock access is paused",
            );
            return;
        }
        let index = match &event {
            Event::Button(b) if b.action == ButtonAction::Press && b.index < 14 => {
                Some(b.index as usize)
            }
            _ => None,
        };
        let runtime = self.runtime.as_mut().unwrap();
        let before = runtime.state().current_page.clone();
        match runtime.handle_event(&event) {
            Ok(action) => {
                let page = runtime.state().current_page.clone();
                if let (Some(action), Some(index)) = (action, index) {
                    self.shared.queued_actions.fetch_add(1, Ordering::SeqCst);
                    if self
                        .actions
                        .try_send(Job {
                            action,
                            index,
                            page,
                            generation: self.generation,
                            epoch: self.shared.action_epoch.load(Ordering::SeqCst),
                        })
                        .is_err()
                    {
                        self.shared.queued_actions.fetch_sub(1, Ordering::SeqCst);
                        self.flash[index] = Some(now + Duration::from_millis(900));
                        let mut s = self.shared.status.lock_or_recover();
                        s.errors += 1;
                        s.message = "Key action queue is full".into();
                    }
                }
                if before != self.runtime.as_ref().unwrap().state().current_page {
                    self.abort_navigation();
                    self.flash = [None; 14];
                    self.activity(true);
                    self.timing.navigations_started =
                        self.timing.navigations_started.saturating_add(1);
                    self.pending_navigation = Some(PendingNavigation {
                        received,
                        handled: Instant::now(),
                        preceding_non_button_events,
                        poll_gap_before_receipt_us: self.timing.last_poll_gap_us,
                    });
                }
                if matches!(event, Event::Button(_)) {
                    self.next_render = now;
                }
            }
            Err(_) => {
                self.shared.status.lock_or_recover().errors += 1;
                self.shared
                    .set_state("error", "Key navigation failed; review the page action");
            }
        }
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE, WAIT_ABANDONED,
            WAIT_OBJECT_0,
        },
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
        },
    };
    pub struct Owner(HANDLE);
    impl Drop for Owner {
        fn drop(&mut self) {
            unsafe {
                ReleaseMutex(self.0);
                CloseHandle(self.0);
            }
        }
    }
    pub fn studio_running() -> io::Result<bool> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        struct Snapshot(HANDLE);
        impl Drop for Snapshot {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let snapshot = Snapshot(snapshot);
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        while ok != 0 {
            let n = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..n])
                .eq_ignore_ascii_case("UlanziDeck.exe")
            {
                return Ok(true);
            }
            ok = unsafe { Process32NextW(snapshot.0, &mut entry) };
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
            Ok(false)
        } else {
            Err(error)
        }
    }
    pub fn connect() -> io::Result<(Link, Owner)> {
        let name: Vec<u16> = "Local\\UnifiedControl-D200H-2207-0019"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let wait = unsafe { WaitForSingleObject(handle, 0) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            unsafe {
                CloseHandle(handle);
            }
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "dock is already owned",
            ));
        }
        let owner = Owner(handle);
        // Verify again after ownership, immediately before the explicit HID open.
        if studio_running()? {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Studio is running",
            ));
        }
        let transport = ulanzi_dock::windows::WindowsHid::open_first(Duration::from_secs(2))?;
        Ok((Link::Native(transport), owner))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn prepared_exit_waits_for_transient_render_then_closes_admission() {
        let (_dir, store, hooks) = setup();
        hooks.hold_render.store(true, Ordering::SeqCst);
        let c = Controller::new(store, hooks.clone(), true);
        c.set_enabled(true).unwrap();
        wait(|| hooks.renders.load(Ordering::SeqCst) > 0);
        let release = hooks.clone();
        let released = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            release.hold_render.store(false, Ordering::SeqCst);
        });
        let started = Instant::now();
        let result = c.prepare_exit();
        released.join().unwrap();
        result.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(60));
        assert!(c.simulate_button(0, true).is_err());
        assert_eq!(hooks.calls.load(Ordering::SeqCst), 0);
        c.shutdown();
    }
    #[test]
    fn prepared_exit_closes_action_admission_and_cancel_restores_it() {
        let (_dir, store, hooks) = setup();
        let c = Controller::new(store, hooks.clone(), true);
        c.set_enabled(true).unwrap();
        wait(|| c.status().pages_sent == 1);
        wait(|| c.prepare_exit().is_ok());
        assert!(c.status().enabled);
        assert!(c.simulate_button(0, true).is_err());
        assert_eq!(hooks.calls.load(Ordering::SeqCst), 0);
        c.cancel_exit();
        c.simulate_button(0, true).unwrap();
        wait(|| hooks.calls.load(Ordering::SeqCst) == 1);
        c.shutdown();
    }
    use super::*;
    struct Hooks {
        calls: AtomicUsize,
        renders: AtomicUsize,
        fail: AtomicBool,
        hold: AtomicBool,
        release: AtomicBool,
        hold_render: AtomicBool,
        active: Mutex<Vec<bool>>,
    }
    impl Hooks {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                renders: AtomicUsize::new(0),
                fail: AtomicBool::new(false),
                hold: AtomicBool::new(false),
                release: AtomicBool::new(false),
                hold_render: AtomicBool::new(false),
                active: Mutex::new(Vec::new()),
            }
        }
    }
    impl DockHooks for Hooks {
        fn inputs(&self) -> RenderInputs {
            RenderInputs::default()
        }
        fn render(
            &self,
            face: &Face,
            index: usize,
            _: &RenderInputs,
        ) -> Result<RenderedFace, String> {
            self.renders.fetch_add(1, Ordering::SeqCst);
            while self.hold_render.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(5));
            }
            let (w, h) = if index == 13 { (458, 196) } else { (196, 196) };
            let color = face.text.bytes().fold(1u8, |a, b| a.wrapping_add(b));
            let mut png = Vec::new();
            let mut enc = png::Encoder::new(&mut png, w, h);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .unwrap()
                .write_image_data(&vec![color; (w * h * 3) as usize])
                .unwrap();
            Ok(RenderedFace {
                width: w,
                height: h,
                png,
                unknown: false,
                valid_until_ms: None,
            })
        }
        fn active_profile(&self, _: &ProfileState, enabled: bool) {
            self.active.lock().unwrap().push(enabled);
        }
        fn execute(&self, _: &PendingAction) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            while self.hold.load(Ordering::SeqCst) && !self.release.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(5));
            }
            if self.fail.load(Ordering::SeqCst) {
                Err("private action value must never appear in status".into())
            } else {
                Ok(())
            }
        }
    }
    fn wait(mut predicate: impl FnMut() -> bool) {
        let start = Instant::now();
        while !predicate() {
            assert!(
                start.elapsed() < Duration::from_secs(6),
                "fake controller condition timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn setup() -> (tempfile::TempDir, Arc<ProfileStore>, Arc<Hooks>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ProfileStore::new(dir.path()));
        let mut state = store.load().unwrap();
        let k = &mut state.profiles[0].pages[0].keys[0];
        k.a.text = "A".into();
        k.b.text = "B".into();
        k.action = "hotkey".into();
        k.value = "Ctrl + M".into();
        store.save(&state).unwrap();
        (dir, store, Arc::new(Hooks::new()))
    }
    fn stepped_worker(link: Link) -> (tempfile::TempDir, Arc<Shared>, Worker) {
        let (dir, store, hooks) = setup();
        let shared = Arc::new(Shared {
            status: Mutex::new(DockStatus {
                state: "connected".into(),
                message: "Simulated dock connected".into(),
                enabled: true,
                fake: true,
                current_profile: None,
                current_page: None,
                pages_sent: 0,
                frames_sent: 0,
                keep_alives_sent: 0,
                probe_writes: 0,
                actions: 0,
                errors: 0,
                timing: DockTimings::default(),
            }),
            enabled: AtomicBool::new(true),
            brightness: AtomicU8::new(50),
            revision: AtomicU64::new(0),
            applied_revision: AtomicU64::new(u64::MAX),
            stop: AtomicBool::new(false),
            fake_studio: AtomicBool::new(false),
            writing: AtomicBool::new(false),
            action_busy: AtomicBool::new(false),
            queued_actions: AtomicUsize::new(0),
            action_epoch: AtomicU64::new(0),
            fake: true,
            admission: RwLock::new(()),
            quiesced: AtomicBool::new(false),
        });
        let (actions, _action_rx) = mpsc::sync_channel(16);
        let (_completed, complete_rx) = mpsc::sync_channel(16);
        let mut worker = Worker::new(shared.clone(), store, hooks, actions, complete_rx);
        worker.dock = Some(Dock::new(link));
        (dir, shared, worker)
    }
    #[test]
    fn background_keepalive_runs_every_five_seconds_without_resending_or_replacing_images() {
        let output = tempfile::tempdir().unwrap();
        let (_dir, shared, mut worker) =
            stepped_worker(Link::Fake(FakeDock::with_out_dir(output.path())));
        let start = Instant::now();
        assert!(worker.next_keep_alive.is_none());
        worker.step(start);
        assert_eq!(shared.status().pages_sent, 1);
        assert_eq!(shared.status().keep_alives_sent, 1);
        let pictures = || {
            (0..14)
                .map(|index| {
                    std::fs::read(output.path().join(if index == 13 {
                        "wide.png".into()
                    } else {
                        format!("key_{index:02}.png")
                    }))
                    .unwrap()
                })
                .collect::<Vec<_>>()
        };
        let baseline = pictures();
        for (millis, expected) in [(4999, 1), (5000, 2), (9999, 2), (10000, 3)] {
            worker.step(start + Duration::from_millis(millis));
            assert_eq!(shared.status().keep_alives_sent, expected);
            assert_eq!(shared.status().pages_sent, 1);
            assert_eq!(shared.status().frames_sent, 14);
            assert_eq!(
                pictures(),
                baseline,
                "all thirteen keys and the wide image must stay intact"
            );
        }
        let Link::Fake(fake) = worker.dock.as_ref().unwrap().transport() else {
            panic!("fake only")
        };
        assert_eq!(fake.small_window.as_deref(), Some("2|0|0|00:00:00|0|24H|"));
        assert_eq!(fake.pages_received, 1);
        assert_eq!(fake.partial_updates_received, 0);
        assert!(fake.errors.is_empty());
        assert_eq!(
            fake.commands_received
                .iter()
                .filter(|&&command| command == 0x0006)
                .count(),
            3
        );
        shared.enabled.store(false, Ordering::SeqCst);
        worker.step(start + Duration::from_secs(15));
        worker.step(start + Duration::from_secs(25));
        assert_eq!(shared.status().keep_alives_sent, 3);
        assert!(worker.next_keep_alive.is_none());
        assert!(worker.dock.is_none());
    }
    #[test]
    fn response_probes_keep_a_quiet_device_connected_without_passive_heartbeats() {
        let (_dir, shared, mut worker) = stepped_worker(Link::ProbeFixture {
            dock: FakeDock::new(),
            replies: true,
        });
        let start = Instant::now();
        worker.step(start);
        // The initial brightness ACK is received, but this fixture never emits
        // the synthetic heartbeat used by the ordinary fake controller.
        assert_eq!(worker.heartbeats, 0);
        worker.step(start + Duration::from_millis(24_999));
        assert_eq!(shared.status().probe_writes, 0);
        worker.step(start + Duration::from_secs(25));
        assert_eq!(shared.status().probe_writes, 1);
        assert_eq!(worker.last_activity, start + Duration::from_secs(25));
        assert!(
            !worker.probe_sent,
            "only the received ACK completes the silence epoch"
        );
        worker.step(start + Duration::from_secs(35));
        worker.step(start + Duration::from_secs(50));
        worker.step(start + Duration::from_secs(75));
        assert_eq!(shared.status().probe_writes, 3);
        assert_eq!(shared.status().errors, 0);
        assert_eq!(shared.status().pages_sent, 1);
        assert_eq!(worker.heartbeats, 0);
        let Link::ProbeFixture { dock, .. } = worker.dock.as_ref().unwrap().transport() else {
            panic!("fixture only")
        };
        assert_eq!(
            dock.brightness,
            Some(50),
            "probe preserves the selected brightness"
        );
        assert_eq!(
            dock.commands_received
                .iter()
                .filter(|&&cmd| cmd == 0x000a)
                .count(),
            4
        );
        shared.enabled.store(false, Ordering::SeqCst);
        worker.step(start + Duration::from_secs(100));
        assert_eq!(shared.status().probe_writes, 3);
        assert!(worker.dock.is_none());
    }
    #[test]
    fn unanswered_probe_does_not_renew_rx_or_repeat_and_still_reconnects() {
        let (_dir, shared, mut worker) = stepped_worker(Link::ProbeFixture {
            dock: FakeDock::new(),
            replies: false,
        });
        let start = Instant::now();
        worker.last_activity = start;
        worker.step(start);
        worker.step(start + Duration::from_secs(25));
        assert_eq!(shared.status().probe_writes, 1);
        assert_eq!(
            worker.last_activity, start,
            "successful writes cannot establish receive liveness"
        );
        worker.step(start + Duration::from_secs(30));
        worker.step(start + Duration::from_millis(34_999));
        assert_eq!(shared.status().probe_writes, 1);
        assert!(worker.dock.is_some());
        worker.step(start + Duration::from_secs(35));
        assert_eq!(shared.status().probe_writes, 1);
        assert_eq!(shared.status().state, "error");
        assert_eq!(shared.status().errors, 1);
        assert!(worker.dock.is_none());
        assert!(!worker.probe_sent);
    }
    #[test]
    fn user_brightness_write_avoids_duplicate_probe_and_disable_blocks_a_due_probe() {
        let (_dir, shared, mut worker) = stepped_worker(Link::ProbeFixture {
            dock: FakeDock::new(),
            replies: false,
        });
        let start = Instant::now();
        worker.last_activity = start;
        worker.step(start);
        shared.brightness.store(42, Ordering::SeqCst);
        worker.step(start + Duration::from_secs(25));
        worker.step(start + Duration::from_secs(26));
        assert_eq!(shared.status().probe_writes, 0);
        let Link::ProbeFixture { dock, .. } = worker.dock.as_ref().unwrap().transport() else {
            panic!("fixture only")
        };
        assert_eq!(dock.brightness, Some(42));
        assert_eq!(
            dock.commands_received
                .iter()
                .filter(|&&cmd| cmd == 0x000a)
                .count(),
            2
        );
        // A due probe is blocked when the dock is disabled, even though there
        // has still been no reply to the user brightness write.
        shared.enabled.store(false, Ordering::SeqCst);
        worker.step(start + Duration::from_secs(34));
        assert_eq!(shared.status().probe_writes, 0);
        assert!(worker.dock.is_none());
    }
    #[test]
    fn rendering_cannot_send_after_admission_is_revoked() {
        for studio in [false, true] {
            let (_dir, store, hooks) = setup();
            hooks.hold_render.store(true, Ordering::SeqCst);
            let c = Controller::new(store, hooks.clone(), true);
            c.set_enabled(true).unwrap();
            wait(|| hooks.renders.load(Ordering::SeqCst) > 0);
            if studio {
                c.set_fake_studio(true).unwrap()
            } else {
                c.set_enabled(false).unwrap()
            }
            hooks.hold_render.store(false, Ordering::SeqCst);
            wait(|| c.status().state == if studio { "blocked" } else { "disabled" });
            assert_eq!(
                c.status().frames_sent,
                0,
                "a revoked render must never reach the dock"
            );
            c.shutdown();
        }
    }
    #[test]
    fn starts_disabled_without_profile_or_device_io_and_gates_studio() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ProfileStore::new(dir.path()));
        let hooks = Arc::new(Hooks::new());
        let c = Controller::new(store.clone(), hooks.clone(), true);
        thread::sleep(Duration::from_millis(50));
        assert!(!store.directory().exists());
        assert_eq!(c.status().state, "disabled");
        assert_eq!(hooks.renders.load(Ordering::SeqCst), 0);
        c.set_fake_studio(true).unwrap();
        c.set_enabled(true).unwrap();
        wait(|| c.status().state == "blocked");
        assert_eq!(c.status().pages_sent, 0);
        assert!(!store.directory().exists());
        c.set_fake_studio(false).unwrap();
        wait(|| c.status().pages_sent == 1);
        assert_eq!(c.status().state, "connected");
        c.set_enabled(false).unwrap();
        wait(|| c.status().state == "disabled");
        assert!(hooks.active.lock().unwrap().ends_with(&[false]));
        c.shutdown();
    }
    #[test]
    fn press_dedup_release_reconnect_and_studio_block_use_real_pipeline() {
        let (_dir, store, hooks) = setup();
        let c = Controller::new(store, hooks.clone(), true);
        c.set_enabled(true).unwrap();
        wait(|| c.status().pages_sent == 1);
        c.simulate_button(0, true).unwrap();
        c.simulate_button(0, true).unwrap();
        wait(|| hooks.calls.load(Ordering::SeqCst) == 1);
        thread::sleep(Duration::from_millis(100));
        assert_eq!(hooks.calls.load(Ordering::SeqCst), 1);
        c.simulate_button(0, false).unwrap();
        c.simulate_button(0, true).unwrap();
        wait(|| hooks.calls.load(Ordering::SeqCst) == 2);
        c.tx.send(Control::Disconnect).unwrap();
        wait(|| c.status().pages_sent == 2);
        assert!(hooks.renders.load(Ordering::SeqCst) >= 28);
        // Reconnect cleared a held press latch; no release is required now.
        c.simulate_button(0, true).unwrap();
        wait(|| hooks.calls.load(Ordering::SeqCst) == 3);
        c.set_fake_studio(true).unwrap();
        wait(|| c.status().state == "blocked");
        assert!(c.simulate_button(0, true).is_err());
        c.set_fake_studio(false).unwrap();
        wait(|| c.status().pages_sent == 3);
        c.shutdown();
    }
    #[test]
    fn displayed_page_status_waits_for_successful_navigation_transfer() {
        let (_dir, store, hooks) = setup();
        let mut state = store.state().unwrap();
        let main = state.current_page.clone();
        let page = super::super::profiles::Page::new("Test");
        let target = page.id.clone();
        state.profiles[0].pages[0].keys[0].action = "folder".into();
        state.profiles[0].pages[0].keys[0].value = target.clone();
        state.profiles[0].pages.push(page);
        store.save(&state).unwrap();
        let c = Controller::new(store, hooks.clone(), true);
        struct Unblock(Arc<Hooks>);
        impl Drop for Unblock {
            fn drop(&mut self) {
                self.0.hold_render.store(false, Ordering::SeqCst);
            }
        }
        let _unblock = Unblock(hooks.clone());
        c.set_enabled(true).unwrap();
        wait(|| c.status().pages_sent == 1);
        assert_eq!(c.status().current_page.as_deref(), Some(main.as_str()));
        let renders = hooks.renders.load(Ordering::SeqCst);
        hooks.hold_render.store(true, Ordering::SeqCst);
        c.simulate_button(0, true).unwrap();
        wait(|| hooks.renders.load(Ordering::SeqCst) > renders);
        let during = c.status().current_page;
        assert_eq!(c.status().timing.navigations_started, 1);
        assert_eq!(c.status().timing.navigations_completed, 0);
        // Hold the real render callback, not a mocked timer. Its elapsed time
        // must appear in both the face stage and host receipt-to-delivery span.
        thread::sleep(Duration::from_millis(50));
        hooks.hold_render.store(false, Ordering::SeqCst);
        wait(|| c.status().pages_sent == 2 && c.status().timing.navigations_completed == 1);
        assert_eq!(during.as_deref(), Some(main.as_str()));
        assert_eq!(c.status().current_page.as_deref(), Some(target.as_str()));
        let timing = c.status().timing;
        let navigation = timing.last_navigation.unwrap();
        assert_eq!(navigation.render.faces_rendered, 14);
        assert!(navigation.render.face_render_us >= 50_000);
        assert!(navigation.receipt_to_delivery_us >= navigation.render.face_render_us);
        assert_eq!(timing.navigations_aborted, 0);
        c.shutdown();
    }
    #[test]
    fn ack_burst_is_counted_before_navigation_without_claiming_physical_latency() {
        use ulanzi_dock::protocol::{build_simple_packet, Command};
        let (_dir, shared, mut worker) = stepped_worker(Link::Fake(FakeDock::new()));
        let mut state = worker.profiles.state().unwrap();
        let page = super::super::profiles::Page::new("private page name");
        let target = page.id.clone();
        state.profiles[0].pages[0].keys[0].action = "folder".into();
        state.profiles[0].pages[0].keys[0].value = target.clone();
        state.profiles[0].pages.push(page);
        worker.profiles.save(&state).unwrap();
        let now = Instant::now();
        worker.step(now);
        // Discard only baseline fixture replies, then inject a known FIFO burst.
        let Link::Fake(fake) = worker.dock.as_mut().unwrap().transport_mut() else {
            panic!("fake only")
        };
        while fake.read(Duration::ZERO).unwrap().is_some() {}
        for _ in 0..6 {
            fake.inject_raw(build_simple_packet(Command::In010b, &[]).unwrap());
            fake.heartbeat();
        }
        fake.press(0);
        let before = worker.timing.non_button_events;
        worker.non_button_since_button = 0;
        for index in 0..12 {
            worker.step(now + Duration::from_millis(index + 1));
            assert_eq!(shared.status().timing.navigations_started, 0);
        }
        assert_eq!(worker.timing.non_button_events - before, 12);
        worker.step(now + Duration::from_millis(13));
        assert_eq!(shared.status().timing.navigations_started, 1);
        assert_eq!(shared.status().timing.navigations_completed, 0);
        worker.step(now + Duration::from_millis(14));
        let status = shared.status();
        assert_eq!(status.current_page.as_deref(), Some(target.as_str()));
        assert_eq!(status.timing.navigations_completed, 1);
        assert_eq!(
            status
                .timing
                .last_navigation
                .as_ref()
                .unwrap()
                .preceding_non_button_events,
            12
        );
        assert_eq!(status.timing.last_render.faces_rendered, 14);
        // Diagnostics retain one fixed-size sample and finite numerical fields,
        // never a page identifier, label, action, account field or history list.
        fn numeric_tree(value: &serde_json::Value) -> bool {
            match value {
                serde_json::Value::Null | serde_json::Value::Number(_) => true,
                serde_json::Value::Object(fields) => fields.values().all(numeric_tree),
                _ => false,
            }
        }
        assert!(numeric_tree(&serde_json::to_value(status.timing).unwrap()));
    }
    #[test]
    fn disabling_aborts_pending_navigation_without_publishing_a_delivery_sample() {
        let (_dir, shared, mut worker) = stepped_worker(Link::Fake(FakeDock::new()));
        let now = Instant::now();
        worker.pending_navigation = Some(PendingNavigation {
            received: now,
            handled: now,
            preceding_non_button_events: 0,
            poll_gap_before_receipt_us: 0,
        });
        worker.timing.navigations_started = 1;
        shared.enabled.store(false, Ordering::SeqCst);
        worker.step(now);
        let timing = shared.status().timing;
        assert_eq!(timing.navigations_completed, 0);
        assert_eq!(timing.navigations_aborted, 1);
        assert!(timing.last_navigation.is_none());
        worker.step(now);
        assert_eq!(shared.status().timing.navigations_aborted, 1);
    }
    #[test]
    fn failed_action_flashes_then_restores_without_leaking_action_data() {
        let (_dir, store, hooks) = setup();
        hooks.fail.store(true, Ordering::SeqCst);
        let c = Controller::new(store, hooks, true);
        c.set_enabled(true).unwrap();
        wait(|| c.status().pages_sent == 1);
        c.simulate_button(0, true).unwrap();
        wait(|| c.status().errors == 1);
        // Completion may arrive before or after the first B-face redraw. In
        // either ordering the error and restored face must each reach the dock.
        thread::sleep(Duration::from_millis(1200));
        assert!(c.status().frames_sent >= 16);
        assert!(!c.status().message.contains("private"));
        assert_eq!(c.status().actions, 1);
        c.shutdown();
    }
    #[test]
    fn disabling_and_shutdown_wait_for_active_transaction_and_skip_queued_actions() {
        let (_dir, store, hooks) = setup();
        hooks.hold.store(true, Ordering::SeqCst);
        let c = Arc::new(Controller::new(store, hooks.clone(), true));
        c.set_enabled(true).unwrap();
        wait(|| c.status().pages_sent == 1);
        c.simulate_button(0, true).unwrap();
        wait(|| hooks.calls.load(Ordering::SeqCst) == 1);
        assert!(c.busy());
        assert!(c.prepare_exit().is_err());
        assert!(c.status().enabled);
        c.simulate_button(0, false).unwrap();
        c.simulate_button(0, true).unwrap();
        wait(|| c.shared.queued_actions.load(Ordering::SeqCst) > 0);
        c.set_enabled(false).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let (controller, flag) = (c.clone(), done.clone());
        let shutdown = thread::spawn(move || {
            controller.shutdown();
            flag.store(true, Ordering::SeqCst);
        });
        thread::sleep(Duration::from_millis(50));
        assert!(!done.load(Ordering::SeqCst));
        assert!(c.busy());
        hooks.release.store(true, Ordering::SeqCst);
        shutdown.join().unwrap();
        assert_eq!(hooks.calls.load(Ordering::SeqCst), 1);
        assert!(!c.busy());
        assert_eq!(c.status().state, "disabled");
    }
}
#[cfg(not(windows))]
mod native {
    use super::*;
    pub struct Owner;
    pub fn studio_running() -> io::Result<bool> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub fn connect() -> io::Result<(Link, Owner)> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
