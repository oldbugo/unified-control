//! Small helpers that reproduce JavaScript number semantics, so files and keys written by 2.0 are byte-compatible
//! with 1.2 (`45` not `45.0`, `Math.round`, `Number(v)` coercion, `${n}` formatting).
use serde_json::{Map, Value};
use std::path::Path;

/// `Math.round`: halves round up (towards +infinity).
pub fn round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `Math.round(v * 1000) / 1000`
pub fn r3(x: f64) -> f64 {
    round(x * 1000.0) / 1000.0
}

/// A JSON number the way JSON.stringify writes it: integral values without ".0".
pub fn num(x: f64) -> Value {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 9.0e15 {
        Value::from(x as i64)
    } else if x.is_finite() {
        Value::from(x)
    } else {
        Value::Null
    }
}

/// `${n}` for a number.
pub fn fmt(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e21 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// `Number(v)` (NaN for anything that does not coerce).
pub fn to_num(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        Some(Value::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::Null) => 0.0,
        None => f64::NAN,
        _ => f64::NAN,
    }
}

/// 1.2 settings `num(v, d, lo, hi)`: default when not finite, else clamped.
pub fn clamp_num(v: Option<&Value>, d: f64, lo: f64, hi: f64) -> f64 {
    let x = to_num(v);
    if !x.is_finite() {
        return d;
    }
    x.max(lo).min(hi)
}

/// JS truthiness.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}

pub fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

pub fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

/// `{ ...a, ...b }` for objects.
pub fn merge(a: &Value, b: &Value) -> Value {
    let mut m = a.as_object().cloned().unwrap_or_default();
    if let Some(bm) = b.as_object() {
        for (k, v) in bm {
            m.insert(k.clone(), v.clone());
        }
    }
    Value::Object(m)
}

pub fn obj() -> Map<String, Value> {
    Map::new()
}

/// `JSON.stringify(v, null, 2)` (serde_json's pretty printer uses the same two-space layout).
pub fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".into())
}

/// Read a file, retrying briefly while a writer is replacing it (Windows sharing violations during the rename).
pub fn read_text(p: &Path) -> std::io::Result<String> {
    let mut tries = 0;
    loop {
        match std::fs::read_to_string(p) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && tries < 20 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            r => return r,
        }
    }
}

pub fn read_json(p: &Path) -> Option<Value> {
    let s = read_text(p).ok()?;
    serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()
}

/// Write `data` atomically: a uniquely named temp file next to `p`, then rename over `p` (retried while a reader has
/// `p` open). Readers see the old or the new content, never a torn file; concurrent writers never share a temp file.
pub fn write_atomic(p: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let tmp = {
        let mut s = p.as_os_str().to_owned();
        s.push(format!(".{}-{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::path::PathBuf::from(s)
    };
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    let mut tries = 0;
    loop {
        match std::fs::rename(&tmp, p) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && tries < 50 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
}

/// JSON.stringify(v, null, 2), written atomically (as 1.2's writeJson, without its shared temp name).
pub fn write_json(p: &Path, v: &Value) -> std::io::Result<()> {
    write_atomic(p, pretty(v).as_bytes())
}

/// ISO-8601 UTC timestamp with milliseconds (`new Date().toISOString()`).
pub fn iso_now() -> String {
    iso(super::scheduler::now_ms())
}

pub fn iso(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // civil from days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{:03}Z", ms % 1000)
}

/// `Date.parse` for the ISO strings we write (UTC, optional milliseconds). None if unparsable.
pub fn parse_iso(t: &str) -> Option<u64> {
    let b = t.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let n = |a: usize, l: usize| t.get(a..a + l)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, s) = (n(0, 4)?, n(5, 2)?, n(8, 2)?, n(11, 2)?, n(14, 2)?, n(17, 2)?);
    let ms = if b.get(19) == Some(&b'.') {
        let frac: String = t[20..].chars().take_while(|c| c.is_ascii_digit()).collect();
        let f3 = format!("{frac:0<3}");
        f3[..3].parse::<i64>().ok()?
    } else {
        0
    };
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let total = ((days * 86400 + h * 3600 + mi * 60 + s) * 1000 + ms) as u64;
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_like_js() {
        assert_eq!(num(45.0), json!(45));
        assert_eq!(num(0.5), json!(0.5));
        assert_eq!(fmt(92.0), "92");
        assert_eq!(fmt(2.25), "2.25");
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(r3(1.23456), 1.235);
        assert_eq!(to_num(Some(&json!("12"))), 12.0);
        assert!(to_num(Some(&json!("x"))).is_nan());
    }

    #[test]
    fn iso_round_trip() {
        let ms = 1_791_712_345_678u64;
        let s = iso(ms);
        assert_eq!(parse_iso(&s), Some(ms));
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(parse_iso("2026-10-11T04:49:20.028Z"), Some(1_791_694_160_028));
    }
}
