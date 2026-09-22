#![cfg_attr(not(feature = "silero-te"), allow(dead_code))]

use std::path::{Path, PathBuf};

use crate::error::ConfigError;
use crate::settings::AppSettings;
use crate::transcription::model_store::resolve_models_dir;

pub(crate) const MODEL_FILE: &str = "model.pt";
pub(crate) const TOKENIZER_FILE: &str = "tokenizer.pt";
pub(crate) const META_FILE: &str = "meta.json";

pub struct SileroTeAssets {
    pub model: PathBuf,
    pub tokenizer: PathBuf,
    pub meta: PathBuf,
}

pub fn silero_te_assets_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?.join("silero-te"))
}

fn bundle_paths(dir: &Path) -> SileroTeAssets {
    SileroTeAssets {
        model: dir.join(MODEL_FILE),
        tokenizer: dir.join(TOKENIZER_FILE),
        meta: dir.join(META_FILE),
    }
}

fn assets_ready(assets: &SileroTeAssets) -> bool {
    assets.model.is_file() && assets.tokenizer.is_file() && assets.meta.is_file()
}

pub fn assets_on_disk(settings: &AppSettings) -> bool {
    silero_te_assets_dir(settings)
        .ok()
        .map(|dir| assets_ready(&bundle_paths(&dir)))
        .unwrap_or(false)
}

pub fn resolve_assets(settings: &AppSettings) -> Result<SileroTeAssets, String> {
    let dir = silero_te_assets_dir(settings).map_err(|error: ConfigError| error.to_string())?;
    let assets = bundle_paths(&dir);
    if assets_ready(&assets) {
        return Ok(assets);
    }
    Err(format!(
        "Silero TE model not found at {} — download required",
        dir.display()
    ))
}

#[cfg(feature = "silero-te")]
pub(crate) fn assets_ready_public(assets: &SileroTeAssets) -> bool {
    assets_ready(assets)
}
