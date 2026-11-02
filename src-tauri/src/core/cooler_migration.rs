//! Copy-once migration. The old app data and original gallery remain untouched.
use super::error::{CoreError, CoreResult};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

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
