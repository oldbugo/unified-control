//! App-owned normalized pictures. Profiles contain opaque IDs, never file paths.
use super::sync::MutexExt;
use super::key_render::{PictureAsset, MAX_PICTURE_BYTES};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub struct AssetStore {
    directory: PathBuf,
    cache: Mutex<HashMap<String, Arc<PictureAsset>>>,
    importing: Mutex<()>,
}
impl AssetStore {
    pub fn new(home: &Path) -> Self {
        Self {
            directory: home.join("assets"),
            cache: Mutex::new(HashMap::new()),
            importing: Mutex::new(()),
        }
    }
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if id.is_empty()
            || id.len() > 64
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err("Invalid picture ID".into());
        }
        Ok(self.directory.join(format!("{id}.png")))
    }
    pub fn import(&self, bytes: &[u8]) -> Result<String, String> {
        let _guard = self.importing.lock_or_recover();
        let image = Arc::new(PictureAsset::decode(bytes)?);
        std::fs::create_dir_all(&self.directory)
            .map_err(|_| "The picture folder could not be created")?;
        if std::fs::read_dir(&self.directory)
            .map_err(|_| "The picture folder could not be read")?
            .take(257)
            .count()
            >= 256
        {
            return Err("The picture library is full (256 pictures)".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let id = format!(
            "asset-{stamp:x}-{:x}-{:x}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        super::js::write_atomic(&self.path(&id)?, image.png())
            .map_err(|_| "The picture could not be saved")?;
        self.remember(id.clone(), image);
        Ok(id)
    }
    fn remember(&self, id: String, image: Arc<PictureAsset>) {
        let mut cache = self.cache.lock_or_recover();
        if cache.len() >= 16 {
            cache.clear()
        }
        cache.insert(id, image);
    }
    pub fn get(&self, id: &str) -> Result<Arc<PictureAsset>, String> {
        let path = self.path(id)?;
        if let Some(image) = self.cache.lock_or_recover().get(id).cloned() {
            return Ok(image);
        }
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|_| "The selected picture is missing")?;
        let mut bytes = Vec::new();
        file.take(MAX_PICTURE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The selected picture could not be read")?;
        let image = Arc::new(PictureAsset::decode(&bytes)?);
        self.remember(id.into(), image.clone());
        Ok(image)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assets_are_normalized_owned_and_path_bounded() {
        let home = tempfile::tempdir().unwrap();
        let store = AssetStore::new(home.path());
        assert!(store.get("../outside").is_err());
        assert!(store.import(b"not an image").is_err());
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, 2, 2);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .unwrap()
                .write_image_data(&[90; 12])
                .unwrap();
        }
        let id = store.import(&bytes).unwrap();
        assert_eq!(store.get(&id).unwrap().dimensions(), (2, 2));
        assert_eq!(
            AssetStore::new(home.path()).get(&id).unwrap().dimensions(),
            (2, 2)
        );
        assert!(store.get("missing").is_err());
    }
}
