//! Central store for tool audio under `{data_storage}/voice_records/` (SHA-256 dedup).

use std::fs;
use std::path::{Path, PathBuf};

use crate::settings::data_storage::{default_data_storage_root, SUBDIR_VOICE_RECORDS};
use crate::tools::shared::normalize_tool_path_key;
use crate::tools::voice_watch::history::{load_history, HistoryEntry};
use crate::tools::voice_watch::paths::paths_equal;
use crate::tools::voice_watch::registry::{content_sha256, DedupRegistry};

#[derive(Debug, Clone)]
pub struct IngestedVoice {
    pub canonical_path: String,
    pub sha256: String,
    pub display_file_name: String,
}

pub fn voice_records_dir() -> Result<PathBuf, String> {
    let root = default_data_storage_root().map_err(|e| e.to_string())?;
    let dir = root.join(SUBDIR_VOICE_RECORDS);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn extension_for(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty())
        .map(|e| format!(".{e}"))
        .unwrap_or_else(|| ".bin".to_string())
}

fn canonical_target(dir: &Path, sha: &str, ext: &str) -> PathBuf {
    dir.join(format!("{sha}{ext}"))
}

fn existing_canonical_for_hash(history: &[HistoryEntry], sha: &str) -> Option<String> {
    for entry in history {
        if entry.content_sha256.as_deref() != Some(sha) {
            continue;
        }
        if Path::new(&entry.path).is_file() {
            return Some(entry.path.clone());
        }
    }
    None
}

fn remember_in_registry(canonical: &Path, sha: &str) {
    let Ok(meta) = fs::metadata(canonical) else {
        return;
    };
    let Ok(mtime) = meta.modified() else {
        return;
    };
    let mut reg = DedupRegistry::load();
    reg.remember(
        &canonical.to_string_lossy(),
        meta.len(),
        mtime,
        sha.to_string(),
    );
}

/// Copy (or reuse) a user file into `voice_records/` and return the canonical path.
pub fn ingest_voice_source(source_path: &str) -> Result<IngestedVoice, String> {
    let trimmed = source_path.trim();
    if trimmed.is_empty() {
        return Err("tools.voiceFiles.invalidPath".to_string());
    }
    let source = Path::new(trimmed);
    if !source.is_file() {
        return Err("tools.voiceFiles.invalidPath".to_string());
    }

    let display_file_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio")
        .to_string();

    let sha = content_sha256(source)?;
    let dir = voice_records_dir()?;
    let ext = extension_for(source);
    let target = canonical_target(&dir, &sha, &ext);

    let canonical_path = normalize_tool_path_key(&target.to_string_lossy());
    let source_key = normalize_tool_path_key(trimmed);

    if paths_equal(&source_key, &canonical_path) {
        remember_in_registry(&target, &sha);
        return Ok(IngestedVoice {
            canonical_path,
            sha256: sha,
            display_file_name,
        });
    }

    let history = load_history();
    if let Some(existing) = existing_canonical_for_hash(&history, &sha) {
        let canonical_path = normalize_tool_path_key(&existing);
        remember_in_registry(Path::new(&canonical_path), &sha);
        return Ok(IngestedVoice {
            canonical_path,
            sha256: sha,
            display_file_name,
        });
    }

    if target.is_file() {
        remember_in_registry(&target, &sha);
        return Ok(IngestedVoice {
            canonical_path,
            sha256: sha,
            display_file_name,
        });
    }

    let tmp = dir.join(format!("{sha}.part"));
    fs::copy(source, &tmp).map_err(|e| format!("tools.voiceFiles.readFailed|{e}"))?;
    if let Err(err) = fs::rename(&tmp, &target) {
        let _ = fs::remove_file(&tmp);
        if !target.is_file() {
            return Err(format!("tools.voiceFiles.readFailed|{err}"));
        }
    }

    remember_in_registry(&target, &sha);
    Ok(IngestedVoice {
        canonical_path,
        sha256: sha,
        display_file_name,
    })
}
