//! Append-only diagnostic log (copied from CoolerScreen's `device/log.rs`):
//! `%APPDATA%\UnifiedControl\device.log`, `<ISO-8601 UTC with ms> <message>`, rotated to .old at ~1 MB.
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn base_dir() -> PathBuf {
    if let Some(h) = std::env::var_os("UC_HOME") {
        return PathBuf::from(h);
    }
    if cfg!(test) {
        return std::env::temp_dir().join("uc-core-unit-tests");
    }
    if let Some(a) = std::env::var_os("APPDATA") {
        return PathBuf::from(a).join("UnifiedControl");
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(home).join(".config").join("UnifiedControl")
}

/// Same as JavaScript's `new Date().toISOString()`.
pub fn iso_now() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let ms = d.subsec_millis();
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil_from_days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

pub fn log(msg: &str) {
    let dir = base_dir();
    let file = dir.join("device.log");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(m) = std::fs::metadata(&file) {
        if m.len() > 1_000_000 {
            let _ = std::fs::rename(&file, dir.join("device.log.old"));
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(f, "{} {}", iso_now(), msg);
    }
    if std::env::var_os("UC_VERBOSE").is_some() {
        eprintln!("[device.log] {msg}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn iso_shape() {
        let s = super::iso_now();
        assert_eq!(s.len(), 24);
        assert!(s.starts_with("20") && s.ends_with('Z') && &s[10..11] == "T");
    }
}
