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
    pnp_sampled_at: Option<String>,
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
                status: json!({"state":"disabled","code":"diagnostics_off","message":"Connection diagnostics are off","lastSampleAt":null,"pnpSampledAt":null,"lastSuccessAt":null,"consecutiveFailures":0}),
                pnp: json!({"available":false}),
                pnp_sampled_at: None,
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
        inner.pnp = json!({"available":false});
        inner.pnp_sampled_at = None;
        inner.status = json!({"state":if enabled{"waiting"}else{"disabled"},"code":if enabled{"waiting_sample"}else{"diagnostics_off"},"message":if enabled{"Waiting for a connection sample"}else{"Connection diagnostics are off"},"lastSampleAt":null,"pnpSampledAt":null,"lastSuccessAt":inner.last_success,"consecutiveFailures":inner.failures});
    }
    pub fn status(&self) -> Value {
        self.inner.lock().unwrap().status.clone()
    }
    /// A completed engine transaction is evidence only, never a health update or
    /// a command. Serialize persistence with opt-out, and keep the same migration
    /// ownership gate as periodic health samples.
    pub fn record_trace(&self, value: &Value) {
        let _inner = self.inner.lock().unwrap();
        if !self.enabled()
            || !self
                .path
                .parent()
                .is_some_and(|parent| parent.join("migration.json").is_file())
        {
            return;
        }
        let Some(trace) = sanitize_trace(value) else {
            return;
        };
        let line =
            json!({"ts":super::super::device::log::iso_now(),"kind":"live_steps","trace":trace});
        let _ = append(&self.path, &line, LOG_LIMIT);
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
        let timestamp = super::super::device::log::iso_now();
        if let Some(pnp) = pnp {
            inner.pnp = sanitize_pnp(&pnp);
            inner.pnp_sampled_at = Some(timestamp.clone());
        }
        let worker = sanitize_worker(worker);
        let probe = match reply {
            Ok(value) => sanitize_probe(&value),
            Err(code) => json!({"sampleError":known_sample_error(code)}),
        };
        let (state, code, message, failure, success) = classify(&worker, &probe, &inner.pnp);
        if success {
            inner.last_success = Some(timestamp.clone());
        }
        inner.failures = if failure {
            inner.failures.saturating_add(1)
        } else {
            0
        };
        inner.status = json!({"state":state,"code":code,"message":message,"lastSampleAt":timestamp,"pnpSampledAt":inner.pnp_sampled_at,"lastSuccessAt":inner.last_success,"consecutiveFailures":inner.failures,
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
/// Reject the entire transaction when an unreviewed field or value appears.
/// In particular, arbitrary transport errors and firmware replies never become
/// diagnostic text. The timestamp is supplied by the host at persistence time.
fn sanitize_trace(value: &Value) -> Option<Value> {
    fn fields(value: &Value, names: &[&str]) -> bool {
        value.as_object().is_some_and(|object| {
            object.len() == names.len() && names.iter().all(|name| object.contains_key(*name))
        })
    }
    fn choice<'a>(value: &'a Value, choices: &[&str]) -> Option<&'a str> {
        value.as_str().filter(|text| choices.contains(text))
    }
    fn duration(value: &Value) -> Option<u64> {
        value.as_u64().filter(|millis| *millis <= 120_000)
    }
    if !fields(
        value,
        &["schema", "sequence", "operation", "totalMs", "steps"],
    ) || value["schema"].as_u64() != Some(1)
    {
        return None;
    }
    let sequence = value["sequence"].as_u64()?;
    let operation = choice(
        &value["operation"],
        &["connect", "stop", "stop_fast", "cancel_connect"],
    )?;
    let total_ms = duration(&value["totalMs"])?;
    let steps = value["steps"]
        .as_array()
        .filter(|steps| (1..=8).contains(&steps.len()))?;
    let mut sanitized = Vec::with_capacity(steps.len());
    for entry in steps {
        if !fields(entry, &["step", "result", "elapsedMs"]) {
            return None;
        }
        let step = choice(
            &entry["step"],
            &[
                "open",
                "end_stream",
                "info",
                "preserve_ending",
                "enter_live",
                "display_on",
                "loop_clip",
                "screen_off",
                "cancelled",
                "close",
            ],
        )?;
        let result = choice(
            &entry["result"],
            &[
                "ok",
                "cancelled",
                "not_found",
                "descriptor_failure",
                "busy",
                "no_reply",
                "write_timeout",
                "short_write",
                "transport_error",
            ],
        )?;
        let elapsed_ms = duration(&entry["elapsedMs"])?;
        sanitized.push(json!({"step":step,"result":result,"elapsedMs":elapsed_ms}));
    }
    Some(
        json!({"schema":1,"sequence":sequence,"operation":operation,"totalMs":total_ms,"steps":sanitized}),
    )
}
/// A second explicit boundary for agent-visible cached diagnostics. This reads
/// no source and exposes no payload, identifier, path, PID or arbitrary text.
pub fn agent_health(value: &Value) -> Value {
    let worker = sanitize_worker(&value["worker"]);
    let probe = sanitize_probe(&value["live"]);
    json!({
        "state":allowed(value["state"].as_str(), &["disabled","waiting","blocked","unresponsive","unavailable","off","paused","connected","failed","connecting"], "unavailable"),
        "code":allowed(value["code"].as_str(), &["diagnostics_off","waiting_sample","engine_disabled","ownership","worker_unavailable","sample_deferred","worker_timeout","live_off","intentional_hold","connected","descriptor_failure","connecting","not_found","busy","no_reply","write_timeout","short_write","transport_error"], "worker_unavailable"),
        "lastSampleAt":agent_timestamp(&value["lastSampleAt"]),
        "pnpSampledAt":agent_timestamp(&value["pnpSampledAt"]),
        "consecutiveFailures":value["consecutiveFailures"].as_u64().unwrap_or(0).min(9_007_199_254_740_991),
        "worker":{"enabled":worker["enabled"],"ready":worker["ready"],"state":worker["state"],
            "generation":worker["generation"].as_u64().unwrap_or(0).min(9_007_199_254_740_991),
            "restarts":worker["restarts"].as_u64().unwrap_or(0).min(9_007_199_254_740_991)},
        "live":{"running":probe["running"],"connected":probe["connected"],"paused":probe["paused"],
            "deviceHold":probe["deviceHold"],"systemHold":probe["systemHold"],"idle":probe["idle"],
            "connectionState":probe["connectionState"],"code":probe["code"],
            "sampleError":value["live"]["sampleError"].as_str().map(known_sample_error)
                .or_else(||value.get("live").filter(|v|v.is_object()).and_then(|_|probe["sampleError"].as_str().map(known_sample_error)))},
        "pnp":sanitize_pnp(&value["pnp"])
    })
}
fn agent_timestamp(value: &Value) -> Value {
    let Some(text) = value.as_str() else {
        return Value::Null;
    };
    let bytes = text.as_bytes();
    if bytes.len() != 24
        || !bytes.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            10 => *b == b'T',
            13 | 16 => *b == b':',
            19 => *b == b'.',
            23 => *b == b'Z',
            _ => b.is_ascii_digit(),
        })
    {
        return Value::Null;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().unwrap_or(0);
    let year = number(0..4);
    let month = number(5..7);
    let day = number(8..10);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        0,
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || day > days[month as usize]
        || number(11..13) > 23
        || number(14..16) > 59
        || number(17..19) > 59
    {
        return Value::Null;
    }
    json!(text)
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
    fn trace() -> Value {
        json!({"schema":1,"sequence":7,"operation":"connect","totalMs":710,
            "steps":[{"step":"open","result":"ok","elapsedMs":200},
                {"step":"end_stream","result":"write_timeout","elapsedMs":500},
                {"step":"close","result":"ok","elapsedMs":10}]})
    }
    #[test]
    fn trace_schema_preserves_finite_evidence_and_rejects_unreviewed_payloads() {
        let original = trace();
        assert_eq!(sanitize_trace(&original), Some(original.clone()));
        for operation in ["connect", "stop", "stop_fast", "cancel_connect"] {
            let mut value = original.clone();
            value["operation"] = json!(operation);
            value["totalMs"] = json!(120_000);
            value["steps"] = json!(vec![
                json!({"step":"close","result":"ok","elapsedMs":120_000});
                8
            ]);
            assert!(sanitize_trace(&value).is_some());
        }
        let secret = "seed-secret USB\\identifier C:\\private\\file";
        for field in ["message", "payload", "deviceId", "timestamp"] {
            let mut value = original.clone();
            value[field] = json!(secret);
            assert!(
                sanitize_trace(&value).is_none(),
                "reject root field {field}"
            );
            let mut value = original.clone();
            value["steps"][0][field] = json!(secret);
            assert!(
                sanitize_trace(&value).is_none(),
                "reject step field {field}"
            );
        }
        for (field, invalid) in [
            ("schema", json!(2)),
            ("schema", json!(1.0)),
            ("sequence", json!(-1)),
            ("sequence", json!(secret)),
            ("operation", json!(secret)),
            ("totalMs", json!(120_001)),
            ("totalMs", json!(-1)),
            ("totalMs", json!(0.5)),
            ("steps", json!([])),
            ("steps", json!(vec![original["steps"][0].clone(); 9])),
        ] {
            let mut value = original.clone();
            value[field] = invalid;
            assert!(sanitize_trace(&value).is_none(), "reject {field}");
        }
        for (field, invalid) in [
            ("step", json!(secret)),
            ("result", json!(secret)),
            ("elapsedMs", json!(120_001)),
            ("elapsedMs", json!(-1)),
            ("elapsedMs", json!(1.5)),
        ] {
            let mut value = original.clone();
            value["steps"][0][field] = invalid;
            assert!(sanitize_trace(&value).is_none(), "reject step {field}");
        }
        for field in ["schema", "sequence", "operation", "totalMs", "steps"] {
            let mut value = original.clone();
            value.as_object_mut().unwrap().remove(field);
            assert!(sanitize_trace(&value).is_none(), "require {field}");
        }
        for value in [Value::Null, json!([]), json!(secret)] {
            assert!(sanitize_trace(&value).is_none());
        }
    }
    #[test]
    fn trace_logging_obeys_opt_out_migration_and_does_not_change_cached_health() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("devices/cooler");
        let path = home.join("connection-health.jsonl");
        let d = Diagnostics::new(path.clone());
        d.record_trace(&trace());
        assert!(!home.exists());
        d.set_enabled(true);
        d.record_trace(&trace());
        assert!(
            !home.exists(),
            "trace must not create an unowned migration target"
        );
        std::fs::create_dir_all(&home).unwrap();
        d.record_trace(&trace());
        assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
        std::fs::write(home.join("migration.json"), b"{}").unwrap();
        let before = d.status();
        d.record_trace(&trace());
        assert_eq!(
            d.status(),
            before,
            "step evidence is not a current health sample"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let line: Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(line["kind"], "live_steps");
        assert_eq!(line["trace"], trace());
        assert!(!agent_timestamp(&line["ts"]).is_null());
        let mut invalid = trace();
        invalid["steps"][0]["result"] = json!("seed-secret");
        d.record_trace(&invalid);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        d.set_enabled(false);
        d.record_trace(&trace());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(d.status()["code"], "diagnostics_off");
    }
    #[test]
    fn trace_records_share_bounded_health_log_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("connection-health.jsonl");
        std::fs::write(dir.path().join("migration.json"), b"{}").unwrap();
        std::fs::write(&path, vec![b' '; LOG_LIMIT as usize - 1]).unwrap();
        std::fs::write(path.with_extension("jsonl.old"), b"older").unwrap();
        let d = Diagnostics::new(path.clone());
        d.set_enabled(true);
        d.record_trace(&trace());
        assert_eq!(
            std::fs::metadata(path.with_extension("jsonl.old"))
                .unwrap()
                .len(),
            LOG_LIMIT - 1
        );
        assert!(std::fs::metadata(&path).unwrap().len() < LOG_LIMIT);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
        let line: Value =
            serde_json::from_str(std::fs::read_to_string(path).unwrap().trim()).unwrap();
        assert_eq!(line["kind"], "live_steps");
    }
    #[test]
    fn agent_projection_is_finite_bounded_and_redacts_every_nested_payload() {
        let secret = "seed-secret USB\\identifier C:\\private\\file";
        let value = json!({"state":secret,"code":secret,"message":secret,"pid":123,
            "lastSampleAt":secret,"pnpSampledAt":"2026-02-30T00:00:00.000Z","consecutiveFailures":u64::MAX,
            "worker":{"state":secret,"ready":true,"generation":u64::MAX,"restarts":u64::MAX,"pid":123,"message":secret},
            "live":{"connectionState":secret,"code":secret,"sampleError":secret,"message":secret,"rxBytes":123,"payload":{"secret":secret}},
            "pnp":{"available":true,"normalNodes":900,"problemCodes":[43,-1,4096,secret,0],"exactMatchedDescriptorCount":900,"identifiers":[secret]}});
        let projected = agent_health(&value);
        let text = projected.to_string();
        for forbidden in [
            "seed-secret",
            "identifier",
            "private",
            "message",
            "pid",
            "payload",
            "rxBytes",
        ] {
            assert!(!text.contains(forbidden), "redact {forbidden}");
        }
        assert_eq!(projected["state"], "unavailable");
        assert_eq!(projected["code"], "worker_unavailable");
        assert_eq!(projected["worker"]["state"], "error");
        assert_eq!(projected["live"]["connectionState"], "error");
        assert_eq!(projected["live"]["code"], "transport_error");
        assert_eq!(projected["live"]["sampleError"], "worker_unavailable");
        assert!(projected["lastSampleAt"].is_null());
        assert!(projected["pnpSampledAt"].is_null());
        assert_eq!(projected["consecutiveFailures"], 9_007_199_254_740_991u64);
        assert_eq!(projected["worker"]["generation"], 9_007_199_254_740_991u64);
        assert_eq!(projected["pnp"]["normalNodes"], 64);
        assert_eq!(projected["pnp"]["exactMatchedDescriptorCount"], 64);
        assert_eq!(projected["pnp"]["problemCodes"], json!([43, 0]));
        let many = agent_health(&json!({"pnp":{"problemCodes":vec![43;100]}}));
        assert_eq!(many["pnp"]["problemCodes"].as_array().unwrap().len(), 64);
    }
    #[test]
    fn agent_probe_errors_preserve_source_failure_and_flag_missing_reply_shape() {
        assert!(agent_health(&json!({"code":"diagnostics_off"}))["live"]["sampleError"].is_null());
        assert_eq!(
            agent_health(&json!({"live":{}}))["live"]["sampleError"],
            "invalid_reply"
        );
        assert_eq!(
            agent_health(&json!({"live":{"sampleError":"worker_timeout"}}))["live"]["sampleError"],
            "worker_timeout"
        );
        assert!(
            agent_health(&json!({"live":{"connectionState":"connected","code":"connected"}}))
                ["live"]["sampleError"]
                .is_null()
        );
    }
    #[test]
    fn agent_timestamp_accepts_only_calendar_valid_host_shape() {
        for valid in ["2026-11-02T02:00:00.123Z", "2024-02-29T23:59:59.999Z"] {
            assert_eq!(agent_timestamp(&json!(valid)), valid);
        }
        for invalid in [
            "2026-02-29T00:00:00.000Z",
            "2026-13-01T00:00:00.000Z",
            "2026-11-00T00:00:00.000Z",
            "2026-11-01T24:00:00.000Z",
            "2026-11-01T00:60:00.000Z",
            "2026-11-01T00:00:60.000Z",
            "0000-01-01T00:00:00.000Z",
            "2026-11-01T00:00:00.000Z secret",
        ] {
            assert!(agent_timestamp(&json!(invalid)).is_null());
        }
        for invalid in [Value::Null, json!(0), json!({"date":"secret"})] {
            assert!(agent_timestamp(&invalid).is_null());
        }
    }
    #[test]
    fn pnp_timestamp_changes_only_on_arrived_samples_and_toggle_discards_inflight() {
        let dir = tempfile::tempdir().unwrap();
        let d = Diagnostics::new(dir.path().join("not-created/health.jsonl"));
        assert!(d.status()["pnpSampledAt"].is_null());
        assert_eq!(agent_health(&d.status())["code"], "diagnostics_off");
        d.set_enabled(true);
        assert_eq!(agent_health(&d.status())["code"], "waiting_sample");
        assert!(d.status()["pnpSampledAt"].is_null());
        let now = Instant::now();
        let ticket = d.begin(now).unwrap();
        d.finish(ticket, &worker(), Ok(probe()), None, now);
        assert!(d.status()["pnpSampledAt"].is_null());
        let ticket = d.begin(now + Duration::from_secs(10)).unwrap();
        d.finish(
            ticket,
            &worker(),
            Ok(probe()),
            Some(pnp_snapshot(true)),
            now,
        );
        let first = d.status()["pnpSampledAt"].clone();
        assert!(!agent_timestamp(&first).is_null());
        let ticket = d.begin(now + Duration::from_secs(20)).unwrap();
        d.finish(ticket, &worker(), Err("sample_deferred"), None, now);
        assert_eq!(d.status()["pnpSampledAt"], first);
        let ticket = d.begin(now + Duration::from_secs(30)).unwrap();
        d.set_enabled(false);
        d.finish(
            ticket,
            &worker(),
            Ok(probe()),
            Some(pnp_snapshot(true)),
            now,
        );
        assert!(d.status()["pnpSampledAt"].is_null());
        assert_eq!(agent_health(&d.status())["code"], "diagnostics_off");
        assert!(!dir.path().join("not-created").exists());
    }
    #[test]
    fn cached_agent_codes_distinguish_worker_live_and_enumeration_failures() {
        let w = worker();
        let pnp = pnp_snapshot(true);
        for (probe, expected) in [
            (json!({"sampleError":"worker_timeout"}), "worker_timeout"),
            (json!({"sampleError":"sample_deferred"}), "sample_deferred"),
            (json!({"running":false}), "live_off"),
            (
                json!({"running":true,"systemHold":true}),
                "intentional_hold",
            ),
            (json!({"running":true,"connected":true}), "connected"),
            (
                json!({"running":true,"code":"descriptor_failure"}),
                "descriptor_failure",
            ),
        ] {
            let (state, code, _, _, _) = classify(&w, &probe, &pnp);
            let projected =
                agent_health(&json!({"state":state,"code":code,"worker":w,"live":probe,"pnp":pnp}));
            assert_eq!(projected["code"], expected);
        }
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
