//! Copy-once migration. The old app data and original gallery remain untouched.
use super::error::{CoreError, CoreResult};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Upgrade the already-published copy without replacing settings or library edits.
/// The caller keeps the worker stopped until its owned source folder is ready.
pub fn upgrade_sources(source_plugins: &Path, target: &Path) -> CoreResult<()> {
    checked_directory(target)?;
    let mut marker = read_marker(target)?;
    let sources = target.join("Sources");
    let target_present = checked_directory(&sources)?;
    if marker["version"] == 2 {
        // A completed import is not a continuing synchronization relationship.
        // Keep later UC edits, including intentionally removed plug-ins.
        return Ok(());
    }
    let source_present = checked_directory(source_plugins)?;
    if source_present {
        let from = source_plugins.canonicalize()?;
        let to = target.canonicalize()?.join("Sources");
        if mapped(&to.to_string_lossy(), &[(from.clone(), PathBuf::new())]).is_some()
            || mapped(&from.to_string_lossy(), &[(to, PathBuf::new())]).is_some()
        {
            return Err(CoreError::new(
                "The cooler source plug-in folders must be separate. Existing data was kept.",
            ));
        }
    }
    let parent = target
        .parent()
        .ok_or_else(|| CoreError::new("Invalid cooler data folder"))?;
    static NEXT_STAGE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let stage = parent.join(format!(
        "cooler-sources-import-{}-{}-{}",
        std::process::id(),
        super::scheduler::now_ms(),
        NEXT_STAGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    // Claim a new directory before arming cleanup. Never clean an existing path
    // if creation failed, and never point this guard at the published Sources.
    std::fs::create_dir(&stage)?;
    let mut stage_guard = OwnedImportStage::new(stage.clone(), true);
    if source_present {
        if !checked_directory(source_plugins)? {
            return Err(CoreError::new(
                "Cooler source plug-ins changed during import. Existing data was kept.",
            ));
        }
        for entry in std::fs::read_dir(source_plugins)? {
            let entry = entry?;
            copy_tree(&entry.path(), &stage.join(entry.file_name()))?;
        }
        if !same_tree(source_plugins, &stage)? {
            return Err(CoreError::new(
                "Cooler source plug-ins changed during import. Existing data was kept; retry when they are idle.",
            ));
        }
    }
    let files = tree_entries(&stage)?
        .values()
        .filter(|directory| !**directory)
        .count();
    if target_present {
        if !same_tree(&stage, &sources)? {
            // Never turn an upgrade into a merge that overwrites newer UC work.
            return Err(CoreError::coded(
                "cooler_migration_conflict",
                "UC already has different cooler source plug-ins. Both copies were kept; review them before importing.",
            ));
        }
    } else {
        std::fs::rename(&stage, &sources)?;
        stage_guard.published();
    }
    marker["version"] = json!(2);
    marker["sourcePlugins"] = json!({
        "imported":true,"sourcePresentAtImport":source_present,"copiedFilesAtImport":files
    });
    let pending = target.join(format!(
        "{}.json.tmp",
        stage.file_name().unwrap().to_string_lossy()
    ));
    let mut pending_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    let mut marker_guard = OwnedImportStage::new(pending.clone(), false);
    pending_file.write_all(&serde_json::to_vec_pretty(&marker).unwrap())?;
    pending_file.sync_all()?;
    drop(pending_file);
    // If publication fails, V1 remains and the next attempt safely compares the
    // already-copied folder before completing the marker.
    std::fs::rename(pending, target.join("migration.json"))?;
    marker_guard.published();
    Ok(())
}

/// Only paths successfully created by this upgrade may enter this guard.
struct OwnedImportStage {
    path: Option<PathBuf>,
    directory: bool,
}
impl OwnedImportStage {
    fn new(path: PathBuf, directory: bool) -> Self {
        Self {
            path: Some(path),
            directory,
        }
    }
    fn published(&mut self) {
        self.path = None;
    }
}
impl Drop for OwnedImportStage {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            if self.directory {
                let _ = std::fs::remove_dir_all(path);
            } else {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Finite provenance only. These are historical import counts, not a claim that
/// the user has never changed the copied files afterward. No local paths escape.
pub fn migration_summary(target: &Path) -> CoreResult<Value> {
    if !checked_directory(target)? || !target.join("migration.json").exists() {
        return Ok(
            json!({"version":0,"originalKept":false,"sourcePlugins":{"imported":false,"folderPresent":false}}),
        );
    }
    let marker = read_marker(target)?;
    let present = checked_directory(&target.join("Sources"))?;
    let mut sources = json!({"imported":marker["version"]==2,"folderPresent":present});
    if marker["version"] == 2 {
        sources["sourcePresentAtImport"] = marker["sourcePlugins"]["sourcePresentAtImport"].clone();
        sources["copiedFilesAtImport"] = marker["sourcePlugins"]["copiedFilesAtImport"].clone();
    }
    Ok(
        json!({"version":marker["version"],"copiedExisting":marker["copiedExisting"],"originalKept":true,"sourcePlugins":sources}),
    )
}

fn checked_directory(path: &Path) -> CoreResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || is_reparse(&meta) || !meta.is_dir() {
                return Err(CoreError::new("Linked or invalid cooler source folders cannot be imported. Existing data was kept."));
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn read_marker(target: &Path) -> CoreResult<Value> {
    let path = target.join("migration.json");
    let meta = std::fs::symlink_metadata(&path)?;
    if meta.file_type().is_symlink() || is_reparse(&meta) || !meta.is_file() {
        return Err(CoreError::new(
            "The cooler import marker is linked or invalid. Existing data was kept.",
        ));
    }
    let marker: Value = serde_json::from_slice(&std::fs::read(&path)?).map_err(|_| {
        CoreError::new("The cooler import marker could not be read. Existing data was kept.")
    })?;
    let version = marker["version"].as_u64();
    if !matches!(version, Some(1 | 2))
        || marker["originalKept"] != true
        || !marker["copiedExisting"].is_boolean()
    {
        return Err(CoreError::new(
            "The cooler import version is unsupported. Existing data was kept.",
        ));
    }
    if version == Some(2)
        && (marker["sourcePlugins"]["imported"] != true
            || !marker["sourcePlugins"]["sourcePresentAtImport"].is_boolean()
            || marker["sourcePlugins"]["copiedFilesAtImport"]
                .as_u64()
                .is_none())
    {
        return Err(CoreError::new(
            "The cooler source import marker is invalid. Existing data was kept.",
        ));
    }
    Ok(marker)
}

fn tree_entries(root: &Path) -> CoreResult<BTreeMap<PathBuf, bool>> {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, bool>) -> CoreResult<()> {
        let meta = std::fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() || is_reparse(&meta) {
            return Err(CoreError::new(
                "Linked cooler source plug-ins cannot be imported. Existing data was kept.",
            ));
        }
        if path != root {
            entries.insert(path.strip_prefix(root).unwrap().into(), meta.is_dir());
        }
        if meta.is_dir() {
            for entry in std::fs::read_dir(path)? {
                visit(root, &entry?.path(), entries)?;
            }
        } else if !meta.is_file() {
            return Err(CoreError::new(
                "An unsupported cooler source entry could not be imported.",
            ));
        }
        Ok(())
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries)?;
    Ok(entries)
}

fn same_tree(a: &Path, b: &Path) -> CoreResult<bool> {
    let entries = tree_entries(a)?;
    if entries != tree_entries(b)? {
        return Ok(false);
    }
    let (mut left, mut right) = ([0u8; 8192], [0u8; 8192]);
    for (relative, directory) in entries {
        if directory {
            continue;
        }
        let mut a = std::fs::File::open(a.join(&relative))?;
        let mut b = std::fs::File::open(b.join(&relative))?;
        loop {
            let n = a.read(&mut left)?;
            let m = b.read(&mut right)?;
            if n != m || left[..n] != right[..m] {
                return Ok(false);
            }
            if n == 0 {
                break;
            }
        }
    }
    Ok(true)
}

pub fn prepare(source: &Path, target: &Path, default_gallery: &Path) -> CoreResult<()> {
    for path in [source, target] {
        if path.exists() {
            let meta = std::fs::symlink_metadata(path)?;
            if meta.file_type().is_symlink() || is_reparse(&meta) {
                return Err(CoreError::new(
                    "Linked cooler data folders cannot be imported automatically.",
                ));
            }
        }
    }
    if target.join("migration.json").is_file() {
        return Ok(());
    }
    if target.exists() && std::fs::read_dir(target)?.next().is_some() {
        return Err(CoreError::new("The cooler data folder already contains data without a completed import. Existing files were kept."));
    }
    let parent = target
        .parent()
        .ok_or_else(|| CoreError::new("Invalid cooler data folder"))?;
    std::fs::create_dir_all(parent)?;
    let stage = parent.join(format!(
        "cooler-import-{}-{}",
        std::process::id(),
        super::scheduler::now_ms()
    ));
    std::fs::create_dir(&stage)?;
    let settings_file = source.join("settings.json");
    if settings_file.exists() {
        let meta = std::fs::symlink_metadata(&settings_file)?;
        if meta.file_type().is_symlink() || is_reparse(&meta) {
            return Err(CoreError::new(
                "Linked cooler settings cannot be imported automatically.",
            ));
        }
    }
    let mut settings: Value = if settings_file.is_file() {
        serde_json::from_slice(&std::fs::read(&settings_file)?).map_err(|_| {
            CoreError::new("CoolerScreen settings could not be read. The original data was kept.")
        })?
    } else {
        json!({})
    };
    if !settings.is_object() {
        return Err(CoreError::new(
            "CoolerScreen settings have an invalid format. The original data was kept.",
        ));
    }
    let gallery = settings["galleryDir"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_gallery.into());
    let resolved_target = parent.canonicalize()?.join(
        target
            .file_name()
            .ok_or_else(|| CoreError::new("Invalid cooler folder"))?,
    );
    for root in [source, &gallery] {
        let resolved_root = if root.exists() {
            root.canonicalize()?
        } else {
            std::path::absolute(root)?
        };
        if mapped(
            &resolved_target.to_string_lossy(),
            &[(resolved_root, PathBuf::from("overlap"))],
        )
        .is_some()
        {
            return Err(CoreError::new(
                "The new cooler folder must be separate from the original library and gallery.",
            ));
        }
    }
    if source.exists() {
        for name in ["companion.json", "library"] {
            let from = source.join(name);
            if from.exists() {
                copy_tree(&from, &stage.join(name))?;
            }
        }
        let offsets = source.join("lyrics/offsets.json");
        if offsets.is_file() {
            copy_tree(&offsets, &stage.join("lyrics/offsets.json"))?;
        }
    }
    if gallery.exists() {
        copy_tree(&gallery, &stage.join("Gallery"))?;
    } else {
        std::fs::create_dir(stage.join("Gallery"))?;
    }
    let mappings = [
        (source.to_path_buf(), target.to_path_buf()),
        (gallery, target.join("Gallery")),
    ];
    rebase(&mut settings, None, &mappings);
    settings["galleryDir"] = json!(target.join("Gallery").to_string_lossy());
    std::fs::write(
        stage.join("settings.json"),
        serde_json::to_vec_pretty(&settings).unwrap(),
    )?;
    if stage.join("library").is_dir() {
        rebase_records(&stage.join("library"), &mappings)?;
    }
    if stage.join("companion.json").is_file() {
        rebase_record(&stage.join("companion.json"), &mappings)?;
    }
    std::fs::write(
        stage.join("migration.json"),
        serde_json::to_vec_pretty(
            &json!({"version":1,"copiedExisting":settings_file.is_file(),"originalKept":true}),
        )
        .unwrap(),
    )?;
    if target.exists() {
        std::fs::remove_dir(target)?;
    }
    std::fs::rename(&stage, target)?;
    Ok(())
}
fn copy_tree(source: &Path, target: &Path) -> CoreResult<()> {
    let meta = std::fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() || is_reparse(&meta) {
        return Err(CoreError::new("The cooler library contains a linked folder. Import stopped without changing the original."));
    }
    if meta.is_dir() {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if meta.is_file() {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, target)?;
    }
    Ok(())
}
fn is_reparse(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}
fn mapped(value: &str, mappings: &[(PathBuf, PathBuf)]) -> Option<String> {
    let value_path = Path::new(value);
    // Longer roots win when the gallery is nested inside the old app folder.
    let mut roots = mappings.iter().collect::<Vec<_>>();
    roots.sort_by_key(|(root, _)| std::cmp::Reverse(root.as_os_str().len()));
    for (from, to) in roots {
        if let Ok(relative) = value_path.strip_prefix(from) {
            return Some(to.join(relative).to_string_lossy().into());
        }
        #[cfg(windows)]
        {
            let normalized = value.replace('/', "\\");
            let root = from.to_string_lossy().replace('/', "\\");
            if normalized
                .get(..root.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&root))
                && normalized
                    .as_bytes()
                    .get(root.len())
                    .is_none_or(|b| *b == b'\\')
            {
                return Some(
                    to.join(normalized[root.len()..].trim_start_matches('\\'))
                        .to_string_lossy()
                        .into(),
                );
            }
        }
    }
    None
}
fn rebase(value: &mut Value, key: Option<&str>, mappings: &[(PathBuf, PathBuf)]) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                rebase(value, Some(key), mappings);
            }
        }
        Value::Array(array) => {
            for value in array {
                rebase(value, key, mappings);
            }
        }
        Value::String(path) if matches!(key, Some("file" | "background" | "galleryDir")) => {
            if let Some(next) = mapped(path, mappings) {
                *path = next;
            }
        }
        _ => {}
    }
}
fn rebase_records(folder: &Path, mappings: &[(PathBuf, PathBuf)]) -> CoreResult<()> {
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            rebase_records(&path, mappings)?;
        } else if matches!(
            path.file_name().and_then(|s| s.to_str()),
            Some("original.json" | "version.json" | "companion.json" | "theme.json")
        ) {
            rebase_record(&path, mappings)?;
        }
    }
    Ok(())
}
fn rebase_record(path: &Path, mappings: &[(PathBuf, PathBuf)]) -> CoreResult<()> {
    let mut value: Value = serde_json::from_slice(&std::fs::read(path)?).map_err(|_| {
        CoreError::new("A cooler library record could not be read. The original data was kept.")
    })?;
    if path.file_name().is_some_and(|n| n == "original.json") {
        if let Some(file) = value["file"].as_str().filter(|s| !s.is_empty()) {
            if mapped(file, mappings).is_none() {
                return Err(CoreError::new("A cooler original points outside its saved library or gallery. Import stopped to preserve the fallback files."));
            }
        }
    }
    rebase(&mut value, None, mappings);
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v1_target(target: &Path) {
        std::fs::create_dir_all(target).unwrap();
        std::fs::write(
            target.join("migration.json"),
            r#"{"version":1,"copiedExisting":true,"originalKept":true}"#,
        )
        .unwrap();
    }

    #[test]
    fn source_upgrade_preserves_uc_edits_and_original_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old-plugins");
        let target = dir.path().join("uc/cooler");
        std::fs::create_dir_all(source.join("assets")).unwrap();
        std::fs::write(source.join("custom.json"), b"plugin").unwrap();
        std::fs::write(source.join("assets/icon.png"), b"icon").unwrap();
        v1_target(&target);
        std::fs::write(target.join("settings.json"), b"new UC settings").unwrap();
        std::fs::create_dir(target.join("library")).unwrap();
        std::fs::write(target.join("library/theme.json"), b"new UC theme").unwrap();
        upgrade_sources(&source, &target).unwrap();
        assert_eq!(
            std::fs::read(target.join("Sources/custom.json")).unwrap(),
            b"plugin"
        );
        assert_eq!(
            std::fs::read(target.join("Sources/assets/icon.png")).unwrap(),
            b"icon"
        );
        assert_eq!(
            std::fs::read(target.join("settings.json")).unwrap(),
            b"new UC settings"
        );
        assert_eq!(
            std::fs::read(target.join("library/theme.json")).unwrap(),
            b"new UC theme"
        );
        assert_eq!(
            std::fs::read(source.join("custom.json")).unwrap(),
            b"plugin"
        );
        let summary = migration_summary(&target).unwrap();
        assert_eq!(summary["version"], 2);
        assert_eq!(summary["sourcePlugins"]["copiedFilesAtImport"], 2);
        assert_eq!(summary["sourcePlugins"]["sourcePresentAtImport"], true);
        assert!(!summary
            .to_string()
            .contains(&dir.path().to_string_lossy().to_string()));
        // Neither later source edits nor re-enabling the cooler overwrite UC.
        std::fs::write(target.join("Sources/custom.json"), b"UC edit").unwrap();
        std::fs::write(source.join("custom.json"), b"old app edit").unwrap();
        upgrade_sources(&source, &target).unwrap();
        assert_eq!(
            std::fs::read(target.join("Sources/custom.json")).unwrap(),
            b"UC edit"
        );
    }

    #[test]
    fn source_conflicts_keep_both_copies_and_the_v1_marker() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old-plugins");
        let target = dir.path().join("uc/cooler");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("custom.json"), b"old").unwrap();
        v1_target(&target);
        std::fs::create_dir(target.join("Sources")).unwrap();
        std::fs::write(target.join("Sources/custom.json"), b"newer UC").unwrap();
        let marker = std::fs::read(target.join("migration.json")).unwrap();
        assert!(upgrade_sources(&source, &target)
            .unwrap_err()
            .code_is("cooler_migration_conflict"));
        assert_eq!(
            std::fs::read(target.join("migration.json")).unwrap(),
            marker
        );
        assert_eq!(
            std::fs::read(target.join("Sources/custom.json")).unwrap(),
            b"newer UC"
        );
        assert_eq!(std::fs::read(source.join("custom.json")).unwrap(), b"old");
        assert!(std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("cooler-sources-import-")));
    }

    #[test]
    fn identical_existing_sources_finish_an_interrupted_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old-plugins");
        let target = dir.path().join("uc/cooler");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("custom.json"), b"same").unwrap();
        v1_target(&target);
        copy_tree(&source, &target.join("Sources")).unwrap();
        upgrade_sources(&source, &target).unwrap();
        assert_eq!(migration_summary(&target).unwrap()["version"], 2);
        assert_eq!(
            std::fs::read(target.join("Sources/custom.json")).unwrap(),
            b"same"
        );
    }

    #[test]
    fn absent_old_sources_get_an_empty_owned_folder() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("uc/cooler");
        v1_target(&target);
        upgrade_sources(&dir.path().join("absent"), &target).unwrap();
        assert!(target.join("Sources").is_dir());
        let summary = migration_summary(&target).unwrap();
        assert_eq!(summary["sourcePlugins"]["sourcePresentAtImport"], false);
        assert_eq!(summary["sourcePlugins"]["copiedFilesAtImport"], 0);
        // An existing completed import must not require the old app folder.
        upgrade_sources(&dir.path().join("still-absent"), &target).unwrap();
    }

    #[test]
    fn invalid_markers_and_non_directory_sources_do_not_publish_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("uc/cooler");
        v1_target(&target);
        for marker in [
            r#"{"version":3,"originalKept":true}"#,
            "invalid",
            r#"{"version":1,"originalKept":true}"#,
            r#"{"version":2,"copiedExisting":true,"originalKept":true}"#,
        ] {
            std::fs::write(target.join("migration.json"), marker).unwrap();
            assert!(upgrade_sources(&dir.path().join("absent"), &target).is_err());
            assert!(!target.join("Sources").exists());
            assert_eq!(
                std::fs::read_to_string(target.join("migration.json")).unwrap(),
                marker
            );
        }
        v1_target(&target);
        let source = dir.path().join("not-a-folder");
        std::fs::write(&source, b"keep").unwrap();
        assert!(upgrade_sources(&source, &target).is_err());
        assert!(!target.join("Sources").exists());
        assert_eq!(std::fs::read(source).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn linked_source_entries_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("plugins");
        let target = dir.path().join("uc/cooler");
        v1_target(&target);
        std::fs::create_dir(&source).unwrap();
        let outside = dir.path().join("outside");
        std::fs::write(&outside, b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, source.join("linked.json")).unwrap();
        assert!(upgrade_sources(&source, &target).is_err());
        assert!(!target.join("Sources").exists());
        assert_eq!(std::fs::read(outside).unwrap(), b"keep");
    }
    #[test]
    fn copies_and_rebases_without_modifying_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old");
        let target = dir.path().join("uc/cooler");
        let gallery = dir.path().join("gallery");
        std::fs::create_dir_all(old.join("library/abcdefgh")).unwrap();
        std::fs::create_dir(&gallery).unwrap();
        std::fs::write(gallery.join("a.mp4"), b"original").unwrap();
        let settings = json!({"galleryDir":gallery,"mask":{"band":{"px":6}},"live":{"inApp":true}});
        std::fs::write(old.join("settings.json"), settings.to_string()).unwrap();
        let record = json!({"file":gallery.join("a.mp4"),"overlay":{"file":old.join("library/abcdefgh/overlay.png")},"name":"Keep"});
        std::fs::write(
            old.join("library/abcdefgh/original.json"),
            record.to_string(),
        )
        .unwrap();
        prepare(&old, &target, &gallery).unwrap();
        let imported: Value =
            serde_json::from_slice(&std::fs::read(target.join("settings.json")).unwrap()).unwrap();
        assert_eq!(imported["mask"], settings["mask"]);
        assert_eq!(imported["live"], settings["live"]);
        let imported: Value = serde_json::from_slice(
            &std::fs::read(target.join("library/abcdefgh/original.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            PathBuf::from(imported["file"].as_str().unwrap()),
            target.join("Gallery/a.mp4")
        );
        assert_eq!(
            PathBuf::from(imported["overlay"]["file"].as_str().unwrap()),
            target.join("library/abcdefgh/overlay.png")
        );
        assert_eq!(
            std::fs::read_to_string(old.join("library/abcdefgh/original.json")).unwrap(),
            record.to_string()
        );
        std::fs::write(target.join("Gallery/a.mp4"), b"edited").unwrap();
        assert_eq!(std::fs::read(gallery.join("a.mp4")).unwrap(), b"original");
        prepare(&old, &target, &gallery).unwrap();
        assert_eq!(
            std::fs::read(target.join("Gallery/a.mp4")).unwrap(),
            b"edited"
        );
    }
    #[test]
    fn broken_source_never_publishes_or_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old");
        let target = dir.path().join("uc/cooler");
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("settings.json"), b"broken").unwrap();
        assert!(prepare(&old, &target, &dir.path().join("gallery")).is_err());
        assert!(!target.exists());
        assert_eq!(std::fs::read(old.join("settings.json")).unwrap(), b"broken");
    }
}
