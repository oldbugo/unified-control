//! Independent coalescing deliveries. Neither consumer owns or can restart the
//! collector, and a blocked worker cannot hold up the other worker's readings.
use super::sync::MutexExt;
use super::{
    cooler::Cooler,
    dock_worker::Controller,
    hardware::{Demand, Service},
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, Weak,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct Bridge {
    stopped: AtomicBool,
    pause_dock: AtomicBool,
    pause_cooler: AtomicBool,
    threads: Mutex<Vec<JoinHandle<()>>>,
}
impl Bridge {
    pub fn start(service: Arc<Service>, dock: Weak<Controller>, cooler: Weak<Cooler>) -> Arc<Self> {
        let this = Arc::new(Self {
            stopped: AtomicBool::new(false),
            pause_dock: AtomicBool::new(false),
            pause_cooler: AtomicBool::new(false),
            threads: Mutex::new(Vec::new()),
        });
        let owner = Arc::downgrade(&this);
        let sensor_service = service.clone();
        // Each delivery loop restarts after a crash (logged) instead of dying silently.
        let dock_stopped = owner.clone();
        let dock_thread = thread::spawn(move || {
            let stopped = || {
                dock_stopped
                    .upgrade()
                    .is_none_or(|o| o.stopped.load(Ordering::SeqCst))
            };
            super::sync::restarting("hardware delivery to dock", stopped, || {
                let mut delivered = None;
                loop {
                    let Some(owner) = owner.upgrade() else { break };
                    if owner.stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    let Some(dock) = dock.upgrade() else { break };
                    let status = dock.status();
                    let worker = dock.worker_status();
                    if status.enabled && worker["state"] == "running" {
                        let candidate = sensor_service.snapshot();
                        let key = candidate
                            .as_ref()
                            .map(|s| (worker["pid"].as_u64(), s.generation, s.sequence));
                        let snapshot =
                            if owner.pause_dock.load(Ordering::SeqCst) || key == delivered {
                                None
                            } else {
                                candidate
                            };
                        if let Ok(reply) = dock.hardware_exchange(snapshot.clone()) {
                            if let Ok(demand) =
                                serde_json::from_value::<Demand>(reply["demand"].clone())
                            {
                                let _ = sensor_service.update_demand("dock", demand);
                            }
                            if snapshot.is_some() {
                                delivered = key;
                            }
                        }
                    } else {
                        sensor_service.remove_demand("dock");
                        delivered = None;
                    }
                    drop(owner);
                    thread::sleep(Duration::from_millis(125));
                }
                sensor_service.remove_demand("dock");
            });
        });
        let owner = Arc::downgrade(&this);
        let cooler_stopped = owner.clone();
        let cooler_thread = thread::spawn(move || {
            let stopped = || {
                cooler_stopped
                    .upgrade()
                    .is_none_or(|o| o.stopped.load(Ordering::SeqCst))
            };
            super::sync::restarting("hardware delivery to cooler", stopped, || {
                let mut delivered = None;
                loop {
                    let Some(owner) = owner.upgrade() else { break };
                    if owner.stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    let Some(cooler) = cooler.upgrade() else {
                        break;
                    };
                    let status = cooler.status();
                    if status["enabled"] == true && status["state"] == "running" {
                        let candidate = service.snapshot();
                        let key = candidate
                            .as_ref()
                            .map(|s| (status["pid"].as_u64(), s.generation, s.sequence));
                        let snapshot =
                            if owner.pause_cooler.load(Ordering::SeqCst) || key == delivered {
                                None
                            } else {
                                candidate
                            };
                        if let Ok(reply) = cooler.hardware_exchange(snapshot.clone()) {
                            if let Ok(demand) =
                                serde_json::from_value::<Demand>(reply["demand"].clone())
                            {
                                let _ = service.update_demand("cooler", demand);
                            }
                            if snapshot.is_some() {
                                delivered = key;
                            }
                        }
                    } else {
                        service.remove_demand("cooler");
                        delivered = None;
                    }
                    drop(owner);
                    thread::sleep(Duration::from_millis(125));
                }
                service.remove_demand("cooler");
            });
        });
        this.threads
            .lock_or_recover()
            .extend([dock_thread, cooler_thread]);
        this
    }
    pub fn pause_delivery(&self, consumer: &str, enabled: bool) -> Result<(), String> {
        match consumer {
            "dock" => self.pause_dock.store(enabled, Ordering::SeqCst),
            "cooler" => self.pause_cooler.store(enabled, Ordering::SeqCst),
            _ => return Err("Unknown hardware consumer".into()),
        }
        Ok(())
    }
    pub fn status(&self) -> Value {
        json!({"paused_delivery":{"dock":self.pause_dock.load(Ordering::SeqCst),"cooler":self.pause_cooler.load(Ordering::SeqCst)}})
    }
    pub fn shutdown(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        for thread in self.threads.lock_or_recover().drain(..) {
            let _ = thread.join();
        }
    }
}
