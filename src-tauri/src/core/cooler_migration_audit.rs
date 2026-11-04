//! Read-only comparison performed by UC, in its own real data namespace.
//! Reports contain only finite counts and flags, never record names or paths.
use super::error::{CoreError, CoreResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 32;
const MAX_JSON: u64 = 8 * 1024 * 1024;
const MAX_JSON_TOTAL: u64 = 64 * 1024 * 1024;
const MAX_COMPARE_BYTES: u64 = 32 * 1024 * 1024 * 1024;

fn failure(code: &str) -> CoreError {
    CoreError::coded(
        code,
        "The cooler migration comparison could not be completed safely.",
    )
}
fn io<T>(result: std::io::Result<T>) -> CoreResult<T> {
    result.map_err(|_| failure("cooler_audit_unreadable"))
}
fn linked(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        return meta.file_attributes() & 0x400 != 0;
    }
    #[cfg(not(windows))]
    false
}
fn safe_meta(path: &Path) -> CoreResult<Option<Metadata>> {
    // A regular leaf reached through a junction is not an independent copy.
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if linked(&meta) => return Err(failure("cooler_audit_linked")),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(failure("cooler_audit_unreadable")),
        }
    }
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(failure("cooler_audit_unreadable")),
    }
}
#[derive(Clone, PartialEq)]
struct Entry {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}
impl Entry {
    fn new(path: PathBuf, meta: &Metadata) -> Self {
        Self {
            path,
            len: meta.len(),
            modified: meta.modified().ok(),
        }
    }
    fn unchanged(&self) -> CoreResult<bool> {
        Ok(safe_meta(&self.path)?.is_some_and(|meta| {
            meta.is_file() && meta.len() == self.len && meta.modified().ok() == self.modified
        }))
    }
}
type Manifest = BTreeMap<PathBuf, Entry>;
#[derive(Default)]
struct Budget<'a> {
    files: usize,
    json: u64,
    compared: u64,
    cancel: Option<&'a dyn Fn() -> CoreResult<()>>,
}
impl Budget<'_> {
    fn check(&self) -> CoreResult<()> {
        self.cancel.map_or(Ok(()), |check| check())
    }
}
fn manifest(root: &Path, budget: &mut Budget) -> CoreResult<Manifest> {
    fn walk(
        root: &Path,
        folder: &Path,
        depth: usize,
        budget: &mut Budget,
        out: &mut Manifest,
    ) -> CoreResult<()> {
        budget.check()?;
        if depth > MAX_DEPTH {
            return Err(failure("cooler_audit_limit"));
        }
        for item in io(fs::read_dir(folder))? {
            budget.check()?;
            let path = io(item)?.path();
            let meta = safe_meta(&path)?.ok_or_else(|| failure("cooler_audit_changed"))?;
            if meta.is_dir() {
                walk(root, &path, depth + 1, budget, out)?;
            } else if meta.is_file() {
                budget.files += 1;
                if budget.files > MAX_FILES {
                    return Err(failure("cooler_audit_limit"));
                }
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| failure("cooler_audit_invalid"))?
                    .to_path_buf();
                out.insert(relative, Entry::new(path, &meta));
            } else {
                return Err(failure("cooler_audit_invalid"));
            }
        }
        Ok(())
    }
    let mut result = Manifest::new();
    if let Some(meta) = safe_meta(root)? {
        if !meta.is_dir() {
            return Err(failure("cooler_audit_invalid"));
        }
        walk(root, root, 0, budget, &mut result)?;
    }
    Ok(result)
}
fn read_json(path: &Path, budget: &mut Budget) -> CoreResult<Option<Value>> {
    budget.check()?;
    let Some(meta) = safe_meta(path)? else {
        return Ok(None);
    };
    if !meta.is_file() {
        return Err(failure("cooler_audit_invalid"));
    }
    if meta.len() > MAX_JSON || budget.json.saturating_add(meta.len()) > MAX_JSON_TOTAL {
        return Err(failure("cooler_audit_limit"));
    }
    budget.json += meta.len();
    let mut bytes = Vec::new();
    let mut file = io(File::open(path))?.take(MAX_JSON + 1);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        budget.check()?;
        let count = io(file.read(&mut chunk))?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    if bytes.len() as u64 > MAX_JSON {
        return Err(failure("cooler_audit_limit"));
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    budget.check()?;
    Ok(serde_json::from_slice(bytes).ok())
}
fn same_bytes(a: &Entry, b: &Entry, budget: &mut Budget) -> CoreResult<bool> {
    budget.check()?;
    if a.len != b.len {
        return Ok(false);
    }
    budget.compared = budget.compared.saturating_add(a.len.saturating_mul(2));
    if budget.compared > MAX_COMPARE_BYTES {
        return Err(failure("cooler_audit_limit"));
    }
    let mut left = io(File::open(&a.path))?;
    let mut right = io(File::open(&b.path))?;
    let (mut x, mut y) = ([0u8; 64 * 1024], [0u8; 64 * 1024]);
    // read_exact per known remaining length avoids false differences from short reads.
    let mut remaining = a.len;
    while remaining != 0 {
        budget.check()?;
        let count = remaining.min(x.len() as u64) as usize;
        io(left.read_exact(&mut x[..count]))?;
        io(right.read_exact(&mut y[..count]))?;
        if x[..count] != y[..count] {
            return Ok(false);
        }
        remaining -= count as u64;
    }
    Ok(true)
}

fn normalized(path: &str) -> String {
    let mut text = path.replace('\\', "/");
    if let Some(rest) = text.strip_prefix("//?/") {
        text = rest.to_string();
    }
    let insensitive =
        cfg!(windows) || text.as_bytes().get(1) == Some(&b':') || text.starts_with("//");
    let leading = text.starts_with('/');
    let mut pieces = Vec::new();
    for piece in text.split('/') {
        match piece {
            "" | "." => {}
            ".." if pieces.last().is_some_and(|p| *p != "..") => {
                pieces.pop();
            }
            _ => pieces.push(piece),
        }
    }
    let mut result = format!("{}{}", if leading { "/" } else { "" }, pieces.join("/"));
    if insensitive {
        result.make_ascii_lowercase();
    }
    result
}
fn inside(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}
fn path_like(text: &str) -> bool {
    text.contains('/') || text.contains('\\') || Path::new(text).extension().is_some()
}
fn normalize_record(value: &mut Value, key: Option<&str>, mappings: &[(String, String)]) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                normalize_record(value, Some(key), mappings);
            }
        }
        Value::Array(array) => {
            for value in array {
                normalize_record(value, key, mappings);
            }
        }
        Value::String(text)
            if matches!(key, Some("file" | "background" | "galleryDir")) && path_like(text) =>
        {
            let mut path = normalized(text);
            for (from, to) in mappings {
                if inside(&path, from) {
                    path = format!("{to}{}", &path[from.len()..]);
                    break;
                }
            }
            *text = path;
        }
        _ => {}
    }
}
fn runtime_record(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("original.json" | "version.json" | "theme.json" | "companion.json")
    )
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct References {
    checked: u64,
    old_dependencies: u64,
    missing: u64,
    external: u64,
}
struct Roots {
    old: String,
    gallery: String,
    target: String,
}
fn check_reference(
    text: &str,
    base: &Path,
    directory: bool,
    roots: &Roots,
    refs: &mut References,
) -> CoreResult<()> {
    if text.is_empty() {
        return Ok(());
    }
    let path = if Path::new(text).is_absolute() || text.as_bytes().get(1) == Some(&b':') {
        PathBuf::from(text)
    } else {
        base.join(text)
    };
    let norm = normalized(&path.to_string_lossy());
    refs.checked += 1;
    if inside(&norm, &roots.old) || inside(&norm, &roots.gallery) {
        refs.old_dependencies += 1;
    } else if !inside(&norm, &roots.target) {
        refs.external += 1;
    }
    if !safe_meta(&path)?.is_some_and(|meta| {
        if directory {
            meta.is_dir()
        } else {
            meta.is_file()
        }
    }) {
        refs.missing += 1;
    }
    Ok(())
}
fn references(
    value: &Value,
    key: Option<&str>,
    base: &Path,
    roots: &Roots,
    refs: &mut References,
) -> CoreResult<()> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                references(value, Some(key), base, roots, refs)?;
            }
        }
        Value::Array(array) => {
            for value in array {
                references(value, key, base, roots, refs)?;
            }
        }
        Value::String(text)
            if key == Some("file") || (key == Some("background") && path_like(text)) =>
        {
            check_reference(text, base, false, roots, refs)?
        }
        _ => {}
    }
    Ok(())
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    source_files: u64,
    target_files: u64,
    source_bytes: u64,
    target_bytes: u64,
    missing_files: u64,
    additional_files: u64,
    identical_files: u64,
    changed_records: u64,
    changed_media_files: u64,
    originals: u64,
    themes: u64,
    versions: u64,
}
#[derive(Default)]
struct Issues {
    unreadable: u64,
    unstable: u64,
}
fn compare_tree(
    source: &Manifest,
    target: &Manifest,
    runtime: bool,
    mappings: &[(String, String)],
    roots: &Roots,
    budget: &mut Budget,
    issues: &mut Issues,
    refs: &mut References,
) -> CoreResult<Counts> {
    let mut counts = Counts {
        source_files: source.len() as u64,
        target_files: target.len() as u64,
        source_bytes: source
            .values()
            .fold(0u64, |total, entry| total.saturating_add(entry.len)),
        target_bytes: target
            .values()
            .fold(0u64, |total, entry| total.saturating_add(entry.len)),
        ..Counts::default()
    };
    let mut target_json = BTreeMap::new();
    for (relative, entry) in target {
        budget.check()?;
        if entry
            .path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            if let Some(value) = read_json(&entry.path, budget)? {
                if runtime && runtime_record(relative) {
                    if !value.is_object() {
                        issues.unreadable += 1;
                    } else {
                        match relative.file_name().and_then(|v| v.to_str()) {
                            Some("original.json") => counts.originals += 1,
                            Some("theme.json") => counts.themes += 1,
                            Some("version.json") => counts.versions += 1,
                            _ => {}
                        }
                        references(
                            &value,
                            None,
                            entry.path.parent().unwrap_or(Path::new("")),
                            roots,
                            refs,
                        )?;
                    }
                }
                target_json.insert(relative, value);
            } else {
                issues.unreadable += 1;
            }
        }
    }
    for (relative, left) in source {
        budget.check()?;
        let Some(right) = target.get(relative) else {
            counts.missing_files += 1;
            continue;
        };
        if left
            .path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            if let (Some(mut a), Some(b)) =
                (read_json(&left.path, budget)?, target_json.get(relative))
            {
                let mut b = b.clone();
                if runtime && runtime_record(relative) {
                    if !a.is_object() {
                        issues.unreadable += 1;
                    }
                    normalize_record(&mut a, None, mappings);
                    normalize_record(&mut b, None, &[]);
                }
                if a == b {
                    counts.identical_files += 1;
                } else {
                    counts.changed_records += 1;
                }
            } else {
                issues.unreadable += 1;
            }
        } else if same_bytes(left, right, budget)? {
            counts.identical_files += 1;
        } else {
            counts.changed_media_files += 1;
        }
    }
    counts.additional_files = target
        .keys()
        .filter(|key| !source.contains_key(*key))
        .count() as u64;
    for entry in source.values().chain(target.values()) {
        budget.check()?;
        if !entry.unchanged()? {
            issues.unstable += 1;
        }
    }
    Ok(counts)
}

/// Compare the original fallback and UC's independent copy without changing either.
/// Metadata differences represent preserved UC edits, not automatic overwrite requests.
pub fn audit(source: &Path, target: &Path, default_gallery: &Path) -> CoreResult<Value> {
    audit_with_cancel(source, target, default_gallery, || Ok(()))
}
/// Cancellation is checked between files and every 64KiB read/compare chunk.
pub fn audit_with_cancel(
    source: &Path,
    target: &Path,
    default_gallery: &Path,
    check: impl Fn() -> CoreResult<()>,
) -> CoreResult<Value> {
    check()?;
    if [source, target, default_gallery]
        .iter()
        .any(|path| !path.is_absolute())
    {
        return Err(failure("cooler_audit_invalid"));
    }
    let mut budget = Budget {
        cancel: Some(&check),
        ..Budget::default()
    };
    let root_files = [
        source.join("settings.json"),
        target.join("settings.json"),
        source.join("companion.json"),
        target.join("companion.json"),
        target.join("migration.json"),
        source.join("lyrics/offsets.json"),
        target.join("lyrics/offsets.json"),
    ]
    .into_iter()
    .map(|path| {
        let meta = safe_meta(&path)?;
        Ok((
            path.clone(),
            meta.as_ref().map(|meta| Entry::new(path, meta)),
        ))
    })
    .collect::<CoreResult<Vec<_>>>()?;
    let source_present = safe_meta(&source.join("settings.json"))?.is_some();
    let target_present = safe_meta(&target.join("settings.json"))?.is_some();
    let mut issues = Issues::default();
    let mut source_settings = read_json(&source.join("settings.json"), &mut budget)?
        .unwrap_or_else(|| {
            issues.unreadable += u64::from(source_present);
            json!({})
        });
    let mut target_settings = read_json(&target.join("settings.json"), &mut budget)?
        .unwrap_or_else(|| {
            issues.unreadable += u64::from(target_present);
            json!({})
        });
    if !source_settings.is_object() {
        issues.unreadable += 1;
        source_settings = json!({});
    }
    if !target_settings.is_object() {
        issues.unreadable += 1;
        target_settings = json!({});
    }
    let source_gallery = source_settings["galleryDir"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_gallery.into());
    let target_gallery = target_settings["galleryDir"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| target.join("Gallery"));
    if !source_gallery.is_absolute() || !target_gallery.is_absolute() {
        return Err(failure("cooler_audit_invalid"));
    }
    let roots = Roots {
        old: normalized(&source.to_string_lossy()),
        gallery: normalized(&source_gallery.to_string_lossy()),
        target: normalized(&target.to_string_lossy()),
    };
    let mut mappings = vec![
        (roots.old.clone(), roots.target.clone()),
        (
            roots.gallery.clone(),
            normalized(&target_gallery.to_string_lossy()),
        ),
    ];
    mappings.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    let old_library = manifest(&source.join("library"), &mut budget)?;
    let new_library = manifest(&target.join("library"), &mut budget)?;
    let old_gallery = manifest(&source_gallery, &mut budget)?;
    let new_gallery = manifest(&target_gallery, &mut budget)?;
    let mut refs = References::default();
    let library = compare_tree(
        &old_library,
        &new_library,
        true,
        &mappings,
        &roots,
        &mut budget,
        &mut issues,
        &mut refs,
    )?;
    let gallery = compare_tree(
        &old_gallery,
        &new_gallery,
        false,
        &mappings,
        &roots,
        &mut budget,
        &mut issues,
        &mut refs,
    )?;
    references(&target_settings, None, target, &roots, &mut refs)?;
    check_reference(
        &target_gallery.to_string_lossy(),
        target,
        true,
        &roots,
        &mut refs,
    )?;
    let mut companion_changed = 0;
    if safe_meta(&source.join("companion.json"))?.is_some() {
        let a = read_json(&source.join("companion.json"), &mut budget)?;
        let b = read_json(&target.join("companion.json"), &mut budget)?;
        if let (Some(mut a), Some(mut b)) = (a, b) {
            references(&b, None, target, &roots, &mut refs)?;
            normalize_record(&mut a, None, &mappings);
            normalize_record(&mut b, None, &[]);
            companion_changed = u64::from(a != b);
        } else {
            issues.unreadable += 1;
        }
    }
    let offsets_source_present = safe_meta(&source.join("lyrics/offsets.json"))?.is_some();
    let offsets_target_present = safe_meta(&target.join("lyrics/offsets.json"))?.is_some();
    let mut offsets = Vec::new();
    for (root, present) in [
        (source, offsets_source_present),
        (target, offsets_target_present),
    ] {
        budget.check()?;
        let value = read_json(&root.join("lyrics/offsets.json"), &mut budget)?;
        if present && !value.as_ref().is_some_and(Value::is_object) {
            issues.unreadable += 1;
        }
        offsets.push(value);
    }
    let lyrics_offsets = json!({"sourcePresent":offsets_source_present,"targetPresent":offsets_target_present,
        "changed":u64::from(offsets[0] != offsets[1])});
    source_settings["galleryDir"] = json!(source_gallery.to_string_lossy());
    target_settings["galleryDir"] = json!(target_gallery.to_string_lossy());
    for settings in [&mut source_settings, &mut target_settings] {
        for name in ["capacity", "loopRoom"] {
            if let Some(object) = settings[name].as_object_mut() {
                object.remove("at");
            }
        }
    }
    normalize_record(&mut source_settings, None, &mappings);
    normalize_record(&mut target_settings, None, &[]);
    let changed_sections = match (source_settings.as_object(), target_settings.as_object()) {
        (Some(a), Some(b)) => a
            .keys()
            .chain(b.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|key| a.get(*key) != b.get(*key))
            .count() as u64,
        _ => 0,
    };
    let marker = read_json(&target.join("migration.json"), &mut budget)?.unwrap_or(Value::Null);
    let version = marker["version"].as_u64().unwrap_or(0);
    let original_kept = marker["originalKept"] == true;
    let marker_valid =
        [1, 2].contains(&version) && original_kept && marker["copiedExisting"] == true;
    // Check tree identity once more: additions/deletions after enumeration are unstable too.
    let mut final_budget = Budget {
        cancel: Some(&check),
        ..Budget::default()
    };
    for (root, before) in [
        (&source.join("library"), &old_library),
        (&target.join("library"), &new_library),
        (&source_gallery, &old_gallery),
        (&target_gallery, &new_gallery),
    ] {
        budget.check()?;
        if &manifest(root, &mut final_budget)? != before {
            issues.unstable += 1;
        }
    }
    for (path, before) in root_files {
        budget.check()?;
        if match before {
            Some(entry) => !entry.unchanged()?,
            None => safe_meta(&path)?.is_some(),
        } {
            issues.unstable += 1;
        }
    }
    let verified = source_present
        && target_present
        && marker_valid
        && (!offsets_source_present || offsets_target_present)
        && issues.unreadable == 0
        && issues.unstable == 0
        && library.missing_files
            + gallery.missing_files
            + library.changed_media_files
            + gallery.changed_media_files
            == 0
        && refs.old_dependencies == 0
        && refs.missing == 0;
    let code = if !source_present {
        "source_missing"
    } else if issues.unstable != 0 {
        "changed_during_audit"
    } else if verified {
        "verified"
    } else {
        "incomplete"
    };
    Ok(
        json!({"verified":verified,"code":code,"migrationVersion":version,"originalKept":original_kept,
        "library":library,"gallery":gallery,"settings":{"sourcePresent":source_present,"targetPresent":target_present,"changedSections":changed_sections},
        "lyricsOffsets":lyrics_offsets,"companionChanged":companion_changed,"references":refs,"unreadableRecords":issues.unreadable,"unstableFiles":issues.unstable}),
    )
}

const MAX_RECEIPT: u64 = 64 * 1024;
const MAX_SAFE_COUNT: u64 = 9_007_199_254_740_991;

fn receipt_error(code: &str) -> CoreError {
    CoreError::coded(
        code,
        "The cooler verification receipt could not be used safely.",
    )
}
fn receipt_meta(path: &Path) -> CoreResult<Option<Metadata>> {
    safe_meta(path).map_err(|_| receipt_error("cooler_receipt_unsafe"))
}
fn receipt_location(path: &Path) -> CoreResult<()> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(receipt_error("cooler_receipt_invalid"));
    }
    Ok(())
}
fn regular_receipt(path: &Path) -> CoreResult<Option<Metadata>> {
    let meta = receipt_meta(path)?;
    if meta.as_ref().is_some_and(|m| !m.is_file()) {
        return Err(receipt_error("cooler_receipt_unsafe"));
    }
    Ok(meta)
}
fn valid_receipt_time(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 24
        || !b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'.',
            23 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
    {
        return false;
    }
    let number = |a: usize, z: usize| text[a..z].parse::<u32>().unwrap_or(u32::MAX);
    let (y, m, d) = (number(0, 4), number(5, 7), number(8, 10));
    let days = match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    y > 0 && d > 0 && d <= days && number(11, 13) < 24 && number(14, 16) < 60 && number(17, 19) < 60
}
fn receipt_count(value: &Value) -> CoreResult<Value> {
    value
        .as_u64()
        .filter(|n| *n <= MAX_SAFE_COUNT)
        .map(Value::from)
        .ok_or_else(|| receipt_error("cooler_receipt_invalid"))
}
fn receipt_flag(value: &Value) -> CoreResult<Value> {
    value
        .as_bool()
        .map(Value::from)
        .ok_or_else(|| receipt_error("cooler_receipt_invalid"))
}
fn project_counts(value: &Value, keys: &[&str], required: bool) -> CoreResult<Value> {
    if !value.is_object() {
        return Err(receipt_error("cooler_receipt_invalid"));
    }
    let mut out = serde_json::Map::new();
    for key in keys {
        if let Some(v) = value.get(*key) {
            out.insert((*key).into(), receipt_count(v)?);
        } else if required {
            return Err(receipt_error("cooler_receipt_invalid"));
        }
    }
    Ok(Value::Object(out))
}
fn project_receipt(value: &Value) -> CoreResult<Value> {
    let invalid = || receipt_error("cooler_receipt_invalid");
    if value["reportVersion"].as_u64() != Some(1) {
        return Err(invalid());
    }
    let state = value["state"].as_str().ok_or_else(invalid)?;
    let code = value["code"].as_str().ok_or_else(invalid)?;
    let version = value["migrationVersion"]
        .as_u64()
        .filter(|n| *n <= 2)
        .ok_or_else(invalid)?;
    let time = value["checkedAt"]
        .as_str()
        .filter(|s| valid_receipt_time(s))
        .ok_or_else(invalid)?;
    let verified = value["verified"].as_bool().ok_or_else(invalid)?;
    let complete = state == "complete";
    match state {
        "complete"
            if version == 2 && verified && code == "verified" && value["originalKept"] == true => {}
        "not_required" if version == 2 && !verified && code == "new_setup" => {}
        "attention"
            if !verified
                && matches!(
                    code,
                    "audit_failed"
                        | "receipt_write_failed"
                        | "migration_setup_failed"
                        | "incomplete"
                        | "changed_during_audit"
                        | "source_missing"
                        | "verification_cancelled"
                        | "verification_interrupted"
                        | "cancelled"
                        | "sources_pending"
                ) => {}
        _ => return Err(invalid()),
    }
    let mut out = json!({"reportVersion":1,"state":state,"code":code,"migrationVersion":version,"checkedAt":time,"verified":verified});
    if let Some(v) = value.get("originalKept") {
        out["originalKept"] = receipt_flag(v)?;
    }
    let files = [
        "sourceFiles",
        "targetFiles",
        "sourceBytes",
        "targetBytes",
        "missingFiles",
        "additionalFiles",
        "identicalFiles",
        "changedRecords",
        "changedMediaFiles",
        "originals",
        "themes",
        "versions",
    ];
    for key in ["library", "gallery"] {
        if let Some(v) = value.get(key) {
            let counts = project_counts(v, &files, complete)?;
            if complete {
                let n = |k: &str| counts[k].as_u64().unwrap();
                if n("missingFiles") != 0
                    || n("changedMediaFiles") != 0
                    || n("identicalFiles") + n("changedRecords") != n("sourceFiles")
                    || n("sourceFiles") + n("additionalFiles") != n("targetFiles")
                    || n("originals") + n("themes") + n("versions") > n("targetFiles")
                {
                    return Err(invalid());
                }
            }
            out[key] = counts;
        } else if complete {
            return Err(invalid());
        }
    }
    if let Some(v) = value.get("references") {
        let counts = project_counts(
            v,
            &["checked", "oldDependencies", "missing", "external"],
            complete,
        )?;
        if complete
            && (counts["oldDependencies"] != 0
                || counts["missing"] != 0
                || counts["external"].as_u64() > counts["checked"].as_u64())
        {
            return Err(invalid());
        }
        out["references"] = counts;
    } else if complete {
        return Err(invalid());
    }
    for key in ["unreadableRecords", "unstableFiles", "companionChanged"] {
        if let Some(v) = value.get(key) {
            let count = receipt_count(v)?;
            if (complete && key != "companionChanged" && count != 0)
                || (key == "companionChanged" && count.as_u64().unwrap() > 1)
            {
                return Err(invalid());
            }
            out[key] = count;
        } else if complete {
            return Err(invalid());
        }
    }
    for (key, flags, counts) in [
        (
            "settings",
            &["sourcePresent", "targetPresent"][..],
            &["changedSections"][..],
        ),
        (
            "sourcePlugins",
            &["imported", "folderPresent"][..],
            &["copiedFilesAtImport"][..],
        ),
        (
            "lyricsOffsets",
            &["sourcePresent", "targetPresent"][..],
            &["changed"][..],
        ),
    ] {
        if let Some(v) = value.get(key) {
            let mut group = project_counts(v, counts, complete)?;
            for flag in flags {
                if let Some(f) = v.get(*flag) {
                    group[*flag] = receipt_flag(f)?;
                    if complete && key != "lyricsOffsets" && f != true {
                        return Err(invalid());
                    }
                } else if complete {
                    return Err(invalid());
                }
            }
            if key == "sourcePlugins" {
                if let Some(f) = v.get("sourcePresentAtImport") {
                    group["sourcePresentAtImport"] = receipt_flag(f)?;
                }
            }
            if key == "lyricsOffsets"
                && (group
                    .get("changed")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| n > 1)
                    || (complete
                        && group["sourcePresent"] == true
                        && group["targetPresent"] != true))
            {
                return Err(invalid());
            }
            out[key] = group;
        } else if complete {
            return Err(invalid());
        }
    }
    Ok(out)
}

/// Bounded, projected historical summary. A pending check invalidates older results.
pub fn read_receipt(path: &Path) -> CoreResult<Option<Value>> {
    receipt_location(path)?;
    let pending = path.with_extension("pending");
    if regular_receipt(&pending)?.is_some() {
        return Ok(None);
    }
    let Some(meta) = regular_receipt(path)? else {
        return Ok(None);
    };
    if meta.len() > MAX_RECEIPT {
        return Err(receipt_error("cooler_receipt_limit"));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| receipt_error("cooler_receipt_unreadable"))?
        .take(MAX_RECEIPT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| receipt_error("cooler_receipt_unreadable"))?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return Err(receipt_error("cooler_receipt_limit"));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| receipt_error("cooler_receipt_invalid"))?;
    let result = project_receipt(&value)?;
    regular_receipt(path)?;
    if regular_receipt(&pending)?.is_some() {
        return Ok(None);
    }
    Ok(Some(result))
}
struct ReceiptTemp(Option<PathBuf>);
impl Drop for ReceiptTemp {
    fn drop(&mut self) {
        if let Some(path) = self.0.as_ref() {
            if regular_receipt(path).ok().flatten().is_some() {
                let _ = fs::remove_file(path);
            }
        }
    }
}
fn atomic_receipt(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    receipt_location(path)?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return Err(receipt_error("cooler_receipt_limit"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| receipt_error("cooler_receipt_invalid"))?;
    if !receipt_meta(parent)?.is_some_and(|m| m.is_dir()) {
        return Err(receipt_error("cooler_receipt_unsafe"));
    }
    regular_receipt(path)?;
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    for _ in 0..16 {
        let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".uc-migration-receipt-{}-{serial}.tmp",
            std::process::id()
        ));
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(receipt_error("cooler_receipt_write_failed")),
        };
        let mut cleanup = ReceiptTemp(Some(temporary.clone()));
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| receipt_error("cooler_receipt_write_failed"))?;
        drop(file);
        regular_receipt(path)?;
        regular_receipt(&temporary)?;
        fs::rename(&temporary, path).map_err(|_| receipt_error("cooler_receipt_write_failed"))?;
        cleanup.0 = None;
        return Ok(());
    }
    Err(receipt_error("cooler_receipt_write_failed"))
}
/// Publish before queue admission. Cancellation deliberately leaves this marker.
pub fn begin_receipt(path: &Path) -> CoreResult<()> {
    receipt_location(path)?;
    regular_receipt(path)?;
    atomic_receipt(&path.with_extension("pending"), b"verification pending\n")
}
/// Atomically publish only known finite fields, then clear the pending marker.
pub fn write_receipt(path: &Path, report: &Value) -> CoreResult<()> {
    let projected = project_receipt(report)?;
    let bytes =
        serde_json::to_vec(&projected).map_err(|_| receipt_error("cooler_receipt_invalid"))?;
    let pending = path.with_extension("pending");
    regular_receipt(&pending)?;
    atomic_receipt(path, &bytes)?;
    if regular_receipt(&pending)?.is_some() {
        fs::remove_file(pending).map_err(|_| receipt_error("cooler_receipt_write_failed"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn complete_receipt(o: &Path, n: &Path, g: &Path) -> Value {
        let mut value = audit(o, n, g).unwrap();
        value["reportVersion"] = json!(1);
        value["state"] = json!("complete");
        value["checkedAt"] = json!("2026-11-03T12:34:56.789Z");
        value["sourcePlugins"] =
            json!({"imported":true,"folderPresent":true,"copiedFilesAtImport":0});
        value
    }
    #[test]
    fn cancellation_propagates_between_compare_chunks_and_before_audit() {
        let (_d, o, n, g) = fixture();
        let cancelled = || Err(CoreError::coded("cancelled", "Cancelled"));
        assert_eq!(
            audit_with_cancel(&o, &n, &g, cancelled)
                .unwrap_err()
                .code
                .as_deref(),
            Some("cancelled")
        );
        let a = o.join("large.bin");
        let b = n.join("large.bin");
        put(&a, vec![7; 256 * 1024]);
        put(&b, vec![7; 256 * 1024]);
        let left = Entry::new(a.clone(), &fs::metadata(&a).unwrap());
        let right = Entry::new(b.clone(), &fs::metadata(&b).unwrap());
        let checks = std::cell::Cell::new(0);
        let check = || {
            checks.set(checks.get() + 1);
            if checks.get() == 3 {
                cancelled()
            } else {
                Ok(())
            }
        };
        let mut budget = Budget {
            cancel: Some(&check),
            ..Budget::default()
        };
        assert_eq!(
            same_bytes(&left, &right, &mut budget)
                .unwrap_err()
                .code
                .as_deref(),
            Some("cancelled")
        );
        assert_eq!(checks.get(), 3);
    }
    #[test]
    fn lyrics_offsets_preserve_edits_and_require_present_copy() {
        let (d, o, n, g) = fixture();
        put(&o.join("lyrics/offsets.json"), br#"{"track":100}"#);
        let missing = audit(&o, &n, &g).unwrap();
        assert_eq!(missing["verified"], false);
        assert_eq!(missing["lyricsOffsets"]["targetPresent"], false);
        put(&n.join("lyrics/offsets.json"), br#"{"track":200}"#);
        let value = complete_receipt(&o, &n, &g);
        assert_eq!(value["verified"], true);
        assert_eq!(value["lyricsOffsets"]["changed"], 1);
        let path = d.path().join("receipt.json");
        write_receipt(&path, &value).unwrap();
        assert_eq!(
            read_receipt(&path).unwrap().unwrap()["lyricsOffsets"],
            value["lyricsOffsets"]
        );
        for bad in [
            Value::Null,
            json!({"sourcePresent":true,"targetPresent":false,"changed":1}),
            json!({"sourcePresent":true,"targetPresent":true,"changed":2}),
        ] {
            let mut invalid = value.clone();
            invalid["lyricsOffsets"] = bad;
            assert!(write_receipt(&path, &invalid).is_err());
        }
        put(&n.join("lyrics/offsets.json"), b"{");
        let broken = audit(&o, &n, &g).unwrap();
        assert_eq!(broken["verified"], false);
        assert_eq!(broken["unreadableRecords"], 1);
    }
    #[test]
    fn receipt_projects_complete_and_pending_prevents_old_success() {
        let (d, o, n, g) = fixture();
        let path = d.path().join("receipt.json");
        let mut value = complete_receipt(&o, &n, &g);
        value["private"] = json!("do not persist");
        value["library"]["private"] = json!("do not persist");
        write_receipt(&path, &value).unwrap();
        let original = fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&original).contains("private"));
        assert_eq!(read_receipt(&path).unwrap().unwrap()["verified"], true);
        begin_receipt(&path).unwrap();
        assert!(read_receipt(&path).unwrap().is_none());
        value["migrationVersion"] = json!(1);
        assert!(write_receipt(&path, &value).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(path.with_extension("pending").is_file());
        let failure = json!({"reportVersion":1,"state":"attention","checkedAt":"2026-11-03T12:34:56.789Z","verified":false,"migrationVersion":1,"code":"verification_cancelled"});
        write_receipt(&path, &failure).unwrap();
        assert!(!path.with_extension("pending").exists());
        assert_eq!(read_receipt(&path).unwrap().unwrap()["state"], "attention");
        assert!(!fs::read_dir(d.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }
    #[test]
    fn receipt_rejects_invalid_success_and_unbounded_input() {
        let (d, o, n, g) = fixture();
        let path = d.path().join("receipt.json");
        let valid = complete_receipt(&o, &n, &g);
        for (key, bad) in [
            ("checkedAt", json!("2026-02-30T12:34:56.789Z")),
            ("library", json!({})),
            ("sourcePlugins", json!({"imported":false})),
            ("unstableFiles", json!(1)),
            ("unreadableRecords", json!(MAX_SAFE_COUNT + 1)),
        ] {
            let mut value = valid.clone();
            value[key] = bad;
            assert!(write_receipt(&path, &value).is_err(), "{key}");
            fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            assert!(read_receipt(&path).is_err(), "{key}");
        }
        File::create(&path)
            .unwrap()
            .set_len(MAX_RECEIPT + 1)
            .unwrap();
        assert!(read_receipt(&path).is_err());
        assert!(atomic_receipt(&path, &vec![0; MAX_RECEIPT as usize + 1]).is_err());
        fs::write(&path, b"{").unwrap();
        assert!(read_receipt(&path).is_err());
        let fresh = json!({"reportVersion":1,"state":"not_required","checkedAt":"2026-11-03T12:34:56.789Z","verified":false,"migrationVersion":2,"code":"new_setup","originalKept":true});
        write_receipt(&path, &fresh).unwrap();
        assert_eq!(read_receipt(&path).unwrap().unwrap()["code"], "new_setup");
    }
    #[test]
    fn receipt_publish_failure_keeps_pending_and_cleans_owned_temporary() {
        let (d, o, n, g) = fixture();
        let path = d.path().join("receipt.json");
        let valid = complete_receipt(&o, &n, &g);
        write_receipt(&path, &valid).unwrap();
        begin_receipt(&path).unwrap();
        let saved = d.path().join("saved.json");
        fs::rename(&path, &saved).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(write_receipt(&path, &valid).is_err());
        assert!(read_receipt(&path).unwrap().is_none());
        fs::remove_dir(&path).unwrap();
        fs::rename(&saved, &path).unwrap();
        assert!(read_receipt(&path).unwrap().is_none());
        write_receipt(&path, &valid).unwrap();
        assert!(read_receipt(&path).unwrap().is_some());
    }
    #[cfg(unix)]
    #[test]
    fn receipts_reject_linked_leaf_and_parent() {
        use std::os::unix::fs::symlink;
        let (d, o, n, g) = fixture();
        let real = d.path().join("real.json");
        write_receipt(&real, &complete_receipt(&o, &n, &g)).unwrap();
        let link = d.path().join("link.json");
        symlink(&real, &link).unwrap();
        assert!(read_receipt(&link).is_err());
        assert!(begin_receipt(&link).is_err());
        let folder = d.path().join("linked-parent");
        symlink(d.path(), &folder).unwrap();
        assert!(read_receipt(&folder.join("real.json")).is_err());
        assert!(write_receipt(&folder.join("other.json"), &complete_receipt(&o, &n, &g)).is_err());
    }
    fn put(path: &Path, bytes: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("original");
        let new = dir.path().join("uc");
        let gallery = dir.path().join("Gallery");
        put(&old.join("settings.json"), br#"{"galleryDir":"","capacity":{"bytes":4,"at":"old"},"sources":{"external":{"folders":["user-media"]}}}"#);
        put(&new.join("settings.json"), serde_json::to_vec(&json!({"galleryDir":new.join("Gallery"),"capacity":{"bytes":4,"at":"new"},"sources":{"external":{"folders":["user-media"]}}})).unwrap());
        put(
            &new.join("migration.json"),
            br#"{"version":2,"copiedExisting":true,"originalKept":true}"#,
        );
        put(&gallery.join("clip.bin"), b"media");
        put(&new.join("Gallery/clip.bin"), b"media");
        put(
            &old.join("library/item/original.json"),
            serde_json::to_vec(&json!({"file":gallery.join("clip.bin")})).unwrap(),
        );
        put(
            &new.join("library/item/original.json"),
            serde_json::to_vec(&json!({"file":new.join("Gallery/clip.bin")})).unwrap(),
        );
        put(
            &old.join("library/item/theme.json"),
            br#"{"widgets":[{"kind":"clock"}]}"#,
        );
        put(
            &new.join("library/item/theme.json"),
            br#"{"widgets":[{"kind":"clock"}]}"#,
        );
        (dir, old, new, gallery)
    }
    #[test]
    fn verifies_rebased_copy_and_ignores_observation_timestamps() {
        let (_d, o, n, g) = fixture();
        let before = fs::read(o.join("settings.json")).unwrap();
        let result = audit(&o, &n, &g).unwrap();
        assert_eq!(result["verified"], true, "{result}");
        assert_eq!(result["library"]["originals"], 1);
        assert_eq!(result["settings"]["changedSections"], 0);
        assert_eq!(fs::read(o.join("settings.json")).unwrap(), before);
        let text = result.to_string();
        assert!(!text.contains(&o.to_string_lossy().to_string()));
        assert!(!text.contains("clip.bin"));
    }
    #[test]
    fn preserves_changed_uc_theme_and_additional_files() {
        let (_d, o, n, g) = fixture();
        put(
            &n.join("library/item/theme.json"),
            br#"{"widgets":[{"kind":"date"}]}"#,
        );
        put(&n.join("library/extra.bin"), b"extra");
        let result = audit(&o, &n, &g).unwrap();
        assert_eq!(result["verified"], true);
        assert_eq!(result["library"]["changedRecords"], 1);
        assert_eq!(result["library"]["additionalFiles"], 1);
        assert!(
            String::from_utf8(fs::read(n.join("library/item/theme.json")).unwrap())
                .unwrap()
                .contains("date")
        );
    }
    #[test]
    fn missing_and_different_media_refuse_verification() {
        let (_d, o, n, g) = fixture();
        fs::remove_file(n.join("Gallery/clip.bin")).unwrap();
        let result = audit(&o, &n, &g).unwrap();
        assert_eq!(result["verified"], false);
        assert_eq!(result["gallery"]["missingFiles"], 1);
        assert!(result["references"]["missing"].as_u64().unwrap() > 0);
        put(&n.join("Gallery/clip.bin"), b"other");
        let result = audit(&o, &n, &g).unwrap();
        assert_eq!(result["gallery"]["changedMediaFiles"], 1);
        assert_eq!(result["verified"], false);
    }
    #[test]
    fn rejects_runtime_old_dependencies_but_not_historical_backups() {
        let (_d, o, n, g) = fixture();
        let legacy = serde_json::to_vec(&json!({"file":g.join("clip.bin")})).unwrap();
        put(&o.join("library/item/meta.v1.json"), &legacy);
        put(&n.join("library/item/meta.v1.json"), &legacy);
        assert_eq!(audit(&o, &n, &g).unwrap()["verified"], true);
        put(&n.join("library/item/original.json"), &legacy);
        let result = audit(&o, &n, &g).unwrap();
        assert_eq!(result["verified"], false);
        assert_eq!(result["references"]["oldDependencies"], 1);
    }
    #[test]
    fn broken_json_and_unknown_marker_are_not_verified() {
        let (_d, o, n, g) = fixture();
        put(
            &n.join("library/item/theme.json"),
            b"invalid private details",
        );
        let result = audit(&o, &n, &g).unwrap();
        assert_eq!(result["verified"], false);
        assert!(!result.to_string().contains("private details"));
        put(&n.join("library/item/theme.json"), b"{}");
        put(
            &n.join("migration.json"),
            br#"{"version":99,"copiedExisting":true,"originalKept":true}"#,
        );
        assert_eq!(audit(&o, &n, &g).unwrap()["verified"], false);
        put(&n.join("settings.json"), b"false");
        assert_eq!(audit(&o, &n, &g).unwrap()["verified"], false);
    }
    #[test]
    fn compares_in_streaming_chunks_and_detects_changes() {
        let (_d, o, n, g) = fixture();
        let bytes = vec![7; 150_000];
        put(&g.join("large.bin"), &bytes);
        put(&n.join("Gallery/large.bin"), &bytes);
        assert_eq!(audit(&o, &n, &g).unwrap()["verified"], true);
        let mut bytes = bytes;
        bytes[70_000] = 8;
        put(&n.join("Gallery/large.bin"), bytes);
        assert_eq!(
            audit(&o, &n, &g).unwrap()["gallery"]["changedMediaFiles"],
            1
        );
        let p = n.join("Gallery/clip.bin");
        let entry = Entry::new(p.clone(), &fs::metadata(&p).unwrap());
        put(&p, b"changed length");
        assert!(!entry.unchanged().unwrap());
    }
    #[test]
    fn normalizes_windows_canonical_prefix_and_boundaries() {
        assert_eq!(
            normalized(r"\\?\C:\Users\Example\Gallery\clip.bin"),
            "c:/users/example/gallery/clip.bin"
        );
        assert!(!inside("c:/old-other/a", "c:/old"));
    }
    #[test]
    fn external_backgrounds_are_preserved_but_missing_extensionless_files_are_reported() {
        let (d, o, n, g) = fixture();
        let external = d.path().join("user-selected.bin");
        put(&external, b"external");
        let theme = serde_json::to_vec(&json!({"background":external})).unwrap();
        put(&o.join("library/item/theme.json"), &theme);
        put(&n.join("library/item/theme.json"), &theme);
        let result = audit(&o, &n, &g).unwrap();
        assert_eq!(result["verified"], true);
        assert_eq!(result["references"]["external"], 1);
        put(&n.join("library/item/theme.json"), br#"{"file":"missing"}"#);
        let result = audit(&o, &n, &g).unwrap();
        assert_eq!(result["verified"], false);
        assert_eq!(result["references"]["missing"], 1);
    }
    #[test]
    fn limits_json_and_tree_depth_without_exposing_paths() {
        let (_d, o, n, g) = fixture();
        let file = File::create(n.join("library/huge.json")).unwrap();
        file.set_len(MAX_JSON + 1).unwrap();
        assert!(audit(&o, &n, &g).unwrap_err().code_is("cooler_audit_limit"));
        fs::remove_file(n.join("library/huge.json")).unwrap();
        let mut deep = n.join("library");
        for _ in 0..=MAX_DEPTH {
            deep = deep.join("d");
        }
        fs::create_dir_all(&deep).unwrap();
        assert!(audit(&o, &n, &g).unwrap_err().code_is("cooler_audit_limit"));
    }
    #[cfg(unix)]
    #[test]
    fn rejects_linked_folders_and_reference_ancestors() {
        use std::os::unix::fs::symlink;
        let (d, o, n, g) = fixture();
        let outside = d.path().join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, n.join("library/linked")).unwrap();
        let error = audit(&o, &n, &g).unwrap_err();
        assert!(error.code_is("cooler_audit_linked"));
        assert!(!error
            .to_string()
            .contains(&outside.to_string_lossy().to_string()));
    }
}
