//! Where Unified Control keeps its files. Copied from CoolerScreen's `core/paths.rs` with its own folder and env var; the
//! cooler-only locations (library, gallery, sources, ffmpeg, driver scripts) are not here.
use std::path::PathBuf;

/// %APPDATA%\UnifiedControl (or UC_HOME).
pub fn base() -> PathBuf {
    if let Some(h) = std::env::var_os("UC_HOME") {
        return PathBuf::from(h);
    }
    // Unit tests never touch the real app folder (device.log, settings.json).
    if cfg!(test) {
        return std::env::temp_dir().join("uc-core-unit-tests");
    }
    let appdata = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"));
    appdata.join("UnifiedControl")
}

pub fn settings() -> PathBuf {
    base().join("settings.json")
}
