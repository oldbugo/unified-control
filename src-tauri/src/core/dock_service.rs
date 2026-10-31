//! Explicitly enabled dock controller. One owner thread holds the transport and
//! native ownership mutex; a separate serial executor finishes active actions.
//! Construction starts disabled and performs no profile, process or device I/O.
use super::{
    dock_runtime::{PageRuntime, PendingAction},
    key_render::{render_face, RenderInputs, RenderedFace},
    profiles::{Face, ProfileState, ProfileStore},
};
use serde::Serialize;
use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
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
#[derive(Clone, Debug, Serialize)]
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
    pub actions: u64,
    pub errors: u64,
}
struct Shared {
    status: Mutex<DockStatus>,
    enabled: AtomicBool,
    brightness: AtomicU8,
    revision: AtomicU64,
    stop: AtomicBool,
    fake_studio: AtomicBool,
    writing: AtomicBool,
    action_busy: AtomicBool,
    queued_actions: AtomicUsize,
    action_epoch: AtomicU64,
    fake: bool,
}
impl Shared {
    fn status(&self) -> DockStatus {
        self.status.lock().unwrap().clone()
    }
    fn set_state(&self, state: &str, message: &str) {
        let mut s = self.status.lock().unwrap();
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
        if self.stop.load(Ordering::SeqCst) || !self.enabled.load(Ordering::SeqCst) {
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
                actions: 0,
                errors: 0,
            }),
            enabled: AtomicBool::new(false),
            brightness: AtomicU8::new(50),
            revision: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            fake_studio: AtomicBool::new(false),
            writing: AtomicBool::new(false),
            action_busy: AtomicBool::new(false),
            queued_actions: AtomicUsize::new(0),
            action_epoch: AtomicU64::new(0),
            fake,
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
        self.shared.status.lock().unwrap().enabled = enabled;
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
    pub fn status(&self) -> DockStatus {
        self.shared.status()
    }
    pub fn busy(&self) -> bool {
        self.shared.writing.load(Ordering::SeqCst)
            || self.shared.action_busy.load(Ordering::SeqCst)
            || self.shared.queued_actions.load(Ordering::SeqCst) > 0
    }
    pub fn simulate_button(&self, index: usize, pressed: bool) -> Result<(), String> {
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
        if let Some((worker, executor)) = self.threads.lock().unwrap().take() {
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
        shared.action_busy.store(true, Ordering::SeqCst);
        shared.queued_actions.fetch_sub(1, Ordering::SeqCst);
        let executed = shared.action_epoch.load(Ordering::SeqCst) == job.epoch
            && shared.safe_to_act()
            && shared.action_epoch.load(Ordering::SeqCst) == job.epoch;
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
    #[cfg(windows)]
    Native(ulanzi_dock::windows::WindowsHid),
}
impl Transport for Link {
    fn write(&mut self, p: &Packet) -> io::Result<()> {
        match self {
            Self::Fake(f) => f.write(p),
            #[cfg(windows)]
            Self::Native(n) => n.write(p),
        }
    }
    fn read(&mut self, t: Duration) -> io::Result<Option<Packet>> {
        match self {
            Self::Fake(f) => f.read(t),
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
        }
    }
    fn activity(&mut self, enabled: bool) {
        if let Some(r) = &self.runtime {
            self.hooks.active_profile(r.state(), enabled);
        }
        self.providers_enabled = enabled;
    }
    fn release(&mut self) {
        if self.dock.is_some() {
            self.shared.action_epoch.fetch_add(1, Ordering::SeqCst);
        }
        self.dock = None;
        self.owner = None;
        self.brightness = None;
        self.next_keep_alive = None;
        self.flash = [None; 14];
        if let Some(r) = &mut self.runtime {
            r.reset_input();
            r.invalidate_images();
        }
        if self.providers_enabled {
            self.activity(false);
        }
    }
    fn failed(&mut self, now: Instant, message: &str) {
        self.release();
        self.shared.status.lock().unwrap().errors += 1;
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
        self.shared.status.lock().unwrap().keep_alives_sent += 1;
        self.next_keep_alive = Some(now + Duration::from_secs(5));
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
                self.shared.action_epoch.fetch_add(1, Ordering::SeqCst);
                self.revision = revision;
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
        if self.shared.enabled.load(Ordering::SeqCst) && now >= self.next_diagnostic {
            let s = self.shared.status();
            crate::device::log(&format!("dock health state={} frames={} pages={} errors={} inbound={} heartbeat={} ack={} keepalive={} last_rx_ms={}",s.state,s.frames_sent,s.pages_sent,s.errors,self.inbound,self.heartbeats,self.acknowledgements,s.keep_alives_sent,now.saturating_duration_since(self.last_activity).as_millis()));
            self.next_diagnostic = now + Duration::from_secs(10);
        }
        while let Ok(done) = self.completed.try_recv() {
            let mut status = self.shared.status.lock().unwrap();
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
        if !self.shared.enabled.load(Ordering::SeqCst) {
            self.release();
            self.retry_at = now;
            self.retry_delay = Duration::from_secs(1);
            self.shared.set_state("disabled", "Dock is disabled");
            return;
        }
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
                    return;
                }
                Err(_) => {
                    self.release();
                    self.shared.set_state(
                        "blocked",
                        "Could not check Ulanzi Studio; dock access is paused",
                    );
                    return;
                }
            }
        }
        if now < self.retry_at {
            return;
        }
        if !self.update_profile(now) {
            return;
        }
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
                    return;
                }
            }
        }
        if self.shared.stop.load(Ordering::SeqCst) || !self.shared.enabled.load(Ordering::SeqCst) {
            self.release();
            return;
        }
        let value = self.shared.brightness.load(Ordering::SeqCst);
        if self.brightness != Some(value) {
            self.shared.writing.store(true, Ordering::SeqCst);
            let result = self.dock.as_mut().unwrap().set_brightness(value);
            self.shared.writing.store(false, Ordering::SeqCst);
            if result.is_err() {
                self.failed(now, "Dock brightness write failed; reconnecting");
                return;
            }
            self.brightness = Some(value);
        }
        // Service a due keepalive before a potentially slow account render.
        if !self.keep_alive(now) {
            return;
        }
        if now >= self.next_render {
            if !self.shared.enabled.load(Ordering::SeqCst)
                || self.shared.stop.load(Ordering::SeqCst)
            {
                return;
            }
            self.shared.writing.store(true, Ordering::SeqCst);
            let inputs = self.hooks.inputs();
            let hooks = self.hooks.clone();
            let flash = self.flash;
            let shared = self.shared.clone();
            let revision = self.revision;
            let result = self.runtime.as_mut().unwrap().render_and_send_guarded(
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
            );
            self.shared.writing.store(false, Ordering::SeqCst);
            if !self.shared.enabled.load(Ordering::SeqCst)
                || self.shared.stop.load(Ordering::SeqCst)
                || !matches!(self.shared.studio(), Ok(false))
            {
                self.release();
                return;
            }
            match result {
                Ok(count) => {
                    let mut s = self.shared.status.lock().unwrap();
                    s.frames_sent += count as u64;
                    if count == 14 {
                        s.pages_sent += 1;
                        if self.next_keep_alive.is_none() {
                            self.next_keep_alive = Some(now);
                        }
                    }
                }
                Err(_) => {
                    self.failed(now, "Dock picture update failed; reconnecting");
                    return;
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
        }
        // The initial keepalive follows successful full-page delivery, never
        // preceding the PNG/SmallViewMode baseline it refers to.
        if !self.keep_alive(now) {
            return;
        }
        if self.shared.fake
            && now.duration_since(self.last_fake_heartbeat) >= Duration::from_secs(10)
        {
            if let Link::Fake(f) = self.dock.as_mut().unwrap().transport_mut() {
                f.heartbeat();
            }
            self.last_fake_heartbeat = now;
        }
        let result = self.dock.as_mut().unwrap().poll_event(if self.shared.fake {
            Duration::ZERO
        } else {
            Duration::from_millis(10)
        });
        match result {
            Ok(Some(event)) => {
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
                                let mut s = self.shared.status.lock().unwrap();
                                s.errors += 1;
                                s.message = "Key action queue is full".into();
                            }
                        }
                        if before != self.runtime.as_ref().unwrap().state().current_page {
                            self.flash = [None; 14];
                            self.activity(true);
                        }
                        if matches!(event, Event::Button(_)) {
                            self.next_render = now;
                        }
                    }
                    Err(_) => {
                        self.shared.status.lock().unwrap().errors += 1;
                        self.shared
                            .set_state("error", "Key navigation failed; review the page action");
                    }
                }
            }
            Ok(None) => {
                if now.duration_since(self.last_activity) >= Duration::from_secs(35) {
                    self.failed(now, "Dock stopped responding; reconnecting");
                    return;
                }
            }
            Err(error) => {
                crate::device::log(&format!("dock input failure kind={:?}", error.kind()));
                self.failed(now, "Dock disconnected; reconnecting");
                return;
            }
        }
        if let Some(runtime) = &self.runtime {
            let mut s = self.shared.status.lock().unwrap();
            s.current_profile = Some(runtime.state().current_profile.clone());
            s.current_page = Some(runtime.state().current_page.clone());
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
    #[test]
    fn background_keepalive_runs_every_five_seconds_without_resending_or_replacing_images() {
        let (_dir, store, hooks) = setup();
        let output = tempfile::tempdir().unwrap();
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
                actions: 0,
                errors: 0,
            }),
            enabled: AtomicBool::new(true),
            brightness: AtomicU8::new(50),
            revision: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            fake_studio: AtomicBool::new(false),
            writing: AtomicBool::new(false),
            action_busy: AtomicBool::new(false),
            queued_actions: AtomicUsize::new(0),
            action_epoch: AtomicU64::new(0),
            fake: true,
        });
        let (actions, _action_rx) = mpsc::sync_channel(16);
        let (_completed, complete_rx) = mpsc::sync_channel(16);
        let mut worker = Worker::new(shared.clone(), store, hooks, actions, complete_rx);
        let start = Instant::now();
        worker.dock = Some(Dock::new(Link::Fake(FakeDock::with_out_dir(output.path()))));
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
