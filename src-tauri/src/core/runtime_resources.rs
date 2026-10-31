//! Shared rendering and reviewed action adapters. Construction activates no source.
use super::{
    actions::{ActionSink, NativeActions},
    assets::AssetStore,
    dock_runtime::PendingAction,
    dock_service::DockHooks,
    key_render::{render_face, RenderInputs, RenderedFace, KEY_SIZE, WIDE_SIZE},
    profiles::{Face, ProfileState},
    sensors::{Needs, Sampler},
    usage::{ActiveProvider, Provider, UsageService},
};
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
    started: Instant,
}
impl RuntimeResources {
    pub fn new(home: &Path, resource_dir: PathBuf, fake: bool) -> Arc<Self> {
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
            started: Instant::now(),
        })
    }
    pub fn shutdown(&self) {
        let _activation = self.activation.lock().unwrap();
        self.enabled.store(false, Ordering::SeqCst);
        self.providers.lock().unwrap().clear();
        self.usage.shutdown();
        self.sampler
            .lock()
            .unwrap()
            .sample(Needs::default(), self.started.elapsed());
    }
    fn ensure_reader(&self) {
        if self.fake || !self.enabled.load(Ordering::SeqCst) {
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
            health.next = now + Duration::from_secs(30);
            health.delay = Duration::from_secs(1);
            return;
        }
        if self.usage.set_active_providers(&desired).is_ok() {
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
            if self.enabled.load(Ordering::SeqCst) && !self.fake {
                let view = match face.kind.as_str() {
                    "reset" => "resets",
                    "trend" => "trend",
                    _ => face.usage_view.as_deref().unwrap_or("rings"),
                };
                if let Ok(result) = self.usage.render(provider, view, width, height) {
                    let snapshot = &result.snapshot;
                    let unknown = snapshot.status != super::usage::UsageStatus::Ok;
                    let mut valid_until_ms = if unknown {
                        None
                    } else {
                        snapshot.observed_at.map(|at| at.saturating_add(300_000))
                    };
                    for window in [&snapshot.five_hour, &snapshot.weekly]
                        .into_iter()
                        .flatten()
                    {
                        if let Some(at) = window.resets_at {
                            valid_until_ms = Some(
                                valid_until_ms
                                    .unwrap_or(u64::MAX)
                                    .min(at.saturating_mul(1000)),
                            );
                        }
                    }
                    if view == "tokens" {
                        if let Some(at) = snapshot.token_history_observed_at {
                            valid_until_ms = Some(
                                valid_until_ms
                                    .unwrap_or(u64::MAX)
                                    .min(at.saturating_add(7_200_000)),
                            );
                        }
                    }
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
        if self.enabled.load(Ordering::SeqCst) && !self.fake {
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
            if let Some(profile) = state.active_profile() {
                for face in profile
                    .pages
                    .iter()
                    .flat_map(|page| page.keys.iter())
                    .flat_map(|key| [&key.a, &key.b])
                {
                    match face.kind.as_str() {
                        "cpu" => needs.cpu = true,
                        "gpu" => needs.gpu = true,
                        "ram" => needs.ram = true,
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
        let mut providers = Vec::new();
        if enabled && !self.fake {
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
        let mut previous = self.providers.lock().unwrap();
        if *previous != providers || !providers.is_empty() && !self.usage.is_running() {
            *previous = providers.clone();
            let ok = self.usage.set_active_providers(&providers).is_ok();
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
