//! settings.json: the app's settings, saved on every change (no Save button). Copied from CoolerScreen's
//! `core/settings.rs` (the `SettingsStore` is unchanged); the cooler fields are not here, only Unified Control's own.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The pages of the window's left list (PowerToys-style frame).
pub const PAGES: [&str; 6] = ["home", "cooler", "dock", "dial", "general", "about"];

/// Settings are always handled as the sanitised JSON object.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings(pub Value);

impl Settings {
    /// The page the window shows (the last one the user picked; default Home).
    pub fn page(&self) -> &str {
        self.0["ui"]["page"].as_str().unwrap_or("home")
    }
}

pub fn defaults() -> Value {
    json!({
        "ui": { "page": "home", "followPhysicalPage": true },
        "dock": { "enabled": false, "brightness": 50 }
    })
}

/// Only known keys, each with a valid value (anything else falls back to its default).
pub fn sanitize(s: &Value) -> Settings {
    let page = s["ui"]["page"]
        .as_str()
        .filter(|p| PAGES.contains(p))
        .unwrap_or("home");
    let mut v = defaults();
    v["ui"]["page"] = json!(page);
    v["ui"]["followPhysicalPage"] = json!(s["ui"]["followPhysicalPage"].as_bool().unwrap_or(true));
    v["dock"]["enabled"] = json!(s["dock"]["enabled"].as_bool().unwrap_or(false));
    v["dock"]["brightness"] = json!(s["dock"]["brightness"]
        .as_u64()
        .filter(|n| *n <= 100)
        .unwrap_or(50));
    Settings(v)
}

pub struct SettingsStore {
    file: PathBuf,
    lock: Mutex<()>,
    on_change: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    /// The last settings read or written successfully: used when the file exists but cannot be parsed, so a bad
    /// read never turns into defaults (a default gallery folder would mark the whole library missing).
    last_good: Mutex<Option<Settings>>,
}

impl SettingsStore {
    pub fn new(file: impl Into<PathBuf>) -> Self {
        SettingsStore {
            file: file.into(),
            lock: Mutex::new(()),
            on_change: Mutex::new(None),
            last_good: Mutex::new(None),
        }
    }
    pub fn file(&self) -> &Path {
        &self.file
    }
    pub fn set_on_change(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.on_change.lock().unwrap() = Some(Box::new(f));
    }

    /// Current settings. Missing file: defaults. Unreadable/unparsable file: retried briefly, then the last good
    /// settings (or, on the very first read, defaults after keeping a copy of the bad file as settings.json.bad).
    pub fn load(&self) -> Settings {
        let mut tries = 0;
        loop {
            match super::js::read_text(&self.file) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return sanitize(&json!({})),
                Ok(t) => {
                    if let Ok(v) = serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')) {
                        let s = sanitize(&v);
                        *self.last_good.lock().unwrap() = Some(s.clone());
                        return s;
                    }
                }
                Err(_) => {}
            }
            tries += 1;
            if tries >= 5 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        eprintln!("settings.json could not be read; using the last good settings");
        if let Some(s) = self.last_good.lock().unwrap().clone() {
            return s;
        }
        let mut bad = self.file.as_os_str().to_owned();
        bad.push(".bad");
        let _ = std::fs::copy(&self.file, PathBuf::from(bad));
        sanitize(&json!({}))
    }

    /// Sanitise and write (as 1.2 `save`). Returns what was written.
    pub fn save(&self, s: &Value) -> std::io::Result<Settings> {
        let _g = self.lock.lock().unwrap();
        self.save_locked(s)
    }

    fn save_locked(&self, s: &Value) -> std::io::Result<Settings> {
        let clean = sanitize(s);
        if let Some(d) = self.file.parent() {
            std::fs::create_dir_all(d)?;
        }
        super::js::write_atomic(&self.file, super::js::pretty(&clean.0).as_bytes())?;
        *self.last_good.lock().unwrap() = Some(clean.clone());
        if let Some(f) = self.on_change.lock().unwrap().as_ref() {
            f();
        }
        Ok(clean)
    }

    /// Load, change, save atomically with respect to other `update` calls.
    pub fn update(&self, f: impl FnOnce(&mut Value)) -> std::io::Result<Settings> {
        let _g = self.lock.lock().unwrap();
        let mut v = self.load().0;
        f(&mut v);
        self.save_locked(&v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_sanitising() {
        assert_eq!(sanitize(&json!({})).0, defaults());
        assert_eq!(
            sanitize(&json!({ "ui": { "page": "dock" }, "junk": 1 })).0,
            json!({ "ui": { "page": "dock", "followPhysicalPage":true }, "dock":{"enabled":false,"brightness":50} })
        );
        assert_eq!(
            sanitize(&json!({ "ui": { "page": "cooler" } })).page(),
            "cooler"
        );
        assert_eq!(
            sanitize(&json!({ "ui": { "page": "dial" } })).page(),
            "dial"
        );
        assert_eq!(
            sanitize(&json!({ "ui": { "page": "unknown" } })).page(),
            "home"
        );
        assert_eq!(sanitize(&json!("not an object")).page(), "home");
    }

    #[test]
    fn concurrent_reads_never_see_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let st = std::sync::Arc::new(SettingsStore::new(dir.path().join("settings.json")));
        st.update(|s| s["ui"]["page"] = json!("about")).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writers: Vec<_> = (0..3)
            .map(|_| {
                let (st, stop) = (st.clone(), stop.clone());
                std::thread::spawn(move || {
                    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                        st.update(|s| s["ui"]["page"] = json!("about")).unwrap();
                    }
                })
            })
            .collect();
        let reader_store = SettingsStore::new(dir.path().join("settings.json")); // no shared last-good cache
        for _ in 0..2000 {
            assert_eq!(reader_store.load().page(), "about", "torn or default read");
        }
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        for w in writers {
            w.join().unwrap();
        }
        assert!(std::fs::read_dir(dir.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }

    #[test]
    fn corrupt_file_keeps_last_good() {
        let dir = tempfile::tempdir().unwrap();
        let st = SettingsStore::new(dir.path().join("settings.json"));
        st.update(|s| s["ui"]["page"] = json!("general")).unwrap();
        std::fs::write(dir.path().join("settings.json"), "{ \"ui\": { trunc").unwrap();
        assert_eq!(st.load().page(), "general");
        // a fresh process with only a corrupt file: defaults, and the bad file is kept for inspection
        let st2 = SettingsStore::new(dir.path().join("settings.json"));
        assert_eq!(st2.load().page(), "home");
        assert!(dir.path().join("settings.json.bad").exists());
    }

    #[test]
    fn every_change_is_saved_at_once_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let st = SettingsStore::new(dir.path().join("settings.json"));
        assert_eq!(st.load(), sanitize(&json!({})));
        let n = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let n2 = n.clone();
        st.set_on_change(move || {
            n2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        st.update(|s| s["ui"]["page"] = json!("dock")).unwrap();
        assert_eq!(n.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            SettingsStore::new(dir.path().join("settings.json"))
                .load()
                .page(),
            "dock",
            "on disk straight away"
        );
    }
}
