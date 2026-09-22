use std::path::{Path, PathBuf};

use crate::error::ConfigError;
use crate::settings::AppSettings;

pub const SUBDIR_STT_MODELS: &str = "models";
pub const SUBDIR_LLM_MODELS: &str = "llm-models";
pub const SUBDIR_DICTIONARY: &str = "dictionary";
pub const SUBDIR_SEGMENT_QUEUE: &str = "segment-queue";
pub const DICTIONARY_FILE: &str = "dictionary.toml";

pub fn default_data_storage_root() -> Result<PathBuf, ConfigError> {
    let base = dirs::config_dir().ok_or_else(|| {
        ConfigError::Read("unable to resolve OS config directory".to_string())
    })?;
    Ok(base.join("Veyro"))
}

pub fn resolve_data_storage_root(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    if let Some(dir) = settings
        .data_storage_dir
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(dir));
    }
    default_data_storage_root()
}

pub fn resolve_stt_models_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    if uses_unified_storage(settings) {
        return Ok(resolve_data_storage_root(settings)?.join(SUBDIR_STT_MODELS));
    }
    if let Some(dir) = settings
        .local_whisper_models_dir
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(dir));
    }
    Ok(default_data_storage_root()?.join(SUBDIR_STT_MODELS))
}

pub fn resolve_llm_models_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    if uses_unified_storage(settings) {
        return Ok(resolve_data_storage_root(settings)?.join(SUBDIR_LLM_MODELS));
    }
    if let Some(dir) = settings
        .local_llm_models_dir
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(dir));
    }
    Ok(default_data_storage_root()?.join(SUBDIR_LLM_MODELS))
}

pub fn resolve_dictionary_path(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    if uses_unified_storage(settings) {
        return Ok(
            resolve_data_storage_root(settings)?
                .join(SUBDIR_DICTIONARY)
                .join(DICTIONARY_FILE),
        );
    }
    if let Some(path) = settings
        .transcription_dictionary_path
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(path));
    }
    Ok(default_data_storage_root()?.join(SUBDIR_DICTIONARY).join(DICTIONARY_FILE))
}

fn uses_unified_storage(settings: &AppSettings) -> bool {
    settings
        .data_storage_dir
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

pub fn ensure_data_storage_layout(root: &Path) -> Result<(), ConfigError> {
    for sub in [
        SUBDIR_STT_MODELS,
        SUBDIR_LLM_MODELS,
        SUBDIR_DICTIONARY,
        SUBDIR_SEGMENT_QUEUE,
    ] {
        let path = root.join(sub);
        std::fs::create_dir_all(&path).map_err(|error| {
            ConfigError::Write(format!("{}: {error}", path.display()))
        })?;
    }
    Ok(())
}

pub fn normalize_data_storage(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    if uses_unified_storage(settings) {
        if settings.local_whisper_models_dir.is_some() {
            settings.local_whisper_models_dir = None;
            changed = true;
        }
        if settings.local_llm_models_dir.is_some() {
            settings.local_llm_models_dir = None;
            changed = true;
        }
        if settings.transcription_dictionary_path.is_some() {
            settings.transcription_dictionary_path = None;
            changed = true;
        }
    }
    changed
}
