//! Validated dock profiles and durable storage. Actions remain inert configuration.
//!
//! A single `ProfileStore` owns one UC_HOME. `save` writes a complete recovery journal,
//! then one JSON file per profile and the index, and removes the journal last. A
//! crash between files therefore recovers one complete state, never a mixed page set.
//! At the first successful load in a process, a validated full snapshot is kept as
//! the startup backup. A corrupt current file never replaces that backup.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DISPLAY_COUNT: usize = 14;
pub const MAX_PROFILES: usize = 50;
pub const MAX_PAGES: usize = 200;
pub const MAX_PAGES_PER_PROFILE: usize = 50;
pub const MAX_STATE_BYTES: usize = 8 * 1024 * 1024;
const MAX_INDEX_BYTES: usize = 16 * 1024;
const VERSION: u32 = 1;
pub const FACE_KINDS: &[&str] = &[
    "text", "icon", "picture", "clock", "date", "claude", "codex", "reset", "trend", "cpu", "gpu",
    "ram",
];
pub const USAGE_VIEWS: &[&str] = &[
    "rings",
    "bars",
    "trend",
    "tokens",
    "resets",
    "five-hour-ring",
    "weekly-ring",
    "five-hour-number",
    "weekly-number",
    "five-hour-reset",
    "weekly-reset",
    "five-hour-time",
    "weekly-time",
    "five-hour-trend",
    "weekly-trend",
    "wide-dashboard",
];
pub const ACTION_KINDS: &[&str] = &[
    "none", "hotkey", "media", "launch", "url", "text", "page", "monitor", "folder", "back",
    "cooler",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Face {
    pub kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, rename = "usageView", skip_serializing_if = "Option::is_none")]
    pub usage_view: Option<String>,
}

impl Default for Face {
    fn default() -> Self {
        Self {
            kind: "text".into(),
            text: String::new(),
            asset: None,
            icon: None,
            usage_view: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Key {
    pub a: Face,
    pub b: Face,
    pub action: String,
    pub value: String,
}

impl Key {
    pub fn empty() -> Self {
        Self::default()
    }
}
impl Default for Key {
    fn default() -> Self {
        Self {
            a: Face::default(),
            b: Face::default(),
            action: "none".into(),
            value: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: String,
    pub name: String,
    pub keys: Vec<Key>,
}

impl Page {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id("page"),
            name: name.into(),
            keys: (0..DISPLAY_COUNT).map(|_| Key::empty()).collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub pages: Vec<Page>,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id("profile"),
            name: name.into(),
            pages: vec![Page::new("Main")],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileState {
    pub version: u32,
    pub current_profile: String,
    pub current_page: String,
    pub profiles: Vec<Profile>,
}

impl Default for ProfileState {
    fn default() -> Self {
        let profile = Profile::new("Everyday");
        Self {
            version: VERSION,
            current_profile: profile.id.clone(),
            current_page: profile.pages[0].id.clone(),
            profiles: vec![profile],
        }
    }
}

impl ProfileState {
    pub fn active_profile(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == self.current_profile)
    }
    pub fn active_page(&self) -> Option<&Page> {
        self.active_profile()?
            .pages
            .iter()
            .find(|p| p.id == self.current_page)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            return Err(invalid("Unsupported dock profile version"));
        }
        if self.profiles.is_empty() || self.profiles.len() > MAX_PROFILES {
            return Err(invalid("A layout must contain 1 to 50 profiles"));
        }
        let mut profile_ids = HashSet::new();
        let mut page_ids = HashSet::new();
        let mut total_pages = 0;
        for profile in &self.profiles {
            validate_id(&profile.id)?;
            if !profile_ids.insert(profile.id.as_str()) {
                return Err(invalid("Duplicate profile ID"));
            }
            validate_name(&profile.name)?;
            if profile.pages.is_empty() || profile.pages.len() > MAX_PAGES_PER_PROFILE {
                return Err(invalid("A profile must contain 1 to 50 pages"));
            }
            total_pages += profile.pages.len();
            if total_pages > MAX_PAGES {
                return Err(invalid("A layout cannot contain more than 200 pages"));
            }
            let own_page_ids: HashSet<&str> = profile.pages.iter().map(|p| p.id.as_str()).collect();
            for page in &profile.pages {
                validate_id(&page.id)?;
                if !page_ids.insert(page.id.as_str()) {
                    return Err(invalid("Duplicate page ID"));
                }
                validate_name(&page.name)?;
                if page.keys.len() != DISPLAY_COUNT {
                    return Err(invalid("A page must contain 13 keys and one wide display"));
                }
                for key in &page.keys {
                    for face in [&key.a, &key.b] {
                        if !FACE_KINDS.contains(&face.kind.as_str()) {
                            return Err(invalid("Unknown face presentation"));
                        }
                        if face.text.chars().count() > 40 || face.text.chars().any(char::is_control)
                        {
                            return Err(invalid(
                                "Display text must contain at most 40 printable characters",
                            ));
                        }
                        if let Some(id) = &face.asset {
                            validate_id(id)?;
                        }
                        if let Some(icon) = &face.icon {
                            if !super::key_render::ICON_NAMES.contains(&icon.as_str()) {
                                return Err(invalid("Unknown icon"));
                            }
                        }
                        if let Some(view) = &face.usage_view {
                            if !USAGE_VIEWS.contains(&view.as_str()) {
                                return Err(invalid("Unknown usage presentation"));
                            }
                        }
                        if face.kind == "claude" && face.usage_view.as_deref() == Some("tokens") {
                            return Err(invalid("Daily tokens are available only for Codex"));
                        }
                    }
                    if !ACTION_KINDS.contains(&key.action.as_str()) {
                        return Err(invalid("Unknown key action"));
                    }
                    if key.value.len() > 4096 || key.value.contains('\0') {
                        return Err(invalid(
                            "Action data is too long or contains a null character",
                        ));
                    }
                    if key.action == "cooler" {
                        super::actions::parse_cooler(&key.value)
                            .map_err(|message| invalid(&message))?;
                    }
                    if ["page", "folder"].contains(&key.action.as_str())
                        && !own_page_ids.contains(key.value.as_str())
                    {
                        return Err(invalid(
                            "Page action must refer to a page in the same profile",
                        ));
                    }
                }
            }
        }
        if self.active_page().is_none() {
            return Err(invalid("Selected profile or page does not exist"));
        }
        if serde_json::to_vec(self)?.len() > MAX_STATE_BYTES {
            return Err(invalid("Dock profiles exceed the 8 MiB limit"));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ProfileError(String);
pub type Result<T> = std::result::Result<T, ProfileError>;
impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ProfileError {}
impl ProfileError {
    pub fn is_conflict(&self) -> bool {
        self.0 == "Dock layout changed; read the latest revision and retry"
    }
}
impl From<io::Error> for ProfileError {
    fn from(e: io::Error) -> Self {
        Self(format!("Dock profile storage failed: {e}"))
    }
}
impl From<serde_json::Error> for ProfileError {
    fn from(_: serde_json::Error) -> Self {
        Self("Dock profile JSON is invalid".into())
    }
}
fn invalid(message: &str) -> ProfileError {
    ProfileError(message.into())
}

fn new_id(prefix: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{prefix}-{time:x}-{:x}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(invalid("Invalid profile or page ID"));
    }
    Ok(())
}
fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(invalid("Names must contain 1 to 80 printable characters"));
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Index {
    version: u32,
    current_profile: String,
    current_page: String,
    profiles: Vec<String>,
}

struct Memory {
    state: Option<ProfileState>,
    notice: Option<String>,
    revision: String,
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            state: None,
            notice: None,
            revision: new_id("revision"),
        }
    }
}

pub struct ProfileStore {
    dir: PathBuf,
    memory: Mutex<Memory>,
    replica: bool,
}

impl ProfileStore {
    /// `base` is UC_HOME, not its profiles subdirectory. This constructor does no I/O.
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self {
            dir: base.into().join("profiles"),
            memory: Mutex::new(Memory::default()),
            replica: false,
        }
    }
    /// Worker-only validated snapshot. It never reads or writes profile files.
    pub fn replica(state: ProfileState) -> Result<Self> {
        state.validate()?;
        Ok(Self {
            dir: PathBuf::new(),
            memory: Mutex::new(Memory {
                state: Some(state),
                ..Memory::default()
            }),
            replica: true,
        })
    }
    pub fn replace_replica(&self, state: ProfileState) -> Result<()> {
        if !self.replica {
            return Err(invalid("Only a worker replica can be replaced"));
        }
        state.validate()?;
        let mut memory = self.memory.lock().unwrap();
        memory.state = Some(state);
        memory.revision = new_id("revision");
        Ok(())
    }
    pub fn directory(&self) -> &Path {
        &self.dir
    }
    pub fn recovery_notice(&self) -> Option<String> {
        self.memory.lock().unwrap().notice.clone()
    }
    pub fn state(&self) -> Result<ProfileState> {
        self.load()
    }
    pub fn state_with_revision(&self) -> Result<(ProfileState, String)> {
        let mut memory = self.memory.lock().unwrap();
        let state = self.load_locked(&mut memory)?;
        Ok((state, memory.revision.clone()))
    }
    pub fn load(&self) -> Result<ProfileState> {
        self.load_locked(&mut self.memory.lock().unwrap())
    }

    /// Every successful call persists the whole validated state; no separate Save action exists.
    /// Use one store per UC_HOME; its mutex serializes all in-process writers.
    pub fn save(&self, state: &ProfileState) -> Result<ProfileState> {
        self.save_versioned(state, None).map(|(state, _)| state)
    }
    pub fn save_with_revision(&self, state: &ProfileState) -> Result<(ProfileState, String)> {
        self.save_versioned(state, None)
    }
    pub fn save_if_revision(
        &self,
        state: &ProfileState,
        expected: &str,
    ) -> Result<(ProfileState, String)> {
        self.save_versioned(state, Some(expected))
    }
    fn save_versioned(
        &self,
        state: &ProfileState,
        expected: Option<&str>,
    ) -> Result<(ProfileState, String)> {
        if self.replica {
            return Err(invalid("Worker profiles are read-only"));
        }
        state.validate()?;
        let mut memory = self.memory.lock().unwrap();
        self.load_locked(&mut memory)?; // establish a valid startup backup before the first edit
        if expected.is_some_and(|revision| revision != memory.revision) {
            return Err(invalid(
                "Dock layout changed; read the latest revision and retry",
            ));
        }
        self.write_transaction(state)?;
        memory.state = Some(state.clone());
        memory.revision = new_id("revision");
        Ok((state.clone(), memory.revision.clone()))
    }

    fn load_locked(&self, memory: &mut Memory) -> Result<ProfileState> {
        if let Some(state) = &memory.state {
            return Ok(state.clone());
        }
        fs::create_dir_all(&self.dir)?;
        let journal = self.dir.join("transaction.json");
        let index = self.dir.join("index.json");
        let backup = self.dir.join("startup-backup.json");
        let (state, recovered_backup) = if journal.try_exists()? {
            match read_state(&journal) {
                Ok(state) => {
                    memory.notice =
                        Some("Recovered a complete dock layout from an interrupted save.".into());
                    (state, false)
                }
                Err(_) => {
                    let state = read_state(&backup).map_err(|_| invalid("An interrupted dock save and its backup could not be read. Existing files were kept."))?;
                    memory.notice = Some(
                        "An interrupted dock save was damaged. Recovered the startup backup."
                            .into(),
                    );
                    (state, true)
                }
            }
        } else if index.try_exists()? {
            match self.read_current() {
                Ok(state) => (state, false),
                Err(_) => {
                    let state = read_state(&backup).map_err(|_| invalid("Dock profiles and their startup backup could not be read. Existing files were kept."))?;
                    memory.notice = Some(
                        "A dock profile could not be read. Recovered the startup backup.".into(),
                    );
                    (state, true)
                }
            }
        } else if backup.try_exists()? {
            let state = read_state(&backup)?;
            memory.notice =
                Some("The dock profile index is missing. Recovered the startup backup.".into());
            (state, true)
        } else {
            // Missing metadata beside existing profiles is damage, not permission to erase a layout.
            for entry in fs::read_dir(&self.dir)? {
                if managed_profile_id(&entry?.file_name().to_string_lossy()).is_some() {
                    return Err(invalid(
                        "The dock profile index is missing. Existing profile files were kept.",
                    ));
                }
            }
            let state = ProfileState::default();
            self.write_transaction(&state)?;
            (state, false)
        };
        state.validate()?;
        // Only an independently validated current state can advance the startup backup.
        if !recovered_backup {
            write_json(&backup, &state)?;
        }
        memory.state = Some(state.clone());
        Ok(state)
    }

    fn read_current(&self) -> Result<ProfileState> {
        let index: Index = read_json(&self.dir.join("index.json"), MAX_INDEX_BYTES)?;
        if index.version != VERSION
            || index.profiles.is_empty()
            || index.profiles.len() > MAX_PROFILES
        {
            return Err(invalid("Invalid dock profile index"));
        }
        let mut seen = HashSet::new();
        let mut profiles = Vec::with_capacity(index.profiles.len());
        let mut remaining_bytes = MAX_STATE_BYTES;
        for id in index.profiles {
            validate_id(&id)?;
            if !seen.insert(id.clone()) {
                return Err(invalid("Duplicate profile in dock index"));
            }
            let profile_path = self.profile_path(&id)?;
            let file_bytes = usize::try_from(fs::metadata(&profile_path)?.len())
                .map_err(|_| invalid("Dock profiles exceed the 8 MiB limit"))?;
            remaining_bytes = remaining_bytes
                .checked_sub(file_bytes)
                .ok_or_else(|| invalid("Dock profiles exceed the 8 MiB limit"))?;
            let profile: Profile = read_json(&profile_path, file_bytes)?;
            if profile.id != id {
                return Err(invalid("Dock profile ID does not match its index"));
            }
            profiles.push(profile);
        }
        let state = ProfileState {
            version: index.version,
            current_profile: index.current_profile,
            current_page: index.current_page,
            profiles,
        };
        state.validate()?;
        Ok(state)
    }

    fn profile_path(&self, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        Ok(self.dir.join(format!("profile-{id}.json")))
    }

    fn write_transaction(&self, state: &ProfileState) -> Result<()> {
        state.validate()?;
        fs::create_dir_all(&self.dir)?;
        let journal = self.dir.join("transaction.json");
        write_json(&journal, state)?;
        for profile in &state.profiles {
            write_json(&self.profile_path(&profile.id)?, profile)?;
        }
        let index = Index {
            version: state.version,
            current_profile: state.current_profile.clone(),
            current_page: state.current_page.clone(),
            profiles: state.profiles.iter().map(|p| p.id.clone()).collect(),
        };
        write_json(&self.dir.join("index.json"), &index)?;
        // After the index is committed, discard only this store's unreferenced profile files.
        // The complete startup backup remains available, including profiles deleted this session.
        let current: HashSet<&str> = state.profiles.iter().map(|p| p.id.as_str()).collect();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if let Some(id) = managed_profile_id(&name.to_string_lossy()) {
                if !current.contains(id) && entry.file_type()?.is_file() {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        fs::remove_file(&journal)?;
        Ok(())
    }
}

fn managed_profile_id(name: &str) -> Option<&str> {
    let id = name.strip_prefix("profile-")?.strip_suffix(".json")?;
    validate_id(id).ok()?;
    Some(id)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, limit: usize) -> Result<T> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > limit as u64 {
        return Err(invalid("Dock profile file exceeds its size limit"));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid("Dock profile file exceeds its size limit"));
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    Ok(serde_json::from_slice(bytes)?)
}
fn read_state(path: &Path) -> Result<ProfileState> {
    let state: ProfileState = read_json(path, MAX_STATE_BYTES)?;
    state.validate()?;
    Ok(state)
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    // Compact encoding makes the documented total byte bound exact on disk too.
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(invalid("Dock profiles exceed the 8 MiB limit"));
    }
    super::js::write_atomic(path, &bytes)?;
    Ok(())
}

/// Import the UI prototype's index-based local draft once. No action is executed.
/// Extra UI navigation fields are ignored; profile/page IDs are newly assigned.
pub fn import_preview(value: &Value) -> Result<ProfileState> {
    if serde_json::to_vec(value)?.len() > MAX_STATE_BYTES {
        return Err(invalid("Prototype draft exceeds the 8 MiB limit"));
    }
    if value.get("version").and_then(Value::as_u64) != Some(VERSION as u64) {
        return Err(invalid("Unsupported prototype draft version"));
    }
    let old_profiles = value
        .get("profiles")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Prototype profiles are missing"))?;
    if old_profiles.is_empty() || old_profiles.len() > MAX_PROFILES {
        return Err(invalid("Prototype profile count is invalid"));
    }
    let mut profiles = Vec::new();
    let mut total_pages = 0;
    for old_profile in old_profiles {
        let name = old_profile
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Prototype profile name is missing"))?;
        let old_pages = old_profile
            .get("pages")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Prototype pages are missing"))?;
        total_pages += old_pages.len();
        if old_pages.is_empty()
            || old_pages.len() > MAX_PAGES_PER_PROFILE
            || total_pages > MAX_PAGES
        {
            return Err(invalid("Prototype page count is invalid"));
        }
        let mut pages = Vec::new();
        for old_page in old_pages {
            let name = old_page
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("Prototype page name is missing"))?;
            let keys: Vec<Key> = serde_json::from_value(
                old_page
                    .get("keys")
                    .cloned()
                    .ok_or_else(|| invalid("Prototype keys are missing"))?,
            )?;
            pages.push(Page {
                id: new_id("page"),
                name: name.into(),
                keys,
            });
        }
        let ids: Vec<String> = pages.iter().map(|p| p.id.clone()).collect();
        for page in &mut pages {
            for key in &mut page.keys {
                if ["page", "folder"].contains(&key.action.as_str()) {
                    let target = key
                        .value
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| ids.get(i))
                        .ok_or_else(|| invalid("Prototype page action target does not exist"))?;
                    key.value = target.clone();
                }
            }
        }
        profiles.push(Profile {
            id: new_id("profile"),
            name: name.into(),
            pages,
        });
    }
    let current_profile = value
        .get("profile")
        .and_then(Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
        .unwrap_or(0);
    let selected_profile = profiles
        .get(current_profile)
        .ok_or_else(|| invalid("Prototype selected profile does not exist"))?;
    let current_page = value
        .get("page")
        .and_then(Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
        .unwrap_or(0);
    let selected_page = selected_profile
        .pages
        .get(current_page)
        .ok_or_else(|| invalid("Prototype selected page does not exist"))?;
    let state = ProfileState {
        version: VERSION,
        current_profile: selected_profile.id.clone(),
        current_page: selected_page.id.clone(),
        profiles,
    };
    state.validate()?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    #[test]
    fn worker_replica_is_validated_read_only_and_file_free() {
        let first = super::ProfileState::default();
        let replica = super::ProfileStore::replica(first.clone()).unwrap();
        assert_eq!(replica.state().unwrap(), first);
        assert!(replica.directory().as_os_str().is_empty());
        assert!(replica.save(&first).is_err());
        let mut replacement = first.clone();
        replacement.profiles[0].name = "Changed in host".into();
        replica.replace_replica(replacement.clone()).unwrap();
        assert_eq!(replica.state().unwrap(), replacement);
        let mut invalid = first;
        invalid.version = 99;
        assert!(replica.replace_replica(invalid).is_err());
        assert_eq!(replica.state().unwrap(), replacement);
    }
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn store() -> (tempfile::TempDir, ProfileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::new(dir.path());
        (dir, store)
    }
    fn edited(mut state: ProfileState, text: &str) -> ProfileState {
        state.profiles[0].pages[0].keys[0].a.text = text.into();
        state
    }

    #[test]
    fn round_trip_and_stable_ids_saved_at_once() {
        let (dir, store) = store();
        let original = store.load().unwrap();
        let changed = edited(original.clone(), "Focus");
        store.save(&changed).unwrap();
        assert_eq!(ProfileStore::new(dir.path()).load().unwrap(), changed);
        assert_eq!(changed.current_profile, original.current_profile);
        assert_eq!(changed.current_page, original.current_page);
        assert_eq!(changed.active_page().unwrap().keys.len(), 14);
        assert!(!store.directory().join("transaction.json").exists());
        assert!(store
            .profile_path(&changed.current_profile)
            .unwrap()
            .exists());
    }

    #[test]
    fn corrupt_profile_recovers_startup_backup_without_poisoning_it() {
        let (dir, store) = store();
        let initial = store.load().unwrap();
        let saved = edited(initial, "Before restart");
        store.save(&saved).unwrap();
        let after_restart = ProfileStore::new(dir.path());
        assert_eq!(after_restart.load().unwrap(), saved); // backup now contains saved
        after_restart
            .save(&edited(saved.clone(), "Later edit"))
            .unwrap();
        fs::write(
            after_restart.profile_path(&saved.current_profile).unwrap(),
            "{broken",
        )
        .unwrap();
        let backup_before =
            fs::read(after_restart.directory().join("startup-backup.json")).unwrap();
        let recovered = ProfileStore::new(dir.path());
        assert_eq!(recovered.load().unwrap(), saved);
        assert!(recovered.recovery_notice().unwrap().contains("backup"));
        assert_eq!(
            fs::read(recovered.directory().join("startup-backup.json")).unwrap(),
            backup_before
        );
        assert_eq!(
            ProfileStore::new(dir.path()).load().unwrap(),
            saved,
            "another restart must not replace the good backup with corruption"
        );
    }

    #[test]
    fn corrupt_index_and_missing_index_recover_backup() {
        let (dir, store) = store();
        let state = store.load().unwrap();
        fs::write(store.directory().join("index.json"), "{}").unwrap();
        assert_eq!(ProfileStore::new(dir.path()).load().unwrap(), state);
        fs::remove_file(store.directory().join("index.json")).unwrap();
        assert_eq!(ProfileStore::new(dir.path()).load().unwrap(), state);
    }

    #[test]
    fn missing_metadata_beside_existing_profiles_never_creates_defaults() {
        let (dir, store) = store();
        let state = store.load().unwrap();
        fs::remove_file(store.directory().join("index.json")).unwrap();
        fs::remove_file(store.directory().join("startup-backup.json")).unwrap();
        let before = fs::read(store.profile_path(&state.current_profile).unwrap()).unwrap();
        assert!(ProfileStore::new(dir.path()).load().is_err());
        assert!(!store.directory().join("index.json").exists());
        assert_eq!(
            fs::read(store.profile_path(&state.current_profile).unwrap()).unwrap(),
            before
        );
    }

    #[test]
    fn unrecoverable_files_are_kept_and_never_reset() {
        let (dir, store) = store();
        store.load().unwrap();
        fs::write(store.directory().join("index.json"), "broken-index").unwrap();
        fs::write(
            store.directory().join("startup-backup.json"),
            "broken-backup",
        )
        .unwrap();
        let broken = ProfileStore::new(dir.path());
        assert!(broken.load().is_err());
        assert!(broken.save(&ProfileState::default()).is_err());
        assert_eq!(
            fs::read_to_string(store.directory().join("index.json")).unwrap(),
            "broken-index"
        );
    }

    #[test]
    fn interrupted_save_recovers_whole_snapshot_not_partial_profiles() {
        let (dir, store) = store();
        let initial = store.load().unwrap();
        let mut target = edited(initial, "Committed to journal");
        target.profiles.push(Profile::new("Second"));
        write_json(&store.directory().join("transaction.json"), &target).unwrap();
        fs::write(store.directory().join("index.json"), "torn").unwrap();
        let recovered = ProfileStore::new(dir.path());
        assert_eq!(recovered.load().unwrap(), target);
        assert!(recovered.recovery_notice().unwrap().contains("interrupted"));
        recovered.save(&target).unwrap();
        assert_eq!(ProfileStore::new(dir.path()).load().unwrap(), target);
    }

    #[test]
    fn invalid_journal_does_not_trust_a_mixed_current_state() {
        let (dir, store) = store();
        let initial = store.load().unwrap();
        store.save(&edited(initial.clone(), "Later edit")).unwrap();
        fs::write(store.directory().join("transaction.json"), "torn").unwrap();
        assert_eq!(ProfileStore::new(dir.path()).load().unwrap(), initial);
    }

    #[test]
    fn cooler_action_storage_roundtrips_and_rejects_invalid_operations() {
        let (_dir, store) = store();
        let mut state = store.load().unwrap();
        state.profiles[0].pages[0].keys[0].action = "cooler".into();
        state.profiles[0].pages[0].keys[0].value =
            r#"{"op":"theme_select","theme_id":"theme-does-not-need-to-be-online"}"#.into();
        store.save(&state).unwrap();
        assert_eq!(store.load().unwrap(), state);
        state.profiles[0].pages[0].keys[0].value = r#"{"op":"brightness_step","delta":0}"#.into();
        assert!(store.save(&state).is_err());
        assert_ne!(store.load().unwrap(), state);
    }

    #[test]
    fn traversal_duplicate_ids_targets_and_shapes_are_rejected_before_writing() {
        let (_dir, store) = store();
        let state = store.load().unwrap();
        let mut cases = Vec::new();
        let mut bad = state.clone();
        bad.profiles[0].id = "../outside".into();
        cases.push(bad);
        let mut bad = state.clone();
        bad.profiles.push(bad.profiles[0].clone());
        cases.push(bad);
        let mut bad = state.clone();
        bad.profiles[0].pages[0].keys.pop();
        cases.push(bad);
        let mut bad = state.clone();
        bad.current_page = "missing".into();
        cases.push(bad);
        let mut bad = state.clone();
        bad.profiles[0].pages[0].keys[0].action = "shell".into();
        cases.push(bad);
        let mut bad = state.clone();
        bad.profiles[0].pages[0].keys[0].action = "page".into();
        bad.profiles[0].pages[0].keys[0].value = "missing".into();
        cases.push(bad);
        let mut bad = state.clone();
        bad.profiles[0].pages[0].keys[0].a.text = "x".repeat(41);
        cases.push(bad);
        let mut bad = state.clone();
        bad.profiles[0].pages[0].keys[0].a.text = "line\nbreak".into();
        cases.push(bad);
        for bad in cases {
            assert!(store.save(&bad).is_err());
        }
        assert_eq!(store.state().unwrap(), state);
    }

    #[test]
    fn indexed_path_traversal_falls_back_without_reading_outside() {
        let (dir, store) = store();
        let state = store.load().unwrap();
        let outside = dir.path().join("outside.json");
        fs::write(&outside, "keep").unwrap();
        fs::write(store.directory().join("index.json"), r#"{"version":1,"currentProfile":"../outside","currentPage":"x","profiles":["../outside"]}"#).unwrap();
        assert_eq!(ProfileStore::new(dir.path()).load().unwrap(), state);
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
    }

    #[test]
    fn preview_import_assigns_ids_and_converts_page_references() {
        let mut first = Page::new("Main");
        let second = Page::new("Tools");
        first.keys[0].action = "page".into();
        first.keys[0].value = "1".into();
        let strip_id = |p: Page| json!({"name":p.name,"keys":p.keys});
        let preview = json!({"version":1,"current":"dock","profile":0,"page":1,"profiles":[{"name":"Everyday","pages":[strip_id(first),strip_id(second)]}]});
        let state = import_preview(&preview).unwrap();
        assert_eq!(state.current_page, state.profiles[0].pages[1].id);
        assert_eq!(state.profiles[0].pages[0].keys[0].value, state.current_page);
        assert_eq!(state.active_page().unwrap().name, "Tools");
    }

    #[test]
    fn total_page_bound_and_oversize_files_are_rejected() {
        let (_dir, store) = store();
        let mut state = ProfileState::default();
        state.profiles.clear();
        for _ in 0..5 {
            let mut profile = Profile::new("Many");
            profile.pages = (0..41).map(|_| Page::new("Page")).collect();
            state.profiles.push(profile);
        }
        state.current_profile = state.profiles[0].id.clone();
        state.current_page = state.profiles[0].pages[0].id.clone();
        assert!(state.validate().is_err());
        store.load().unwrap();
        fs::write(
            store.directory().join("index.json"),
            " ".repeat(MAX_INDEX_BYTES + 1),
        )
        .unwrap();
        assert!(store.read_current().is_err());
    }

    #[test]
    fn serialized_state_byte_bound_covers_all_profiles_together() {
        let mut state = ProfileState::default();
        state.profiles.clear();
        for _ in 0..4 {
            let mut profile = Profile::new("Full");
            profile.pages = (0..50).map(|_| Page::new("Page")).collect();
            for page in &mut profile.pages {
                for key in &mut page.keys {
                    key.action = "text".into();
                    key.value = "x".repeat(4096);
                }
            }
            state.profiles.push(profile);
        }
        state.current_profile = state.profiles[0].id.clone();
        state.current_page = state.profiles[0].pages[0].id.clone();
        assert!(state.validate().unwrap_err().to_string().contains("8 MiB"));
    }

    #[test]
    fn deleting_profiles_keeps_file_count_bounded_and_preserves_unowned_files() {
        let (_dir, store) = store();
        let mut state = store.load().unwrap();
        let extra = Profile::new("Remove later");
        let file = store.profile_path(&extra.id).unwrap();
        state.profiles.push(extra);
        store.save(&state).unwrap();
        assert!(file.exists());
        fs::write(store.directory().join("personal-note.txt"), "keep").unwrap();
        state.profiles.pop();
        store.save(&state).unwrap();
        assert!(!file.exists());
        assert_eq!(
            fs::read_to_string(store.directory().join("personal-note.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn concurrent_saves_are_complete_and_leave_no_shared_temp_file() {
        let (dir, store) = store();
        let store = Arc::new(store);
        let initial = store.load().unwrap();
        let threads: Vec<_> = (0..4)
            .map(|i| {
                let store = store.clone();
                let state = edited(initial.clone(), &format!("Worker {i}"));
                std::thread::spawn(move || {
                    for _ in 0..10 {
                        store.save(&state).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let disk = ProfileStore::new(dir.path()).load().unwrap();
        assert_eq!(store.state().unwrap(), disk);
        assert!(fs::read_dir(store.directory()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }
}
