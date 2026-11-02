//! Bounded file streaming into host-owned staging paths; no caller supplied paths.
use super::error::{CoreError, CoreResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const CHUNK: usize = 256 * 1024;
pub struct Transfer {
    pub path: PathBuf,
    pub kind: String,
    pub original_id: Option<String>,
    size: u64,
    received: u64,
    pub finishing: bool,
}
pub struct Transfers {
    root: PathBuf,
    items: HashMap<String, Transfer>,
    ids: AtomicU64,
}
impl Transfers {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            items: HashMap::new(),
            ids: AtomicU64::new(1),
        }
    }
    pub fn busy(&self) -> bool {
        !self.items.is_empty()
    }
    pub fn begin(&mut self, args: &Value) -> CoreResult<Value> {
        let name = args["name"]
            .as_str()
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 240
                    && !s
                        .chars()
                        .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
                    && !s.ends_with(['.', ' '])
            })
            .ok_or_else(|| CoreError::new("Choose a valid file name"))?;
        let size = args["size"]
            .as_u64()
            .filter(|s| *s > 0 && *s <= MAX_FILE)
            .ok_or_else(|| CoreError::new("Choose a file no larger than 2 GiB"))?;
        let kind = args["kind"]
            .as_str()
            .filter(|s| ["original", "asset"].contains(s))
            .ok_or_else(|| CoreError::new("Invalid import kind"))?;
        let original_id = if kind == "asset" {
            Some(
                args["originalId"]
                    .as_str()
                    .filter(|s| s.len() <= 32 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
                    .ok_or_else(|| CoreError::new("Choose a theme for this picture"))?
                    .into(),
            )
        } else {
            None
        };
        if self.items.len() >= 8 {
            return Err(CoreError::coded(
                "busy",
                "Too many files are being imported",
            ));
        }
        std::fs::create_dir_all(&self.root)?;
        let id = format!(
            "{}-{}-{}",
            std::process::id(),
            super::scheduler::now_ms(),
            self.ids.fetch_add(1, Ordering::Relaxed)
        );
        let folder = self.root.join(&id);
        std::fs::create_dir(&folder)?;
        let path = folder.join(name);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.items.insert(
            id.clone(),
            Transfer {
                path,
                kind: kind.into(),
                original_id,
                size,
                received: 0,
                finishing: false,
            },
        );
        Ok(json!({"transferId":id}))
    }
    pub fn append(&mut self, args: &Value) -> CoreResult<Value> {
        let id = args["transferId"]
            .as_str()
            .ok_or_else(|| CoreError::new("Invalid file transfer"))?;
        let transfer = self
            .items
            .get_mut(id)
            .ok_or_else(|| CoreError::new("File transfer no longer exists"))?;
        if transfer.finishing {
            return Err(CoreError::coded("busy", "The file import is finishing"));
        }
        if args["offset"].as_u64() != Some(transfer.received) {
            return Err(CoreError::new("File data arrived out of order"));
        }
        let values = args["bytes"]
            .as_array()
            .filter(|bytes| !bytes.is_empty() && bytes.len() <= CHUNK)
            .ok_or_else(|| CoreError::new("Invalid file chunk"))?;
        let bytes = values
            .iter()
            .map(|v| v.as_u64().filter(|n| *n <= 255).map(|n| n as u8))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| CoreError::new("Invalid file chunk"))?;
        if transfer.received + bytes.len() as u64 > transfer.size {
            return Err(CoreError::new("File exceeds its declared size"));
        }
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&transfer.path)?;
        file.write_all(&bytes)?;
        transfer.received += bytes.len() as u64;
        Ok(json!({"received":transfer.received}))
    }
    pub fn finish(&mut self, id: &str) -> CoreResult<(PathBuf, String, Option<String>)> {
        let transfer = self
            .items
            .get_mut(id)
            .ok_or_else(|| CoreError::new("File transfer no longer exists"))?;
        if transfer.finishing || transfer.received != transfer.size {
            return Err(CoreError::new("File transfer is not complete"));
        }
        transfer.finishing = true;
        Ok((
            transfer.path.clone(),
            transfer.kind.clone(),
            transfer.original_id.clone(),
        ))
    }
    pub fn abort(&mut self, id: &str) -> CoreResult<Value> {
        let transfer = self
            .items
            .get(id)
            .ok_or_else(|| CoreError::new("File transfer no longer exists"))?;
        if transfer.finishing {
            return Err(CoreError::coded("busy", "The file import is finishing"));
        }
        self.remove(id)?;
        Ok(json!({"ok":true}))
    }
    pub fn complete(&mut self, id: &str, success: bool) -> CoreResult<()> {
        if success {
            self.remove(id)
        } else {
            if let Some(transfer) = self.items.get_mut(id) {
                transfer.finishing = false;
            }
            Ok(())
        }
    }
    fn remove(&mut self, id: &str) -> CoreResult<()> {
        if let Some(transfer) = self.items.get(id) {
            std::fs::remove_file(&transfer.path)?;
            if let Some(folder) = transfer.path.parent() {
                std::fs::remove_dir(folder)?;
            }
        }
        self.items.remove(id);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_bounded_transfer_keeps_private_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut transfers = Transfers::new(dir.path().join("transfers"));
        for name in ["../x", "a/b", "bad.exe:", "x\\y"] {
            assert!(transfers
                .begin(&json!({"name":name,"size":2,"kind":"original"}))
                .is_err());
        }
        let id = transfers
            .begin(&json!({"name":"clip.mp4","size":3,"kind":"original"}))
            .unwrap()["transferId"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(transfers.busy());
        assert!(transfers
            .append(&json!({"transferId":id,"offset":1,"bytes":[1]}))
            .is_err());
        transfers
            .append(&json!({"transferId":id,"offset":0,"bytes":[1,2]}))
            .unwrap();
        assert!(transfers.finish(&id).is_err());
        transfers
            .append(&json!({"transferId":id,"offset":2,"bytes":[3]}))
            .unwrap();
        let (path, _, _) = transfers.finish(&id).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), [1, 2, 3]);
        assert!(transfers.abort(&id).is_err());
        transfers.complete(&id, false).unwrap();
        transfers.abort(&id).unwrap();
        assert!(!path.exists());
        assert!(!transfers.busy());
    }
}
