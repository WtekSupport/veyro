use std::path::{Path, PathBuf};

use crate::settings::data_storage::resolve_data_storage_root;
use crate::settings::AppSettings;

fn storage_root(settings: &AppSettings) -> Result<std::path::PathBuf, String> {
    resolve_data_storage_root(settings).map_err(|error| error.to_string())
}

use super::types::SpeechAnalysisReport;

const CACHE_DIR: &str = "speech_analysis_cache";

fn cache_dir(settings: &AppSettings) -> Result<PathBuf, String> {
    Ok(storage_root(settings)?.join(CACHE_DIR))
}

fn cache_path(settings: &AppSettings, path_key: &str) -> Result<PathBuf, String> {
    let safe: String = path_key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    Ok(cache_dir(settings)?.join(format!("{safe}.json")))
}

pub fn save_report_snapshot(settings: &AppSettings, report: &SpeechAnalysisReport) -> Result<(), String> {
    let dir = cache_dir(settings)?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = cache_path(settings, &report.path_key)?;
    let json = serde_json::to_string_pretty(report).map_err(|error| error.to_string())?;
    std::fs::write(path, json).map_err(|error| error.to_string())
}

pub fn load_report_snapshot(
    settings: &AppSettings,
    path_key: &str,
) -> Result<Option<SpeechAnalysisReport>, String> {
    let path = cache_path(settings, path_key)?;
    if !Path::new(&path).exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|error| error.to_string())
}

