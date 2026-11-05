//! Shared rendering and reviewed action adapters. Construction activates no source.
use super::{
    actions::{ActionSink, NativeActions},
    assets::AssetStore,
    dock_runtime::PendingAction,
    dock_service::DockHooks,
    key_render::{render_face, RenderInputs, RenderedFace, KEY_SIZE, WIDE_SIZE},
    profiles::{Face, ProfileState},
    sensors::{Needs, Sampler},
    usage::{ActiveProvider, Provider, UsageError, UsageService, UsageStatus},
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
struct ReaderHealth {
    next: Instant,
    delay: Duration,
}

#[derive(Clone, serde::Serialize)]
struct UsageDiagnostic {
    provider: Provider,
    selected: bool,
    status: UsageStatus,
    code: &'static str,
    message: &'static str,
}
impl UsageDiagnostic {
    fn inactive(provider: Provider) -> Self {
        Self {
            provider,
            selected: false,
            status: UsageStatus::Inactive,
            code: "inactive",
            message: UsageStatus::Inactive.message(),
        }
    }
}
struct UsageDiagnostics {
    reader_code: &'static str,
    reader_message: &'static str,
    providers: [UsageDiagnostic; 2],
}
impl Default for UsageDiagnostics {
    fn default() -> Self {
        Self {
            reader_code: "inactive",
            reader_message: "AI usage reader is inactive.",
            providers: [
                UsageDiagnostic::inactive(Provider::Claude),
                UsageDiagnostic::inactive(Provider::Codex),
            ],
        }
    }
}

pub struct RuntimeResources {
    pub assets: AssetStore,
    pub usage: UsageService,
    pub resource_dir: PathBuf,
    pub fake: bool,
    enabled: AtomicBool,
    needs: Mutex<Needs>,
    sampler: Mutex<Sampler>,
    providers: Mutex<Vec<ActiveProvider>>,
    actions: Mutex<NativeActions>,
    activation: Mutex<()>,
    health: Mutex<ReaderHealth>,
    usage_diagnostics: Mutex<UsageDiagnostics>,
    started: Instant,
    shared_hardware: bool,
    hardware: Mutex<super::hardware::Receiver>,
    local_sensor_polls: std::sync::atomic::AtomicU64,
    shared_usage: bool,
    usage_receiver: Mutex<super::shared_usage::Receiver>,
    usage_physical: Mutex<Vec<super::shared_usage::RenderSpec>>,
    usage_recent: Mutex<Vec<(super::shared_usage::RenderSpec, Instant)>>,
}
impl RuntimeResources {
    pub fn new(home: &Path, resource_dir: PathBuf, fake: bool) -> Arc<Self> {
        Self::with_hardware_mode(home, resource_dir, fake, false, false)
    }
    pub fn new_shared(home: &Path, resource_dir: PathBuf, fake: bool) -> Arc<Self> {
        Self::with_hardware_mode(home, resource_dir, fake, true, false)
    }
    pub fn new_shared_services(
        home: &Path,
        resource_dir: PathBuf,
        fake: bool,
        shared_usage: bool,
    ) -> Arc<Self> {
        Self::with_hardware_mode(home, resource_dir, fake, true, shared_usage)
    }
    fn with_hardware_mode(
        home: &Path,
        resource_dir: PathBuf,
        fake: bool,
        shared_hardware: bool,
        shared_usage: bool,
    ) -> Arc<Self> {
        let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let resources = if cfg!(debug_assertions) {
            source_root.join("resources")
        } else {
            resource_dir.clone()
        };
        let script = if cfg!(debug_assertions) {
            source_root.join("../companions/usage-reader/reader.mjs")
        } else {
            resource_dir.join("usage-reader/reader.mjs")
        };
        let node = resource_dir.join("node/node.exe");
        let node = if node.is_file() {
            node
        } else if cfg!(debug_assertions) {
            find_node().unwrap_or_else(|| PathBuf::from("node.exe"))
        } else {
            node
        };
        Arc::new(Self {
            assets: AssetStore::new(home),
            usage: UsageService::new(node, script, home),
            resource_dir: resources.clone(),
            fake,
            enabled: AtomicBool::new(false),
            needs: Mutex::new(Needs::default()),
            sampler: Mutex::new(Sampler::new()),
            providers: Mutex::new(Vec::new()),
            actions: Mutex::new(NativeActions::new(resources, home.into(), fake)),
            activation: Mutex::new(()),
            health: Mutex::new(ReaderHealth {
                next: Instant::now(),
                delay: Duration::from_secs(1),
            }),
            usage_diagnostics: Mutex::new(UsageDiagnostics::default()),
            started: Instant::now(),
            shared_hardware,
            hardware: Mutex::new(super::hardware::Receiver::default()),
            local_sensor_polls: std::sync::atomic::AtomicU64::new(0),
            shared_usage,
            usage_receiver: Mutex::new(super::shared_usage::Receiver::default()),
            usage_physical: Mutex::new(Vec::new()),
            usage_recent: Mutex::new(Vec::new()),
        })
    }
    pub fn usage_exchange(
        &self,
        batch: Option<super::shared_usage::Batch>,
    ) -> Result<Value, String> {
        if !self.shared_usage {
            return Err("Shared usage mode is disabled".into());
        }
        if let Some(batch) = batch {
            self.usage_receiver
                .lock()
                .unwrap()
                .accept(batch, super::scheduler::now_ms())?;
        }
        Ok(json!({"demand":self.usage_demand(),"state":self.usage_state()}))
    }
    pub fn usage_demand(&self) -> super::shared_usage::Demand {
        if !self.enabled.load(Ordering::SeqCst) {
            return super::shared_usage::Demand::default();
        }
        let providers = self.providers.lock().unwrap().clone();
        let mut renders = self.usage_physical.lock().unwrap().clone();
        let mut recent = self.usage_recent.lock().unwrap();
        recent
            .retain(|(_, at)| at.elapsed() < Duration::from_millis(super::shared_usage::LEASE_MS));
        for (spec, _) in recent.iter().rev() {
            if renders.len() >= super::shared_usage::MAX_SPECS {
                break;
            }
            if !renders.contains(spec) {
                renders.push(spec.clone());
            }
        }
        renders.retain(|spec| {
            providers
                .iter()
                .any(|p| p.provider == spec.provider && (spec.view != "tokens" || p.token_history))
        });
        super::shared_usage::Demand { providers, renders }
    }
    pub fn usage_state(&self) -> Value {
        let mut state = self
            .usage_receiver
            .lock()
            .unwrap()
            .status(&self.providers.lock().unwrap(), super::scheduler::now_ms());
        state["shared"] = json!(self.shared_usage);
        state
    }
    pub fn hardware_exchange(
        &self,
        snapshot: Option<super::hardware::Snapshot>,
    ) -> Result<Value, String> {
        if !self.shared_hardware {
            return Err("Shared hardware mode is disabled".into());
        }
        if let Some(snapshot) = snapshot {
            self.hardware
                .lock()
                .unwrap()
                .accept(snapshot, super::scheduler::now_ms())?;
        }
        Ok(json!({"demand":self.hardware_demand(),"state":self.hardware_state()}))
    }
    fn hardware_demand(&self) -> super::hardware::Demand {
        let needs = *self.needs.lock().unwrap();
        let enabled = self.enabled.load(Ordering::SeqCst);
        super::hardware::Demand {
            cpu: enabled && needs.cpu,
            ram: enabled && needs.ram,
            gpu: enabled && needs.gpu,
            hz: 1,
        }
    }
    pub fn hardware_state(&self) -> Value {
        let mut state = self
            .hardware
            .lock()
            .unwrap()
            .state(super::scheduler::now_ms());
        state["shared"] = json!(self.shared_hardware);
        state["demand"] = json!(self.hardware_demand());
        state["local_polls"] = json!(self.local_sensor_polls.load(Ordering::SeqCst));
        state
    }
    pub fn shutdown(&self) {
        let _activation = self.activation.lock().unwrap();
        self.enabled.store(false, Ordering::SeqCst);
        self.providers.lock().unwrap().clear();
        self.usage.shutdown();
        self.usage_receiver.lock().unwrap().clear();
        self.usage_physical.lock().unwrap().clear();
        self.usage_recent.lock().unwrap().clear();
        *self.usage_diagnostics.lock().unwrap() = UsageDiagnostics::default();
        self.sampler
            .lock()
            .unwrap()
            .sample(Needs::default(), self.started.elapsed());
    }
    /// Sanitized diagnostics only; inspecting this never activates a source or
    /// returns quota numbers, account identifiers, filesystem paths or raw errors.
    pub fn usage_status(&self) -> Value {
        if self.shared_usage {
            return self.usage_state();
        }
        let diagnostics = self.usage_diagnostics.lock().unwrap();
        json!({"reader":{"code":diagnostics.reader_code,"message":diagnostics.reader_message},"providers":diagnostics.providers})
    }
    fn reader_result(&self, result: Result<(), UsageError>) {
        let mut diagnostics = self.usage_diagnostics.lock().unwrap();
        let (code,message)=match result {
            Ok(())=>("ready","AI usage reader is running; each provider still needs a current verified observation."),
            Err(error)=>(error.code(),error.message()),
        };
        if diagnostics.reader_code != code {
            // Only finite codes are logged, never helper/OS text or user data.
            crate::device::log(&format!("usage reader status: {code}"));
        }
        diagnostics.reader_code = code;
        diagnostics.reader_message = message;
        if let Err(error) = result {
            for provider in &mut diagnostics.providers {
                if provider.selected {
                    provider.status = UsageStatus::Error;
                    provider.code = error.code();
                    provider.message = error.message();
                }
            }
        }
    }
    fn ensure_reader(&self) {
        if self.shared_usage || self.fake || !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        let _activation = self.activation.lock().unwrap();
        if !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        let desired = self.providers.lock().unwrap().clone();
        if desired.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut health = self.health.lock().unwrap();
        if now < health.next {
            return;
        }
        if self.usage.is_running() && self.usage.check_health() {
            self.reader_result(Ok(()));
            health.next = now + Duration::from_secs(30);
            health.delay = Duration::from_secs(1);
            return;
        }
        let activation = self.usage.set_active_providers(&desired);
        self.reader_result(activation);
        if activation.is_ok() {
            health.next = now + Duration::from_secs(30);
            health.delay = Duration::from_secs(1)
        } else {
            health.next = now + health.delay;
            health.delay = (health.delay * 2).min(Duration::from_secs(30));
        }
    }
    pub fn render_one(
        &self,
        face: &Face,
        index: usize,
        input: &RenderInputs,
    ) -> Result<RenderedFace, String> {
        let (width, height) = if index == 13 { WIDE_SIZE } else { KEY_SIZE };
        let provider = match face.kind.as_str() {
            "claude" => Some(Provider::Claude),
            "codex" | "reset" | "trend" => Some(Provider::Codex),
            _ => None,
        };
        if let Some(provider) = provider {
            if self.shared_usage {
                if self.enabled.load(Ordering::SeqCst) {
                    if let Some(spec) = super::shared_usage::RenderSpec::from_face(face, index) {
                        let selected = self.providers.lock().unwrap().iter().any(|p| {
                            p.provider == provider && (spec.view != "tokens" || p.token_history)
                        });
                        if selected {
                            let mut recent = self.usage_recent.lock().unwrap();
                            recent.retain(|(_, at)| {
                                at.elapsed() < Duration::from_millis(super::shared_usage::LEASE_MS)
                            });
                            if let Some((_, at)) = recent.iter_mut().find(|(s, _)| *s == spec) {
                                *at = Instant::now();
                            } else {
                                if recent.len() >= super::shared_usage::MAX_SPECS {
                                    recent.remove(0);
                                }
                                recent.push((spec.clone(), Instant::now()));
                            }
                            drop(recent);
                            if let Some(frame) = self
                                .usage_receiver
                                .lock()
                                .unwrap()
                                .render(&spec, super::scheduler::now_ms())
                            {
                                return Ok(frame);
                            }
                        }
                    }
                }
                return render_face(&face.kind, "", index == 13, input);
            }
            if self.enabled.load(Ordering::SeqCst) && !self.fake {
                let view = match face.kind.as_str() {
                    "reset" => "resets",
                    "trend" => "trend",
                    _ => face.usage_view.as_deref().unwrap_or("rings"),
                };
                let result = self.usage.render(provider, view, width, height);
                {
                    let mut diagnostics = self.usage_diagnostics.lock().unwrap();
                    let diagnostic = diagnostics
                        .providers
                        .iter_mut()
                        .find(|item| item.provider == provider)
                        .unwrap();
                    match &result {
                        Ok(rendered) => {
                            diagnostic.status = rendered.snapshot.status;
                            diagnostic.code = match diagnostic.status {
                                UsageStatus::Ok => "verified",
                                UsageStatus::Stale => "stale",
                                UsageStatus::Loading => "loading",
                                UsageStatus::Inactive => "inactive",
                                UsageStatus::Error => "source-error",
                                UsageStatus::Unavailable => "unverified-or-unavailable",
                            };
                            diagnostic.message = diagnostic.status.message();
                        }
                        Err(error) => {
                            diagnostic.status = UsageStatus::Error;
                            diagnostic.code = error.code();
                            diagnostic.message = error.message();
                        }
                    }
                }
                if let Ok(result) = result {
                    let snapshot = &result.snapshot;
                    let unknown = snapshot.status != super::usage::UsageStatus::Ok;
                    let valid_until_ms = snapshot.valid_until_ms(view);
                    return Ok(RenderedFace {
                        width,
                        height,
                        png: result.png,
                        unknown,
                        valid_until_ms,
                    });
                }
            }
            return render_face(&face.kind, "", index == 13, input);
        }
        let mut inputs = input.clone();
        inputs.icon = face.icon.clone();
        if let Some(id) = &face.asset {
            inputs.picture = self.assets.get(id).ok();
        }
        render_face(&face.kind, &face.text, index == 13, &inputs)
    }
}
impl DockHooks for RuntimeResources {
    fn inputs(&self) -> RenderInputs {
        self.ensure_reader();
        let mut inputs = local_time();
        if self.enabled.load(Ordering::SeqCst) && self.shared_hardware {
            if let Some(snapshot) = self
                .hardware
                .lock()
                .unwrap()
                .current(super::scheduler::now_ms())
            {
                inputs.hardware_valid_until_ms = Some(snapshot.expires_at_ms);
                inputs.cpu = snapshot.cpu_load_pct.map(|value| value as f32);
                inputs.ram = snapshot
                    .ram_used_bytes
                    .zip(snapshot.ram_total_bytes)
                    .filter(|(_, total)| *total > 0)
                    .map(|(used, total)| (used as f64 / total as f64 * 100.) as f32);
                inputs.gpu = snapshot
                    .gpu
                    .and_then(|gpu| gpu.load_pct)
                    .map(|value| value as f32);
            }
        } else if self.enabled.load(Ordering::SeqCst) && !self.fake {
            self.local_sensor_polls.fetch_add(1, Ordering::SeqCst);
            let readings = self
                .sampler
                .lock()
                .unwrap()
                .sample(*self.needs.lock().unwrap(), self.started.elapsed());
            inputs.cpu = readings.cpu;
            inputs.gpu = readings.gpu;
            inputs.ram = readings.ram;
        }
        inputs
    }
    fn render(
        &self,
        face: &Face,
        index: usize,
        input: &RenderInputs,
    ) -> Result<RenderedFace, String> {
        self.render_one(face, index, input)
    }
    fn active_profile(&self, state: &ProfileState, enabled: bool) {
        let _activation = self.activation.lock().unwrap();
        self.enabled.store(enabled, Ordering::SeqCst);
        let mut needs = Needs::default();
        let mut claude = false;
        let mut codex = false;
        let mut tokens = false;
        if enabled {
            // Hardware interest follows the delivered page. Account providers
            // retain their existing active-profile policy independently.
            if let Some(page) = state.active_page() {
                for face in page.keys.iter().flat_map(|key| [&key.a, &key.b]) {
                    match face.kind.as_str() {
                        "cpu" => needs.cpu = true,
                        "gpu" => needs.gpu = true,
                        "ram" => needs.ram = true,
                        _ => {}
                    }
                }
            }
            if let Some(profile) = state.active_profile() {
                for face in profile
                    .pages
                    .iter()
                    .flat_map(|page| page.keys.iter())
                    .flat_map(|key| [&key.a, &key.b])
                {
                    match face.kind.as_str() {
                        "claude" => claude = true,
                        "codex" => {
                            codex = true;
                            tokens |= face.usage_view.as_deref() == Some("tokens");
                        }
                        "reset" | "trend" => codex = true,
                        _ => {}
                    }
                }
            }
        }
        *self.needs.lock().unwrap() = needs;
        {
            let mut diagnostics = self.usage_diagnostics.lock().unwrap();
            for (provider, selected) in [(Provider::Claude, claude), (Provider::Codex, codex)] {
                let diagnostic = diagnostics
                    .providers
                    .iter_mut()
                    .find(|item| item.provider == provider)
                    .unwrap();
                let selected = selected && enabled && (!self.fake || self.shared_usage);
                if diagnostic.selected != selected {
                    *diagnostic = UsageDiagnostic::inactive(provider);
                    diagnostic.selected = selected;
                    if selected {
                        diagnostic.status = UsageStatus::Loading;
                        diagnostic.code = "loading";
                        diagnostic.message = UsageStatus::Loading.message();
                    }
                }
            }
            if !enabled || (!claude && !codex) || self.fake && !self.shared_usage {
                diagnostics.reader_code = "inactive";
                diagnostics.reader_message = "AI usage reader is inactive.";
            }
        }
        let mut providers = Vec::new();
        if enabled && (!self.fake || self.shared_usage) {
            if claude {
                providers.push(ActiveProvider {
                    provider: Provider::Claude,
                    token_history: false,
                });
            }
            if codex {
                providers.push(ActiveProvider {
                    provider: Provider::Codex,
                    token_history: tokens,
                });
            }
        }
        if self.shared_usage {
            let mut physical = Vec::new();
            if enabled {
                if let Some(page) = state.active_page() {
                    for (index, key) in page.keys.iter().enumerate() {
                        for face in [&key.a, &key.b] {
                            if let Some(spec) =
                                super::shared_usage::RenderSpec::from_face(face, index)
                            {
                                if !physical.contains(&spec) {
                                    physical.push(spec);
                                }
                            }
                        }
                    }
                }
            }
            *self.usage_physical.lock().unwrap() = physical;
            *self.providers.lock().unwrap() = providers;
            if !enabled {
                self.usage_recent.lock().unwrap().clear();
            }
            return;
        }
        let mut previous = self.providers.lock().unwrap();
        if *previous != providers || !providers.is_empty() && !self.usage.is_running() {
            *previous = providers.clone();
            let activation = self.usage.set_active_providers(&providers);
            if !providers.is_empty() {
                self.reader_result(activation);
            }
            let ok = activation.is_ok();
            let mut health = self.health.lock().unwrap();
            health.delay = Duration::from_secs(1);
            health.next = Instant::now()
                + if ok {
                    Duration::from_secs(30)
                } else {
                    Duration::from_secs(1)
                };
        }
        if !enabled {
            self.sampler
                .lock()
                .unwrap()
                .sample(Needs::default(), self.started.elapsed());
        }
    }
    fn execute(&self, action: &PendingAction) -> Result<(), String> {
        if self.fake {
            super::actions::plan_action(action).map(|_| ())
        } else {
            self.actions.lock().unwrap().execute(action)
        }
    }
}
fn find_node() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|value| {
        std::env::split_paths(&value)
            .map(|dir| dir.join("node.exe"))
            .find(|path| path.is_file())
    })
}
pub fn local_time() -> RenderInputs {
    #[cfg(windows)]
    unsafe {
        let mut time: windows_sys::Win32::Foundation::SYSTEMTIME = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut time);
        return RenderInputs {
            clock: Some(format!("{:02}:{:02}", time.wHour, time.wMinute)),
            date: Some(format!(
                "{:04}-{:02}-{:02}",
                time.wYear, time.wMonth, time.wDay
            )),
            ..Default::default()
        };
    }
    #[cfg(not(windows))]
    {
        RenderInputs::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_usage_prefetches_both_faces_preserves_profile_policy_and_never_starts_local_reader() {
        let home = tempfile::tempdir().unwrap();
        let r = RuntimeResources::new_shared_services(
            home.path(),
            home.path().join("resources"),
            true,
            true,
        );
        let mut state = ProfileState::default();
        state.profiles[0].pages[0].keys[0].a.kind = "claude".into();
        state.profiles[0].pages[0].keys[0].b.kind = "codex".into();
        state.profiles[0].pages[0].keys[0].b.usage_view = Some("tokens".into());
        r.active_profile(&state, true);
        let demand = r.usage_demand();
        demand.validate().unwrap();
        assert_eq!(demand.providers.len(), 2);
        assert_eq!(demand.renders.len(), 2);
        assert!(demand
            .providers
            .iter()
            .any(|p| p.provider == Provider::Codex && p.token_history));
        assert!(!r.usage.is_running());
        let empty = super::super::profiles::Page::new("No AI faces");
        state.current_page = empty.id.clone();
        state.profiles[0].pages.push(empty);
        r.active_profile(&state, true);
        assert_eq!(r.usage_demand().providers.len(), 2);
        assert!(r.usage_demand().renders.is_empty());
        r.active_profile(&state, false);
        assert!(r.usage_demand().providers.is_empty());
        assert!(!r.usage.is_running());
    }
    #[test]
    fn unselected_preview_cannot_activate_a_shared_account_provider() {
        let home = tempfile::tempdir().unwrap();
        let r = RuntimeResources::new_shared_services(
            home.path(),
            home.path().join("resources"),
            true,
            true,
        );
        let state = ProfileState::default();
        r.active_profile(&state, true);
        let frame = r
            .render_one(
                &Face {
                    kind: "codex".into(),
                    ..Default::default()
                },
                0,
                &RenderInputs::default(),
            )
            .unwrap();
        assert!(frame.unknown);
        assert!(r.usage_demand().providers.is_empty());
        assert!(r.usage_demand().renders.is_empty());
        assert!(!r.usage.is_running());
    }
    #[test]
    fn hardware_demand_stops_on_pages_without_sensor_faces() {
        let home = tempfile::tempdir().unwrap();
        let runtime =
            RuntimeResources::new_shared(home.path(), home.path().join("resources"), true);
        let mut state = ProfileState::default();
        state.profiles[0].pages[0].keys[0].a.kind = "cpu".into();
        state.profiles[0].pages[0].keys[1].b.kind = "gpu".into();
        runtime.active_profile(&state, true);
        let demand = runtime.hardware_demand();
        assert!(demand.cpu && demand.gpu && !demand.ram);
        let empty = super::super::profiles::Page::new("No hardware");
        state.current_page = empty.id.clone();
        state.profiles[0].pages.push(empty);
        runtime.active_profile(&state, true);
        assert!(!runtime.hardware_demand().active());
        state.current_page = state.profiles[0].pages[0].id.clone();
        runtime.active_profile(&state, true);
        assert!(runtime.hardware_demand().cpu);
        runtime.active_profile(&state, false);
        assert!(!runtime.hardware_demand().active());
        assert_eq!(runtime.local_sensor_polls.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn usage_diagnostics_are_inert_finite_and_contain_no_numbers_or_paths() {
        let home = tempfile::tempdir().unwrap();
        let runtime = RuntimeResources::new(home.path(), home.path().join("resources"), true);
        let diagnostic = runtime.usage_status();
        assert_eq!(diagnostic["reader"]["code"], "inactive");
        assert_eq!(diagnostic["providers"][0]["status"], "inactive");
        assert!(!runtime.usage.is_running());
        let frame = runtime
            .render_one(
                &Face {
                    kind: "codex".into(),
                    ..Default::default()
                },
                0,
                &RenderInputs::default(),
            )
            .unwrap();
        assert!(frame.unknown);
        assert!(!runtime.usage.is_running());
        let text = diagnostic.to_string();
        for forbidden in [
            "usedPercent",
            "observedAt",
            "accountId",
            &home.path().display().to_string(),
        ] {
            assert!(!text.contains(forbidden));
        }
        runtime.shutdown();
    }
}
