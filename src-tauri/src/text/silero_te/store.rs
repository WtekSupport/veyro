#![cfg_attr(not(feature = "silero-te"), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::error::ConfigError;
use crate::settings::AppSettings;
use crate::transcription::model_store::resolve_models_dir;

static BUNDLED_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_bundled_dir(path: PathBuf) {
    let _ = BUNDLED_DIR.set(path);
}

const MODEL_FILE: &str = "model.pt";
const TOKENIZER_FILE: &str = "tokenizer.pt";
const META_FILE: &str = "meta.json";

pub struct SileroTeAssets {
    pub model: PathBuf,
    pub tokenizer: PathBuf,
    pub meta: PathBuf,
}

pub fn resolve_assets(settings: &AppSettings) -> Result<SileroTeAssets, String> {
    let user_dir = resolve_models_dir(settings)
        .map_err(|error: ConfigError| error.to_string())?
        .join("silero-te");
    let user = bundle_paths(&user_dir);
    if assets_ready(&user) {
        return Ok(user);
    }

    if let Some(resource) = BUNDLED_DIR.get() {
        let bundled = bundle_paths(resource);
        if assets_ready(&bundled) {
            return Ok(bundled);
        }
    }
    if let Ok(resource) = resource_bundle_dir() {
        let bundled = bundle_paths(&resource);
        if assets_ready(&bundled) {
            return Ok(bundled);
        }
    }

    Err(format!(
        "Silero TE model files not found (expected {} under {:?} or app resources)",
        MODEL_FILE, user_dir
    ))
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

fn resource_bundle_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut dir = exe
        .parent()
        .ok_or_else(|| "executable has no parent directory".to_string())?
        .to_path_buf();
    for _ in 0..6 {
        let candidate = dir.join("resources").join("silero-te");
        if candidate.join(MODEL_FILE).is_file() {
            return Ok(candidate);
        }
        let candidate = dir.join("silero-te");
        if candidate.join(MODEL_FILE).is_file() {
            return Ok(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    Err("Silero TE resources not found next to executable".to_string())
}
