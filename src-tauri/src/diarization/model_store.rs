//! Diarization model cache under `{models}/diarization` via polyvoice ModelRegistry.

use std::path::PathBuf;

use serde::Serialize;

use crate::error::ConfigError;
use crate::settings::AppSettings;
use crate::transcription::model_store::resolve_models_dir;

/// Approximate on-disk size of the Balanced INT8 pair + VBx PLDA (~8.4 MB pair + extras).
pub const APPROX_DOWNLOAD_MB: u32 = 12;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationModelStatus {
    pub path: String,
    pub exists: bool,
    pub size_mb: u32,
    pub available: bool,
}

pub fn diarization_cache_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?.join("diarization"))
}

#[cfg(feature = "local-diarization")]
pub fn model_ready(settings: &AppSettings) -> bool {
    let Ok(dir) = diarization_cache_dir(settings) else {
        return false;
    };
    if !dir.is_dir() {
        return false;
    }
    // Polyvoice stores verified weights under the cache; require at least one sizable file.
    std::fs::read_dir(&dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .any(|entry| {
                    entry
                        .metadata()
                        .map(|meta| meta.is_file() && meta.len() > 100_000)
                        .unwrap_or(false)
                })
        })
        .unwrap_or(false)
}

#[cfg(not(feature = "local-diarization"))]
pub fn model_ready(_settings: &AppSettings) -> bool {
    false
}

pub fn status(settings: &AppSettings) -> Result<DiarizationModelStatus, ConfigError> {
    let path = diarization_cache_dir(settings)?;
    Ok(DiarizationModelStatus {
        path: path.display().to_string(),
        exists: model_ready(settings),
        size_mb: APPROX_DOWNLOAD_MB,
        available: cfg!(feature = "local-diarization"),
    })
}

/// Ensure Balanced profile weights are present (downloads if missing).
#[cfg(feature = "local-diarization")]
pub fn ensure_models(settings: &AppSettings) -> Result<PathBuf, String> {
    use polyvoice::models::ModelRegistry;
    use polyvoice::types::Profile;

    let dir = diarization_cache_dir(settings).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let registry = ModelRegistry::with_cache_dir(&dir).map_err(|error| error.to_string())?;
    let _models = registry
        .ensure_for_profile(Profile::Balanced)
        .map_err(|error| format!("tools.audioSrt.diarizationDownloadFailed|{error}"))?;
    let _ = registry.ensure_vbx_plda_dir();
    Ok(dir)
}

#[cfg(not(feature = "local-diarization"))]
pub fn ensure_models(_settings: &AppSettings) -> Result<PathBuf, String> {
    Err("tools.audioSrt.diarizationUnavailable".to_string())
}
