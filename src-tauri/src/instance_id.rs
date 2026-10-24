//! Who may join whom: the single-instance lock (and the `--quit` event) belong to one *kind of build in one app folder*.
//!   * the installed release app with the default app folder keeps the plain identifier from tauri.conf.json (a second Start-menu
//!     launch brings up the running window, as always);
//!   * a DEBUG build never uses that identifier, and neither does any build run with UC_HOME: they get
//!     `<identifier>.<kind>-<hash of the folder>`, so test instances can never forward a launch to the installed app (nor
//!     accept one from it), even without UC_MULTI. Two instances of the same kind in the same folder do join.
use std::path::Path;

/// Build kind and app folder in one string: what the lock and the `--quit` event are scoped to.
/// `None` for the release build with the default folder.
pub fn scope(debug: bool, home: Option<&Path>) -> Option<String> {
    if !debug && home.is_none() {
        return None;
    }
    let kind = if debug { "debug" } else { "release" };
    let folder = home.map(|h| h.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').to_string()).unwrap_or_else(|| "default".into());
    let mut h: u64 = 0xcbf29ce484222325;
    for b in format!("{kind}|{folder}").bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    Some(format!("{kind}-{h:016x}"))
}

/// The single-instance identifier for this process.
pub fn identifier(base: &str, debug: bool, home: Option<&Path>) -> String {
    match scope(debug, home) {
        None => base.to_string(),
        Some(s) => format!("{base}.{s}"),
    }
}

/// The UC_HOME of this process, if set and not empty.
pub fn env_home() -> Option<std::path::PathBuf> {
    std::env::var_os("UC_HOME").filter(|h| !h.is_empty()).map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: &str = "com.oldbugo.unifiedcontrol";

    #[test]
    fn only_the_release_build_with_the_default_folder_has_the_installed_identifier() {
        assert_eq!(identifier(BASE, false, None), BASE);
        // a debug build, with or without a scratch folder, never does
        assert_ne!(identifier(BASE, true, None), BASE);
        assert_ne!(identifier(BASE, true, Some(Path::new("C:/scratch/home"))), BASE);
        // nor does any build run with its own app folder
        assert_ne!(identifier(BASE, false, Some(Path::new("C:/scratch/home"))), BASE);
        // not even a "default-looking" folder name or an empty-ish path
        for h in ["", "default", r"C:/Users/pc/AppData/Roaming/UnifiedControl"] {
            assert_ne!(identifier(BASE, true, Some(Path::new(h))), BASE);
            assert_ne!(identifier(BASE, false, Some(Path::new(h))), BASE);
        }
        assert!(identifier(BASE, true, None).starts_with(&format!("{BASE}.debug-")));
    }

    #[test]
    fn the_same_kind_and_folder_join_and_different_ones_do_not() {
        let a = identifier(BASE, true, Some(Path::new("C:/scratch/A")));
        assert_eq!(a, identifier(BASE, true, Some(Path::new(r"c:\scratch\a\"))), "case, slashes and a trailing slash do not matter");
        assert_ne!(a, identifier(BASE, true, Some(Path::new("C:/scratch/B"))));
        assert_ne!(a, identifier(BASE, false, Some(Path::new("C:/scratch/A"))), "debug and release never share");
        assert_ne!(identifier(BASE, true, None), identifier(BASE, true, Some(Path::new("C:/scratch/A"))));
    }
}
