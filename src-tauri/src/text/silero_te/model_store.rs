#![cfg(feature = "silero-te")]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use tracing::info;

use crate::error::ConfigError;
use crate::settings::AppSettings;
use crate::transcription::model_store::{download_client, DownloadProgress, resolve_models_dir};

use super::store::{self, SileroTeAssets, MODEL_FILE, META_FILE, TOKENIZER_FILE};

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
const DOWNLOAD_MAX_ATTEMPTS: u32 = 5;
const DOWNLOAD_RETRY_DELAY: Duration = Duration::from_secs(5);

/// GitHub release tag with `model.pt`, `tokenizer.pt`, and `meta.json` (see `scripts/package-silero-te-release.ps1`).
const RELEASE_TAG: &str = "silero-te-assets-v1";

pub const APPROX_DOWNLOAD_MB: u32 = 88;

#[derive(Clone, Serialize)]
pub struct SileroTeModelStatus {
    pub path: String,
    pub exists: bool,
    pub size_mb: u32,
    pub runtime_ready: bool,
    pub assets_ready: bool,
    pub runtime_path: String,
}

pub fn silero_te_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?.join("silero-te"))
}

pub fn assets_ready(settings: &AppSettings) -> bool {
    store::assets_on_disk(settings)
}

pub fn status(settings: &AppSettings) -> Result<SileroTeModelStatus, ConfigError> {
    use super::runtime_store;

    let dir = silero_te_dir(settings)?;
    let assets_ready = assets_ready(settings);
    let runtime_ready = runtime_store::runtime_ready(settings);
    let runtime_path = runtime_store::runtime_dir(settings)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let size_mb = if !runtime_ready {
        runtime_store::APPROX_RUNTIME_MB + APPROX_DOWNLOAD_MB
    } else if !assets_ready {
        APPROX_DOWNLOAD_MB
    } else {
        0
    };
    Ok(SileroTeModelStatus {
        path: dir.display().to_string(),
        exists: assets_ready && runtime_ready,
        size_mb,
        runtime_ready,
        assets_ready,
        runtime_path,
    })
}

struct AssetSpec {
    file_name: &'static str,
    min_bytes: u64,
}

const ASSETS: [AssetSpec; 3] = [
    AssetSpec {
        file_name: MODEL_FILE,
        min_bytes: 1_000_000,
    },
    AssetSpec {
        file_name: TOKENIZER_FILE,
        min_bytes: 100_000,
    },
    AssetSpec {
        file_name: META_FILE,
        min_bytes: 1_000,
    },
];

const GITHUB_RELEASE_BASE: &str =
    "https://github.com/WtekSupport/veyro/releases/download";

/// Stable total for multi-file asset progress (model + tokenizer + meta).
const ASSETS_ESTIMATE_BYTES: u64 = 95_000_000;

fn download_urls(file_name: &str) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(base) = std::env::var("VEYRO_SILERO_TE_BASE_URL") {
        let base = base.trim_end_matches('/');
        if !base.is_empty() {
            urls.push(format!("{base}/{file_name}"));
        }
    }
    urls.push(format!("{GITHUB_RELEASE_BASE}/{RELEASE_TAG}/{file_name}"));
    urls
}

fn is_not_found_error(message: &str) -> bool {
    message.contains("404") || message.contains("Not Found")
}

/// Debug builds: copy from `src-tauri/resources/silero-te` after `extract-silero-te.py`.
fn try_seed_dev_resources(dir: &Path) -> Result<bool, String> {
    #[cfg(not(debug_assertions))]
    {
        let _ = dir;
        return Ok(false);
    }
    #[cfg(debug_assertions)]
    {
        let Some(manifest) = option_env!("CARGO_MANIFEST_DIR").map(PathBuf::from) else {
            return Ok(false);
        };
        let src = manifest.join("resources").join("silero-te");
        let assets = SileroTeAssets {
            model: src.join(MODEL_FILE),
            tokenizer: src.join(TOKENIZER_FILE),
            meta: src.join(META_FILE),
        };
        if !store::assets_ready_public(&assets) {
            return Ok(false);
        }
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        for name in [MODEL_FILE, TOKENIZER_FILE, META_FILE] {
            fs::copy(src.join(name), dir.join(name)).map_err(|error| error.to_string())?;
        }
        info!(
            "seeded Silero TE assets from {} into {}",
            src.display(),
            dir.display()
        );
        Ok(true)
    }
}

pub async fn download_assets<F>(
    http: &Client,
    settings: &AppSettings,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    let dir = silero_te_dir(settings).map_err(|error| error.to_string())?;
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;

    if store::assets_on_disk(settings) {
        return Ok(dir);
    }

    on_progress(DownloadProgress::new(0, None));
    tokio::time::sleep(Duration::from_millis(50)).await;

    if try_seed_dev_resources(&dir)? {
        let total = ASSETS.iter().map(|spec| spec.min_bytes).sum();
        on_progress(DownloadProgress::new(total, Some(total)));
        tokio::time::sleep(Duration::from_millis(50)).await;
        return Ok(dir);
    }

    let mut completed_bytes = 0_u64;

    for spec in &ASSETS {
        let dest = dir.join(spec.file_name);
        info!("downloading Silero TE asset {} to {}", spec.file_name, dest.display());
        let file_bytes = download_one(http, spec.file_name, &dest, spec.min_bytes, |file_progress| {
            let combined = completed_bytes + file_progress.downloaded;
            on_progress(DownloadProgress::new(combined, Some(ASSETS_ESTIMATE_BYTES)));
        })
        .await?;
        completed_bytes += file_bytes;
        on_progress(DownloadProgress::new(
            completed_bytes.min(ASSETS_ESTIMATE_BYTES),
            Some(ASSETS_ESTIMATE_BYTES),
        ));
    }

    let assets = SileroTeAssets {
        model: dir.join(MODEL_FILE),
        tokenizer: dir.join(TOKENIZER_FILE),
        meta: dir.join(META_FILE),
    };
    if !store::assets_ready_public(&assets) {
        return Err("Silero TE download finished but files are missing or invalid".to_string());
    }

    Ok(dir)
}

async fn download_one<F>(
    http: &Client,
    file_name: &str,
    dest: &Path,
    min_bytes: u64,
    mut on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(DownloadProgress),
{
    let partial = dest.with_extension("part");
    if partial.exists() {
        fs::remove_file(&partial).map_err(|error| error.to_string())?;
    }

    let urls = download_urls(file_name);
    let mut last_error = String::new();

    for url in urls {
        for attempt in 1..=DOWNLOAD_MAX_ATTEMPTS {
            match download_one_attempt(http, &url, dest, &partial, min_bytes, &mut on_progress).await
            {
                Ok(bytes) => return Ok(bytes),
                Err(error) => {
                    last_error = error.clone();
                    if is_not_found_error(&error) {
                        tracing::warn!("Silero TE asset not at {url}, trying next source");
                        break;
                    }
                    if attempt < DOWNLOAD_MAX_ATTEMPTS {
                        tracing::warn!(
                            "retrying Silero TE download from {url} (attempt {attempt}/{DOWNLOAD_MAX_ATTEMPTS})"
                        );
                        tokio::time::sleep(DOWNLOAD_RETRY_DELAY).await;
                    }
                }
            }
        }
    }

    Err(format!(
        "{last_error}. \
         GitHub release `{RELEASE_TAG}` is missing Silero TE files — run \
         `python scripts/extract-silero-te.py` then `scripts/package-silero-te-release.ps1 -RepoRoot .` \
         (requires `gh auth login`). Override with VEYRO_SILERO_TE_BASE_URL for testing."
    ))
}

async fn download_one_attempt<F>(
    http: &Client,
    url: &str,
    dest: &Path,
    partial: &Path,
    min_bytes: u64,
    on_progress: &mut F,
) -> Result<u64, String>
where
    F: FnMut(DownloadProgress),
{
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "HTTP {} while downloading Silero TE asset from {url}",
            response.status()
        ));
    }

    let total = response.content_length();
    let mut downloaded = 0_u64;
    let mut last_emit = Instant::now();
    let mut stream = response.bytes_stream();
    let mut file = fs::File::create(partial).map_err(|error| error.to_string())?;

    let mut emit = |downloaded: u64, total: Option<u64>, force: bool| {
        if force || last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
            on_progress(DownloadProgress::new(downloaded, total));
            last_emit = Instant::now();
        }
    };

    emit(0, total, true);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        file.write_all(&chunk).map_err(|error| error.to_string())?;
        downloaded += chunk.len() as u64;
        emit(downloaded, total, false);
    }

    drop(file);
    emit(downloaded, total, true);

    if downloaded < min_bytes {
        let _ = fs::remove_file(partial);
        return Err(format!(
            "downloaded file too small ({downloaded} bytes, expected at least {min_bytes}) — check release URL"
        ));
    }

    if looks_like_html_error_page(partial) {
        let _ = fs::remove_file(partial);
        return Err(format!(
            "download from {url} returned an HTML page instead of a model file"
        ));
    }

    if dest.exists() {
        fs::remove_file(dest).map_err(|error| error.to_string())?;
    }
    fs::rename(partial, dest).map_err(|error| error.to_string())?;
    Ok(downloaded)
}

fn looks_like_html_error_page(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let head = bytes.iter().take(512).copied().collect::<Vec<_>>();
    let Ok(text) = std::str::from_utf8(&head) else {
        return false;
    };
    let trimmed = text.trim_start();
    trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<html")
}

pub fn download_http_client() -> Result<Client, String> {
    download_client()
}
