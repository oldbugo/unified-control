//! Which build this is: git short hash (a "+" when built with uncommitted changes), commit date and build time
//! (embedded by build.rs; "unknown" when git was not available). Copied from CoolerScreen's `core/buildinfo.rs`.
pub const HASH: &str = env!("CS_GIT_HASH");
pub const COMMIT_DATE: &str = env!("CS_GIT_DATE");
/// Seconds since 1970 when the build script ran.
pub fn built_at() -> u64 {
    env!("CS_BUILT_AT").parse().unwrap_or(0)
}
pub fn title(version: &str) -> String {
    format!("Unified Control {version} ({HASH}){}", screen_note())
}
/// `UC_FAKE_DEVICE`: the fake devices are used instead of real hardware (tests). Phase 0 has no device code at all, so this
/// is only reported; the dock page (phase 1) must honour it and never open the real dock when it is set.
pub fn fake_device() -> Option<String> {
    std::env::var("UC_FAKE_DEVICE").ok().filter(|v| !v.is_empty() && v != "0")
}
/// Debug builds only: which devices this process uses, for the window title and the log (release builds: nothing).
pub fn screen_note() -> String {
    if !cfg!(debug_assertions) {
        return String::new();
    }
    match fake_device() {
        Some(m) => format!(" (debug, fake devices: {m})"),
        None => " (debug, REAL devices)".into(),
    }
}
/// First line of device.log at start-up, to match what the user runs.
pub fn startup_line(version: &str) -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    format!("Unified Control {version} build {HASH} started, pid {}, exe {exe}{}", std::process::id(), screen_note())
}
