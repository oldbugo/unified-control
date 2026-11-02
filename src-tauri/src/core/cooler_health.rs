//! Opt-out local diagnostics. Whitelisted counters only; never device commands,
//! identifiers, media paths, account data, raw RPC payloads or arbitrary errors.
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};

const LOG_LIMIT: u64 = 2 * 1024 * 1024;
pub struct Ticket {
    epoch: u64,
    pub sample_pnp: bool,
}
struct Inner {
    next_sample: Instant,
    next_pnp: Instant,
    last_log: Option<Instant>,
    signature: String,
    status: Value,
    pnp: Value,
    last_success: Option<String>,
    failures: u64,
}
pub struct Diagnostics {
    enabled: AtomicBool,
    epoch: AtomicU64,
    inner: Mutex<Inner>,
    path: PathBuf,
}
impl Diagnostics {
    pub fn new(path: PathBuf) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            inner: Mutex::new(Inner {
                next_sample: Instant::now(),
                next_pnp: Instant::now(),
                last_log: None,
                signature: String::new(),
                status: json!({"state":"disabled","code":"diagnostics_off","message":"Connection diagnostics are off","lastSampleAt":null,"lastSuccessAt":null,"consecutiveFailures":0}),
                pnp: json!({"available":false}),
                last_success: None,
                failures: 0,
            }),
            path,
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }
    pub fn set_enabled(&self, enabled: bool) {
        let mut inner = self.inner.lock().unwrap();
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.enabled.store(enabled, Ordering::SeqCst);
        inner.next_sample = Instant::now();
        inner.next_pnp = Instant::now();
        inner.signature.clear();
        inner.status = json!({"state":if enabled{"waiting"}else{"disabled"},"code":if enabled{"waiting_sample"}else{"diagnostics_off"},"message":if enabled{"Waiting for a connection sample"}else{"Connection diagnostics are off"},"lastSampleAt":null,"lastSuccessAt":inner.last_success,"consecutiveFailures":inner.failures});
    }
    pub fn status(&self) -> Value {
        self.inner.lock().unwrap().status.clone()
    }
    pub fn begin(&self, now: Instant) -> Option<Ticket> {
        if !self.enabled() {
            return None;
        }
        let mut inner = self.inner.lock().unwrap();
        if !self.enabled() || now < inner.next_sample {
            return None;
        }
        inner.next_sample = now + Duration::from_secs(10);
        let sample_pnp = now >= inner.next_pnp;
        if sample_pnp {
            inner.next_pnp = now + Duration::from_secs(30);
        }
        Some(Ticket {
            epoch: self.epoch.load(Ordering::SeqCst),
            sample_pnp,
        })
    }
    pub fn finish(
        &self,
        ticket: Ticket,
        worker: &Value,
        reply: Result<Value, &str>,
        pnp: Option<Value>,
        now: Instant,
    ) {
        let mut inner = self.inner.lock().unwrap();
        if !self.enabled() || ticket.epoch != self.epoch.load(Ordering::SeqCst) {
            return;
        }
        if let Some(pnp) = pnp {
            inner.pnp = sanitize_pnp(&pnp);
        }
        let worker = sanitize_worker(worker);
        let probe = match reply {
            Ok(value) => sanitize_probe(&value),
            Err(code) => json!({"sampleError":known_sample_error(code)}),
        };
        let (state, code, message, failure, success) = classify(&worker, &probe, &inner.pnp);
        let timestamp = super::super::device::log::iso_now();
        if success {
            inner.last_success = Some(timestamp.clone());
        }
        inner.failures = if failure {
            inner.failures.saturating_add(1)
        } else {
            0
        };
        inner.status = json!({"state":state,"code":code,"message":message,"lastSampleAt":timestamp,"lastSuccessAt":inner.last_success,"consecutiveFailures":inner.failures,
            "worker":worker,"live":probe,"pnp":inner.pnp});
        // Attempt/retry counters are useful in samples, but are not transitions.
        let signature = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            state,
            code,
            worker["generation"],
            probe["systemHold"],
            probe["paused"],
            inner.pnp["normalNodes"],
            inner.pnp["exactMatchedDescriptorCount"]
        );
        let changed = signature != inner.signature;
        let due = inner
            .last_log
            .is_none_or(|last| now.saturating_duration_since(last) >= Duration::from_secs(60));
        // Before first enable, health stays in memory. Only the completed
        // migration marker authorizes disk logging inside the worker folder.
        if (changed || due)
            && self
                .path
                .parent()
                .is_some_and(|parent| parent.join("migration.json").is_file())
        {
            let line = json!({"ts":timestamp,"kind":if changed{"transition"}else{"snapshot"},"health":inner.status});
            if append(&self.path, &line, LOG_LIMIT).is_ok() {
                inner.signature = signature;
                inner.last_log = Some(now);
            }
        }
    }
}
fn append(path: &std::path::Path, value: &Value, limit: u64) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.len().saturating_add(bytes.len() as u64) > limit)
    {
        let old = path.with_extension("jsonl.old");
        if old.exists() {
            std::fs::remove_file(&old)?;
        }
        std::fs::rename(path, old)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&bytes)
}
fn allowed<'a>(value: Option<&'a str>, choices: &[&str], fallback: &'a str) -> &'a str {
    value.filter(|v| choices.contains(v)).unwrap_or(fallback)
}
fn known_sample_error(code: &str) -> &str {
    allowed(
        Some(code),
        &[
            "worker_timeout",
            "worker_unavailable",
            "sample_deferred",
            "invalid_reply",
            "worker_disconnected",
        ],
        "worker_unavailable",
    )
}
fn sanitize_worker(value: &Value) -> Value {
    json!({"enabled":value["enabled"]==true,"ready":value["ready"]==true,
        "state":allowed(value["state"].as_str(),&["disabled","starting","blocked","running","reconnecting","stopping","unavailable","error"],"error"),
        "generation":value["generation"].as_u64().unwrap_or(0),"restarts":value["restarts"].as_u64().unwrap_or(0)})
}
pub fn sanitize_probe(value: &Value) -> Value {
    let mut result = json!({
        "running":value["running"]==true,"connected":value["connected"]==true,"paused":value["paused"]==true,
        "deviceHold":value["deviceHold"]==true,"systemHold":value["systemHold"]==true,"idle":value["idle"]==true,
        "connectionState":allowed(value["connectionState"].as_str(),&["off","paused","connected","connecting","notFound","failedUsb","busy","noReply","error"],"error"),
        "code":allowed(value["code"].as_str(),&["off","paused","connected","connecting","not_found","descriptor_failure","busy","no_reply","write_timeout","short_write","transport_error"],"transport_error"),
        "stage":allowed(value["stage"].as_str(),&["off","hold","connecting","streaming"],"connecting"),
        "attempts":value["attempts"].as_u64().unwrap_or(0),"retryInSeconds":value["retryInSeconds"].as_u64().unwrap_or(0).min(60),
        "rxBytes":value["rxBytes"].as_u64().unwrap_or(0),"rxTransfers":value["rxTransfers"].as_u64().unwrap_or(0),"statusFrames":value["statusFrames"].as_u64().unwrap_or(0),
        "measuredFps":value["measuredFps"].as_f64().filter(|v|v.is_finite()&&*v>=0.0&&*v<=1000.0).unwrap_or(0.0),
    });
    if value
        .get("connectionState")
        .and_then(Value::as_str)
        .is_none()
    {
        result["sampleError"] = json!("invalid_reply");
    }
    result
}
fn sanitize_pnp(value: &Value) -> Value {
    json!({"available":value["available"]==true,"simulated":value["simulated"]==true,
        "normalNodes":value["normalNodes"].as_u64().unwrap_or(0).min(64),
        "problemCodes":value["problemCodes"].as_array().map(|a|a.iter().filter_map(Value::as_u64).filter(|n|*n<=4095).take(64).collect::<Vec<_>>()).unwrap_or_default(),
        "exactMatchedDescriptorCount":value["exactMatchedDescriptorCount"].as_u64().unwrap_or(0).min(64)})
}
fn classify<'a>(
    worker: &Value,
    probe: &'a Value,
    pnp: &Value,
) -> (&'static str, &'a str, &'static str, bool, bool) {
    if worker["enabled"] != true {
        return (
            "disabled",
            "engine_disabled",
            "Cooler engine is disabled",
            false,
            false,
        );
    }
    if worker["state"] == "blocked" {
        return (
            "blocked",
            "ownership",
            "Another cooler application owns the screen",
            true,
            false,
        );
    }
    if worker["ready"] != true {
        return (
            "waiting",
            "worker_unavailable",
            "Waiting for the cooler engine",
            false,
            false,
        );
    }
    if let Some(error) = probe["sampleError"].as_str() {
        return match error {
            "sample_deferred" => (
                "waiting",
                "sample_deferred",
                "Connection sample deferred while commands are being sent",
                false,
                false,
            ),
            "worker_timeout" => (
                "unresponsive",
                "worker_timeout",
                "Cooler engine did not answer its health request",
                true,
                false,
            ),
            _ => (
                "unavailable",
                "worker_unavailable",
                "Cooler health information is unavailable",
                true,
                false,
            ),
        };
    }
    if probe["running"] != true {
        return (
            "off",
            "live_off",
            "Live is off; no live connection is expected",
            false,
            false,
        );
    }
    if probe["paused"] == true
        || probe["deviceHold"] == true
        || probe["systemHold"] == true
        || probe["idle"] == true
    {
        return (
            "paused",
            "intentional_hold",
            "Live is intentionally paused",
            false,
            false,
        );
    }
    if probe["connected"] == true {
        return (
            "connected",
            "connected",
            "Live reports a verified connection",
            false,
            true,
        );
    }
    if pnp["available"] == true
        && pnp["normalNodes"] == 0
        && pnp["exactMatchedDescriptorCount"].as_u64().unwrap_or(0) > 0
    {
        return (
            "failed",
            "descriptor_failure",
            "Windows reports a matched cooler descriptor failure",
            true,
            false,
        );
    }
    match probe["code"].as_str().unwrap_or("transport_error") {
        "connecting" => (
            "connecting",
            "connecting",
            "Live is connecting",
            false,
            false,
        ),
        "not_found" => (
            "failed",
            "not_found",
            "Windows did not expose the cooler transport",
            true,
            false,
        ),
        "descriptor_failure" => (
            "failed",
            "descriptor_failure",
            "Windows reports a cooler USB enumeration failure",
            true,
            false,
        ),
        "busy" => (
            "blocked",
            "busy",
            "The cooler transport is in use",
            true,
            false,
        ),
        "no_reply" => (
            "failed",
            "no_reply",
            "The cooler did not answer the firmware handshake",
            true,
            false,
        ),
        "write_timeout" => (
            "failed",
            "write_timeout",
            "The cooler USB write timed out",
            true,
            false,
        ),
        "short_write" => (
            "failed",
            "short_write",
            "The cooler accepted only part of a USB transfer",
            true,
            false,
        ),
        _ => (
            "failed",
            "transport_error",
            "The cooler transport reported an error",
            true,
            false,
        ),
    }
}

/// Read-only Windows Configuration Manager calls. No device handle, write, reset,
/// elevation or automatic recovery. Identifiers exist only in this stack frame.
pub fn pnp_snapshot(fake: bool) -> Value {
    if fake {
        return json!({"available":true,"simulated":true,"normalNodes":2,"problemCodes":[0,0],"exactMatchedDescriptorCount":0});
    }
    #[cfg(windows)]
    {
        native_pnp::snapshot().unwrap_or_else(|| json!({"available":false}))
    }
    #[cfg(not(windows))]
    {
        json!({"available":false})
    }
}
#[cfg(windows)]
mod native_pnp {
    use super::*;
    use std::collections::HashSet;
    use windows_sys::Win32::Devices::{
        DeviceAndDriverInstallation::*,
        Properties::{DEVPKEY_Device_LocationInfo, DEVPKEY_Device_Parent, DEVPROP_TYPE_STRING},
    };
    use windows_sys::Win32::Foundation::DEVPROPKEY;
    fn ids(present: bool) -> Option<Vec<String>> {
        let flags = if present {
            CM_GETIDLIST_FILTER_PRESENT
        } else {
            0
        };
        for _ in 0..3 {
            unsafe {
                let mut len = 0;
                if CM_Get_Device_ID_List_SizeW(&mut len, std::ptr::null(), flags) != CR_SUCCESS
                    || len == 0
                    || len > 2_000_000
                {
                    return None;
                }
                let mut buffer = vec![0u16; len as usize];
                let result =
                    CM_Get_Device_ID_ListW(std::ptr::null(), buffer.as_mut_ptr(), len, flags);
                if result == CR_BUFFER_SMALL {
                    continue;
                }
                if result != CR_SUCCESS {
                    return None;
                }
                return Some(
                    buffer
                        .split(|c| *c == 0)
                        .filter(|v| !v.is_empty())
                        .map(String::from_utf16_lossy)
                        .map(|s| s.to_ascii_uppercase())
                        .collect(),
                );
            }
        }
        None
    }
    fn locate(id: &str, phantom: bool) -> Option<u32> {
        let id: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        let mut dev = 0;
        (unsafe {
            CM_Locate_DevNodeW(
                &mut dev,
                id.as_ptr(),
                if phantom {
                    CM_LOCATE_DEVNODE_PHANTOM
                } else {
                    0
                },
            )
        } == CR_SUCCESS)
            .then_some(dev)
    }
    fn property(dev: u32, key: &DEVPROPKEY) -> Option<String> {
        unsafe {
            let mut buffer = vec![0u16; 2048];
            let mut len = (buffer.len() * 2) as u32;
            let mut kind = 0;
            if CM_Get_DevNode_PropertyW(
                dev,
                key,
                &mut kind,
                buffer.as_mut_ptr().cast(),
                &mut len,
                0,
            ) != CR_SUCCESS
                || kind != DEVPROP_TYPE_STRING
                || len == 0
                || len as usize > buffer.len() * 2
                || len % 2 != 0
            {
                return None;
            }
            let text = String::from_utf16_lossy(&buffer[..len as usize / 2]);
            let text = text.trim_end_matches('\0').to_ascii_uppercase();
            (!text.is_empty()).then_some(text)
        }
    }
    fn pair(dev: u32) -> Option<(String, String)> {
        Some((
            property(dev, &DEVPKEY_Device_Parent)?,
            property(dev, &DEVPKEY_Device_LocationInfo)?,
        ))
    }
    fn known(id: &str) -> bool {
        ["USB\\VID_33C3&PID_7788", "USB\\VID_33C3&PID_7791"]
            .iter()
            .any(|prefix| {
                id.strip_prefix(prefix)
                    .is_some_and(|tail| tail.starts_with('\\') || tail.starts_with('&'))
            })
    }
    pub fn snapshot() -> Option<Value> {
        let all = ids(false)?;
        let present = ids(true)?.into_iter().collect::<HashSet<_>>();
        let mut history = HashSet::new();
        let mut problems = vec![];
        let mut normal = 0u64;
        for id in all.iter().filter(|id| known(id)) {
            if let Some(dev) = locate(id, true) {
                if let Some(pair) = pair(dev) {
                    history.insert(pair);
                }
                if present.contains(id) {
                    normal += 1;
                    let (mut status, mut problem) = (0, 0);
                    if unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, dev, 0) }
                        == CR_SUCCESS
                    {
                        problems.push(problem);
                    }
                }
            }
        }
        let mut matched = 0u64;
        for id in present
            .iter()
            .filter(|id| id.starts_with("USB\\VID_0000&PID_000"))
        {
            if let Some(dev) = locate(id, false) {
                if pair(dev).is_some_and(|pair| history.contains(&pair)) {
                    matched += 1;
                    let (mut status, mut problem) = (0, 0);
                    if unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, dev, 0) }
                        == CR_SUCCESS
                    {
                        problems.push(problem);
                    }
                }
            }
        }
        Some(
            json!({"available":true,"simulated":false,"normalNodes":normal,"problemCodes":problems,"exactMatchedDescriptorCount":matched}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn worker() -> Value {
        json!({"enabled":true,"ready":true,"state":"running","generation":2,"restarts":1})
    }
    fn probe() -> Value {
        json!({"running":true,"connected":false,"connectionState":"error","code":"write_timeout","stage":"connecting","attempts":20})
    }
    #[test]
    fn distinguishes_timeout_enumeration_and_intentional_holds() {
        let w = worker();
        let mut p = probe();
        let mut pnp = pnp_snapshot(true);
        assert_eq!(classify(&w, &p, &pnp).1, "write_timeout");
        pnp["normalNodes"] = json!(0);
        pnp["exactMatchedDescriptorCount"] = json!(1);
        assert_eq!(classify(&w, &p, &pnp).1, "descriptor_failure");
        p["systemHold"] = json!(true);
        assert_eq!(classify(&w, &p, &pnp).1, "intentional_hold");
        p["running"] = json!(false);
        assert_eq!(classify(&w, &p, &pnp).1, "live_off");
        assert_eq!(
            classify(&w, &json!({"sampleError":"worker_timeout"}), &pnp).1,
            "worker_timeout"
        );
    }
    #[test]
    fn logs_only_whitelist_and_disable_discards_inflight_samples() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("connection-health.jsonl");
        std::fs::write(dir.path().join("migration.json"), b"{}").unwrap();
        let d = Diagnostics::new(log.clone());
        assert!(d.begin(Instant::now()).is_none());
        d.set_enabled(true);
        let now = Instant::now();
        let t = d.begin(now).unwrap();
        let mut p = probe();
        p["message"] = json!("seed-account-secret USB\\identifier C:\\private\\file");
        p["extra"] = json!({"tokens":"secret"});
        d.finish(t, &worker(), Ok(p), Some(pnp_snapshot(true)), now);
        let text = std::fs::read_to_string(&log).unwrap();
        for forbidden in ["seed-account", "identifier", "private", "tokens", "secret"] {
            assert!(!text.contains(forbidden));
        }
        assert_eq!(d.status()["consecutiveFailures"], 1);
        assert!(d.begin(now + Duration::from_secs(9)).is_none());
        let t = d.begin(now + Duration::from_secs(10)).unwrap();
        d.set_enabled(false);
        d.finish(
            t,
            &worker(),
            Ok(probe()),
            None,
            now + Duration::from_secs(10),
        );
        assert_eq!(std::fs::read_to_string(&log).unwrap(), text);
        assert_eq!(d.status()["code"], "diagnostics_off");
    }
    #[test]
    fn rotation_is_bounded_to_two_files_and_fake_probe_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("health.jsonl");
        for n in 0..20 {
            append(&path, &json!({"n":n,"data":"0123456789"}), 100).unwrap();
        }
        assert!(std::fs::metadata(&path).unwrap().len() <= 100);
        assert!(
            std::fs::metadata(path.with_extension("jsonl.old"))
                .unwrap()
                .len()
                <= 100
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        assert_eq!(pnp_snapshot(true), pnp_snapshot(true));
    }
    #[test]
    fn diagnostics_does_not_create_the_migration_target() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("devices/cooler");
        let d = Diagnostics::new(home.join("connection-health.jsonl"));
        d.set_enabled(true);
        let now = Instant::now();
        let ticket = d.begin(now).unwrap();
        d.finish(
            ticket,
            &json!({"enabled":false,"state":"disabled","ready":false}),
            Err("worker_unavailable"),
            Some(pnp_snapshot(true)),
            now,
        );
        assert!(!home.exists());
        assert_eq!(d.status()["code"], "engine_disabled");
        std::fs::create_dir_all(&home).unwrap();
        d.set_enabled(true);
        let now = Instant::now();
        let ticket = d.begin(now).unwrap();
        d.finish(
            ticket,
            &json!({"enabled":false,"state":"disabled","ready":false}),
            Err("worker_unavailable"),
            Some(pnp_snapshot(true)),
            now,
        );
        assert_eq!(
            std::fs::read_dir(&home).unwrap().count(),
            0,
            "an existing empty migration target must stay empty"
        );
    }
}
