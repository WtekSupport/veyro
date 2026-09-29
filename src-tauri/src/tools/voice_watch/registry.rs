use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::settings::data_storage::default_data_storage_root;

pub const REGISTRY_FILE: &str = "voice-watch-registry.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub path: String,
    pub size: u64,
    pub mtime_ms: u64,
    pub sha256: String,
    pub processed_at_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryDisk {
    entries: Vec<RegistryEntry>,
}

#[derive(Debug, Default)]
pub struct DedupRegistry {
    by_hash: HashMap<String, RegistryEntry>,
    by_path_sig: HashMap<String, RegistryEntry>,
}

fn path_sig(path: &str, size: u64, mtime_ms: u64) -> String {
    format!("{path}|{size}|{mtime_ms}")
}

pub fn registry_path() -> PathBuf {
    default_data_storage_root()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(REGISTRY_FILE)
}

pub fn content_sha256(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl DedupRegistry {
    pub fn load() -> Self {
        let path = registry_path();
        let Ok(bytes) = fs::read(&path) else {
            return Self::default();
        };
        let Ok(disk) = serde_json::from_slice::<RegistryDisk>(&bytes) else {
            return Self::default();
        };
        let mut reg = Self::default();
        for entry in disk.entries {
            reg.by_hash.insert(entry.sha256.clone(), entry.clone());
            reg.by_path_sig.insert(
                path_sig(&entry.path, entry.size, entry.mtime_ms),
                entry,
            );
        }
        reg
    }

    pub fn save(&self) -> Result<(), String> {
        let path = registry_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut entries: Vec<_> = self.by_hash.values().cloned().collect();
        entries.sort_by(|a, b| a.processed_at_ms.cmp(&b.processed_at_ms));
        // Cap growth
        if entries.len() > 5000 {
            entries = entries.split_off(entries.len() - 5000);
        }
        let disk = RegistryDisk { entries };
        let json = serde_json::to_vec_pretty(&disk).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn is_duplicate(&self, path: &str, size: u64, mtime: SystemTime, sha: &str) -> bool {
        if self.by_hash.contains_key(sha) {
            return true;
        }
        let mtime_ms = system_time_ms(mtime);
        self.by_path_sig
            .contains_key(&path_sig(path, size, mtime_ms))
    }

    pub fn remember(
        &mut self,
        path: &str,
        size: u64,
        mtime: SystemTime,
        sha: String,
    ) {
        let now = system_time_ms(SystemTime::now());
        let entry = RegistryEntry {
            path: path.to_string(),
            size,
            mtime_ms: system_time_ms(mtime),
            sha256: sha.clone(),
            processed_at_ms: now,
        };
        self.by_path_sig.insert(
            path_sig(&entry.path, entry.size, entry.mtime_ms),
            entry.clone(),
        );
        self.by_hash.insert(sha, entry);
        let _ = self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn dedup_by_hash() {
        let dir = std::env::temp_dir().join(format!(
            "veyro-dedup-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.ogg");
        {
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(b"hello-voice").unwrap();
        }
        let sha = content_sha256(&path).unwrap();
        let mut reg = DedupRegistry::default();
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        let path_str = path.to_string_lossy().to_string();
        assert!(!reg.is_duplicate(&path_str, 11, mtime, &sha));
        reg.remember(&path_str, 11, mtime, sha.clone());
        assert!(reg.is_duplicate(&path_str, 11, mtime, &sha));
        assert!(reg.is_duplicate("/other/b.ogg", 11, mtime, &sha));
        let _ = fs::remove_dir_all(&dir);
    }
}
