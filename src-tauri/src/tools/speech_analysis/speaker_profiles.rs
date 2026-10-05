//! Accumulate net speech / word counts across files for one speaker profile.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::settings::data_storage::default_data_storage_root;

const PROFILES_FILE: &str = "speech-speaker-profiles.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerProfileSummary {
    pub id: String,
    pub label: String,
    pub net_speech_duration_ms: u64,
    pub word_count: u64,
    pub file_count: u32,
    pub updated_at_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProfileDisk {
    profiles: Vec<SpeakerProfileSummary>,
}

fn profiles_path() -> PathBuf {
    default_data_storage_root()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(PROFILES_FILE)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn load_disk() -> ProfileDisk {
    let path = profiles_path();
    let Ok(bytes) = fs::read(&path) else {
        return ProfileDisk::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_disk(disk: &ProfileDisk) -> Result<(), String> {
    let path = profiles_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(disk).map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())
}

pub fn list_speaker_profiles() -> Vec<SpeakerProfileSummary> {
    let mut profiles = load_disk().profiles;
    profiles.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    profiles
}

pub fn get_speaker_profile(id: &str) -> Option<SpeakerProfileSummary> {
    load_disk()
        .profiles
        .into_iter()
        .find(|p| p.id == id)
}

pub fn ensure_speaker_profile(label: &str) -> Result<SpeakerProfileSummary, String> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err("tools.speechAnalysis.speakerProfile.emptyLabel".to_string());
    }
    let mut disk = load_disk();
    let id = format!("sp-{}", now_ms());
    let profile = SpeakerProfileSummary {
        id: id.clone(),
        label: trimmed.to_string(),
        net_speech_duration_ms: 0,
        word_count: 0,
        file_count: 0,
        updated_at_ms: now_ms(),
    };
    disk.profiles.push(profile.clone());
    save_disk(&disk)?;
    Ok(profile)
}

pub fn append_speaker_profile_file(
    id: &str,
    net_speech_ms: u64,
    word_count: usize,
) -> Result<SpeakerProfileSummary, String> {
    let mut disk = load_disk();
    let Some(entry) = disk.profiles.iter_mut().find(|p| p.id == id) else {
        return Err("tools.speechAnalysis.speakerProfile.notFound".to_string());
    };
    entry.net_speech_duration_ms = entry
        .net_speech_duration_ms
        .saturating_add(net_speech_ms);
    entry.word_count = entry
        .word_count
        .saturating_add(word_count as u64);
    entry.file_count = entry.file_count.saturating_add(1);
    entry.updated_at_ms = now_ms();
    let out = entry.clone();
    save_disk(&disk)?;
    Ok(out)
}
