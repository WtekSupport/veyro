use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::settings::data_storage::default_data_storage_root;
use crate::tools::voice_watch::paths::paths_equal;

pub const HISTORY_FILE: &str = "voice-history.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryStatus {
    Pending,
    Processing,
    Done,
    Error,
    SpeechUnrecognized,
    TooLong,
    NotAudio,
    Skipped,
    /// In the shared file index only — no STT queued (e.g. added from speech analysis).
    Indexed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messenger: Option<String>,
    pub appeared_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    pub status: HistoryStatus,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryDisk {
    entries: Vec<HistoryEntry>,
}

pub fn history_path() -> PathBuf {
    default_data_storage_root()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(HISTORY_FILE)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn load_history() -> Vec<HistoryEntry> {
    let path = history_path();
    let Ok(bytes) = fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice::<HistoryDisk>(&bytes)
        .map(|d| d.entries)
        .unwrap_or_default()
}

pub fn save_history(entries: &[HistoryEntry]) -> Result<(), String> {
    let path = history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let disk = HistoryDisk {
        entries: entries.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&disk).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

fn dedupe_history(entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
    let mut sorted = entries;
    sorted.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    let mut kept = Vec::with_capacity(sorted.len());
    for entry in sorted {
        let duplicate = kept.iter().any(|prev: &HistoryEntry| {
            paths_equal(&prev.path, &entry.path)
                || prev
                    .content_sha256
                    .as_deref()
                    .zip(entry.content_sha256.as_deref())
                    .is_some_and(|(left, right)| left == right)
        });
        if !duplicate {
            kept.push(entry);
        }
    }
    kept
}

pub fn list_history() -> Vec<HistoryEntry> {
    let loaded = load_history();
    let before = loaded.len();
    let deduped = dedupe_history(loaded);
    if deduped.len() != before {
        let _ = save_history(&deduped);
    }
    deduped
}

pub fn purge_expired(retention_days: u32) -> Vec<HistoryEntry> {
    if retention_days == 0 {
        return load_history();
    }
    let cutoff = now_ms().saturating_sub(u64::from(retention_days) * 86_400_000);
    let mut entries = load_history();
    entries.retain(|e| e.updated_at_ms >= cutoff);
    let _ = save_history(&entries);
    entries
}

pub fn clear_history() -> Result<(), String> {
    save_history(&[])
}

pub fn delete_history_entry(id: &str) -> Result<Vec<HistoryEntry>, String> {
    let mut entries = load_history();
    entries.retain(|e| e.id != id);
    save_history(&entries)?;
    Ok(entries)
}

pub fn upsert_history_entry(entry: HistoryEntry) -> Result<HistoryEntry, String> {
    let mut entries = load_history();
    if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
        *existing = entry.clone();
    } else {
        entries.push(entry.clone());
    }
    // Cap list
    if entries.len() > 2000 {
        entries.sort_by(|a, b| a.updated_at_ms.cmp(&b.updated_at_ms));
        entries = entries.split_off(entries.len() - 2000);
    }
    save_history(&entries)?;
    Ok(entry)
}

pub fn new_history_id() -> String {
    format!("vh-{}", now_ms())
}

pub fn touch_now_ms() -> u64 {
    now_ms()
}
