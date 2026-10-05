//! Persisted dictation session transcripts for the “Dictation buffers” tool.
//!
//! Text is appended when injected live or deferred on focus loss (flush does not
//! double-append). Current session updates emit a live event for the tool UI.
//! When a session becomes **done**, its text is written to
//! `{data_storage}/dictation_buffers/{id}.txt` and indexed in JSON (`transcriptFile`).

use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::settings::data_storage::{
    default_data_storage_root, SUBDIR_DICTATION_BUFFERS,
};

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
    /// Relative path under data storage (`dictation_buffers/{id}.txt`) when persisted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_file: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryDisk {
    entries: Vec<TranscriptEntry>,
}

static STORE: LazyLock<Mutex<Vec<TranscriptEntry>>> = LazyLock::new(|| {
    let mut entries = load_history();
    if sync_done_sessions_to_storage(&mut entries) {
        let _ = save_history(&entries);
    }
    Mutex::new(entries)
});

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

fn buffers_dir() -> PathBuf {
    default_data_storage_root()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(SUBDIR_DICTATION_BUFFERS)
}

fn session_file_relative_id(entry_id: &str) -> String {
    format!("{SUBDIR_DICTATION_BUFFERS}/{entry_id}.txt")
}

fn session_file_path(entry_id: &str) -> PathBuf {
    buffers_dir().join(format!("{entry_id}.txt"))
}

fn persist_session_file(entry: &mut TranscriptEntry) -> Result<(), String> {
    if entry.text.trim().is_empty() {
        entry.transcript_file = None;
        if session_file_path(&entry.id).exists() {
            let _ = fs::remove_file(session_file_path(&entry.id));
        }
        return Ok(());
    }
    let dir = buffers_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = session_file_path(&entry.id);
    fs::write(&path, &entry.text).map_err(|e| e.to_string())?;
    entry.transcript_file = Some(session_file_relative_id(&entry.id));
    Ok(())
}

fn finalize_finished_sessions(entries: &mut [TranscriptEntry], finished: &[TranscriptEntry]) {
    for done in finished {
        if let Some(entry) = entries.iter_mut().find(|e| e.id == done.id) {
            if let Err(err) = persist_session_file(entry) {
                tracing::warn!("dictation session file save failed: {err}");
            }
        }
    }
}

/// Write every completed session with text to `{data_storage}/dictation_buffers/`.
fn sync_done_sessions_to_storage(entries: &mut [TranscriptEntry]) -> bool {
    let mut index_changed = false;
    for entry in entries.iter_mut() {
        if entry.status != TranscriptStatus::Done || entry.text.trim().is_empty() {
            continue;
        }
        let prev_file = entry.transcript_file.clone();
        match persist_session_file(entry) {
            Ok(()) => {
                if entry.transcript_file != prev_file {
                    index_changed = true;
                }
            }
            Err(err) => tracing::warn!(
                "dictation session file sync failed for {}: {err}",
                entry.id
            ),
        }
    }
    index_changed
}

fn delete_session_file(entry_id: &str) {
    let path = session_file_path(entry_id);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
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
        for entry in entries.iter_mut() {
            if entry.status == TranscriptStatus::Done {
                let _ = persist_session_file(entry);
            }
        }
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
    let removed: Vec<TranscriptEntry> = entries.drain(0..drop_n).collect();
    for entry in removed {
        delete_session_file(&entry.id);
    }
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
    let demoted_publish: Vec<(u64, u64, String)> = finished
        .iter()
        .map(|e| (e.session_id, e.started_at_ms, e.text.clone()))
        .collect();
    finalize_finished_sessions(&mut entries, &finished);
    if let Some(existing) = entries.iter_mut().find(|e| e.id == id) {
        existing.status = TranscriptStatus::Active;
        existing.updated_at_ms = now;
        let changed = existing.clone();
        let _ = save_history(&entries);
        if let Some(app) = app {
            for done in &finished {
                let current = entries
                    .iter()
                    .find(|e| e.id == done.id)
                    .cloned()
                    .unwrap_or_else(|| done.clone());
                crate::app::events::emit_dictation_transcript_updated(app, current);
            }
            crate::app::events::emit_dictation_transcript_updated(app, changed);
        }
        for (session_id, started_at_ms, text) in demoted_publish {
            crate::tools::dictation_session_audio::publish_session_recording(
                app,
                session_id,
                started_at_ms,
                &text,
            );
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
        transcript_file: None,
    };
    entries.push(entry.clone());
    trim_cap(&mut entries);
    let _ = save_history(&entries);
    if let Some(app) = app {
        for done in &finished {
            let current = entries
                .iter()
                .find(|e| e.id == done.id)
                .cloned()
                .unwrap_or_else(|| done.clone());
            crate::app::events::emit_dictation_transcript_updated(app, current);
        }
        crate::app::events::emit_dictation_transcript_updated(app, entry);
        for (session_id, started_at_ms, text) in demoted_publish {
            crate::tools::dictation_session_audio::publish_session_recording(
                Some(app),
                session_id,
                started_at_ms,
                &text,
            );
        }
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
    let session_ids: Vec<u64> = STORE
        .lock()
        .ok()
        .map(|entries| {
            entries
                .iter()
                .filter(|e| e.status == TranscriptStatus::Active)
                .map(|e| e.session_id)
                .collect()
        })
        .unwrap_or_default();
    for session_id in session_ids {
        complete_session(app, session_id, None);
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
    finalize_finished_sessions(&mut entries, &finished);
    let demoted_publish: Vec<(u64, u64, String)> = finished
        .iter()
        .map(|e| (e.session_id, e.started_at_ms, e.text.clone()))
        .collect();
    let _ = save_history(&entries);
    if let Some(app) = app {
        for done in &finished {
            let current = entries
                .iter()
                .find(|e| e.id == done.id)
                .cloned()
                .unwrap_or_else(|| done.clone());
            crate::app::events::emit_dictation_transcript_updated(app, current);
        }
        for (session_id, started_at_ms, text) in demoted_publish {
            crate::tools::dictation_session_audio::publish_session_recording(
                Some(app),
                session_id,
                started_at_ms,
                &text,
            );
        }
    }
}

/// Mark session done and write `{data_storage}/dictation_buffers/{id}.txt`.
pub fn complete_session(
    app: Option<&AppHandle>,
    session_id: u64,
    final_text: Option<&str>,
) {
    if session_id == 0 {
        return;
    }
    let Ok(mut entries) = STORE.lock() else {
        return;
    };
    let id = entry_id_for_session(session_id);
    let Some(entry) = entries.iter_mut().find(|e| e.id == id) else {
        return;
    };
    if let Some(text) = final_text {
        entry.text = text.to_string();
    }
    entry.status = TranscriptStatus::Done;
    entry.updated_at_ms = now_ms();
    if let Err(err) = persist_session_file(entry) {
        tracing::warn!("dictation session file save failed for {id}: {err}");
    }
    let changed = entry.clone();
    let publish = (changed.session_id, changed.started_at_ms, changed.text.clone());
    let _ = save_history(&entries);
    drop(entries);
    if let Some(app) = app {
        crate::app::events::emit_dictation_transcript_updated(app, changed);
        let (session_id, started_at_ms, text) = publish;
        crate::tools::dictation_session_audio::publish_session_recording(
            Some(app),
            session_id,
            started_at_ms,
            &text,
        );
    }
}

pub fn list() -> Vec<TranscriptEntry> {
    let Ok(mut entries) = STORE.lock() else {
        return Vec::new();
    };
    if sync_done_sessions_to_storage(&mut entries) {
        let _ = save_history(&entries);
    }
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
    delete_session_file(id);
    save_history(&entries)?;
    let mut out = entries.clone();
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    Ok(out)
}
