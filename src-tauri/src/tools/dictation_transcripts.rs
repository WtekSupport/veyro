//! Persisted dictation session transcripts for the “Dictation buffers” tool.
//!
//! Text is appended when injected live or deferred on focus loss (flush does not
//! double-append). Current session updates emit a live event for the tool UI.

use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::settings::data_storage::default_data_storage_root;

pub const HISTORY_FILE: &str = "dictation-transcripts.json";
const MAX_ENTRIES: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptStatus {
    Active,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub id: String,
    pub session_id: u64,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub status: TranscriptStatus,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryDisk {
    entries: Vec<TranscriptEntry>,
}

static STORE: LazyLock<Mutex<Vec<TranscriptEntry>>> =
    LazyLock::new(|| Mutex::new(load_history()));

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn history_path() -> PathBuf {
    default_data_storage_root()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(HISTORY_FILE)
}

fn load_history() -> Vec<TranscriptEntry> {
    let path = history_path();
    let Ok(bytes) = fs::read(&path) else {
        return Vec::new();
    };
    let mut entries = serde_json::from_slice::<HistoryDisk>(&bytes)
        .map(|d| d.entries)
        .unwrap_or_default();
    // No live session survives process restart — coerce stale "active" rows.
    let now = now_ms();
    let mut changed = false;
    for entry in entries.iter_mut() {
        if entry.status == TranscriptStatus::Active {
            entry.status = TranscriptStatus::Done;
            entry.updated_at_ms = now;
            changed = true;
        }
    }
    if changed {
        let _ = save_history(&entries);
    }
    entries
}

fn save_history(entries: &[TranscriptEntry]) -> Result<(), String> {
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

fn persist_and_emit(app: Option<&AppHandle>, entries: &[TranscriptEntry], changed: &TranscriptEntry) {
    let _ = save_history(entries);
    if let Some(app) = app {
        crate::app::events::emit_dictation_transcript_updated(app, changed.clone());
    }
}

fn trim_cap(entries: &mut Vec<TranscriptEntry>) {
    if entries.len() <= MAX_ENTRIES {
        return;
    }
    entries.sort_by(|a, b| a.updated_at_ms.cmp(&b.updated_at_ms));
    let drop_n = entries.len() - MAX_ENTRIES;
    entries.drain(0..drop_n);
}

fn entry_id_for_session(session_id: u64) -> String {
    format!("dt-{session_id}")
}

fn demote_other_actives(
    entries: &mut [TranscriptEntry],
    keep_id: &str,
    now: u64,
) -> Vec<TranscriptEntry> {
    let mut finished = Vec::new();
    for entry in entries.iter_mut() {
        if entry.id != keep_id && entry.status == TranscriptStatus::Active {
            entry.status = TranscriptStatus::Done;
            entry.updated_at_ms = now;
            finished.push(entry.clone());
        }
    }
    finished
}

/// Ensure an active transcript row exists for this dictation session id.
pub fn ensure_active(app: Option<&AppHandle>, session_id: u64) {
    if session_id == 0 {
        return;
    }
    let Ok(mut entries) = STORE.lock() else {
        return;
    };
    let id = entry_id_for_session(session_id);
    let now = now_ms();
    let finished = demote_other_actives(&mut entries, &id, now);
    if let Some(existing) = entries.iter_mut().find(|e| e.id == id) {
        existing.status = TranscriptStatus::Active;
        existing.updated_at_ms = now;
        let changed = existing.clone();
        let _ = save_history(&entries);
        if let Some(app) = app {
            for done in finished {
                crate::app::events::emit_dictation_transcript_updated(app, done);
            }
            crate::app::events::emit_dictation_transcript_updated(app, changed);
        }
        return;
    }
    let entry = TranscriptEntry {
        id,
        session_id,
        started_at_ms: now,
        updated_at_ms: now,
        status: TranscriptStatus::Active,
        text: String::new(),
    };
    entries.push(entry.clone());
    trim_cap(&mut entries);
    let _ = save_history(&entries);
    if let Some(app) = app {
        for done in finished {
            crate::app::events::emit_dictation_transcript_updated(app, done);
        }
        crate::app::events::emit_dictation_transcript_updated(app, entry);
    }
}

/// Append committed transcript text (live inject or defer). Empty chunks are ignored.
pub fn append_text(app: Option<&AppHandle>, session_id: u64, text: &str) {
    if session_id == 0 || text.is_empty() {
        return;
    }
    ensure_active(app, session_id);
    let Ok(mut entries) = STORE.lock() else {
        return;
    };
    let id = entry_id_for_session(session_id);
    let Some(entry) = entries.iter_mut().find(|e| e.id == id) else {
        return;
    };
    entry.text.push_str(text);
    entry.updated_at_ms = now_ms();
    entry.status = TranscriptStatus::Active;
    let changed = entry.clone();
    persist_and_emit(app, &entries, &changed);
}

pub fn mark_all_active_done(app: Option<&AppHandle>) {
    let Ok(mut entries) = STORE.lock() else {
        return;
    };
    let now = now_ms();
    let mut finished: Vec<TranscriptEntry> = Vec::new();
    for entry in entries.iter_mut() {
        if entry.status == TranscriptStatus::Active {
            entry.status = TranscriptStatus::Done;
            entry.updated_at_ms = now;
            finished.push(entry.clone());
        }
    }
    if finished.is_empty() {
        return;
    }
    let _ = save_history(&entries);
    if let Some(app) = app {
        for done in finished {
            crate::app::events::emit_dictation_transcript_updated(app, done);
        }
    }
}

/// Keep only `session_id` active; demote any other live rows (orphan cleanup).
pub fn mark_done_except(app: Option<&AppHandle>, session_id: u64) {
    if session_id == 0 {
        mark_all_active_done(app);
        return;
    }
    let Ok(mut entries) = STORE.lock() else {
        return;
    };
    let keep_id = entry_id_for_session(session_id);
    let finished = demote_other_actives(&mut entries, &keep_id, now_ms());
    if finished.is_empty() {
        return;
    }
    let _ = save_history(&entries);
    if let Some(app) = app {
        for done in finished {
            crate::app::events::emit_dictation_transcript_updated(app, done);
        }
    }
}

pub fn list() -> Vec<TranscriptEntry> {
    let Ok(entries) = STORE.lock() else {
        return Vec::new();
    };
    let mut out = entries.clone();
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    out
}

pub fn get(id: &str) -> Option<TranscriptEntry> {
    let Ok(entries) = STORE.lock() else {
        return None;
    };
    entries.iter().find(|e| e.id == id).cloned()
}

pub fn remove(id: &str) -> Result<Vec<TranscriptEntry>, String> {
    let Ok(mut entries) = STORE.lock() else {
        return Err("dictation transcript store lock poisoned".into());
    };
    entries.retain(|e| e.id != id);
    save_history(&entries)?;
    let mut out = entries.clone();
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    Ok(out)
}
