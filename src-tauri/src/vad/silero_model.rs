#![cfg(feature = "vad-silero")]

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

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
const DOWNLOAD_MAX_ATTEMPTS: u32 = 5;
const DOWNLOAD_RETRY_DELAY: Duration = Duration::from_secs(5);

pub const ONNX_FILE: &str = "silero_vad.onnx";
pub const APPROX_DOWNLOAD_MB: u32 = 3;

const DEFAULT_MODEL_URL: &str =
    "https://github.com/snakers4/silero-vad/raw/v6.2.1/src/silero_vad/data/silero_vad.onnx";

#[derive(Clone, Serialize)]
pub struct SileroVadModelStatus {
    pub path: String,
    pub exists: bool,
    pub size_mb: u32,
}

pub fn silero_vad_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?.join("silero-vad"))
}

pub fn model_path(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(silero_vad_dir(settings)?.join(ONNX_FILE))
}

pub fn model_exists(settings: &AppSettings) -> bool {
    model_path(settings)
        .ok()
        .is_some_and(|path| path.is_file() && path.metadata().map(|m| m.len() > 100_000).unwrap_or(false))
}

pub fn status(settings: &AppSettings) -> Result<SileroVadModelStatus, ConfigError> {
    let path = model_path(settings)?;
    Ok(SileroVadModelStatus {
        path: path.display().to_string(),
        exists: model_exists(settings),
        size_mb: APPROX_DOWNLOAD_MB,
    })
}

pub fn download_url() -> String {
    std::env::var("VEYRO_SILERO_VAD_MODEL_URL").unwrap_or_else(|_| DEFAULT_MODEL_URL.to_string())
}

pub async fn download_model<F>(
    http: &Client,
    settings: &AppSettings,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    let path = model_path(settings).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let url = download_url();
    info!("downloading Silero VAD model to {}", path.display());

    let partial = path.with_extension("part");
    if partial.exists() {
        fs::remove_file(&partial).map_err(|error| error.to_string())?;
    }

    let mut last_error = String::new();
    for attempt in 1..=DOWNLOAD_MAX_ATTEMPTS {
        match download_attempt(http, &url, &path, &partial, &mut on_progress).await {
            Ok(()) => return Ok(path),
            Err(error) => {
                last_error = error;
                if attempt < DOWNLOAD_MAX_ATTEMPTS {
                    tracing::warn!(
                        "retrying Silero VAD download (attempt {attempt}/{DOWNLOAD_MAX_ATTEMPTS})"
                    );
                    tokio::time::sleep(DOWNLOAD_RETRY_DELAY).await;
                }
            }
        }
    }
    Err(last_error)
}

async fn download_attempt<F>(
    http: &Client,
    url: &str,
    dest: &Path,
    partial: &Path,
    on_progress: &mut F,
) -> Result<(), String>
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
            "HTTP {} while downloading Silero VAD from {url}",
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

    if downloaded < 100_000 {
        let _ = fs::remove_file(partial);
        return Err(format!(
            "Silero VAD download too small ({downloaded} bytes)"
        ));
    }

    if dest.exists() {
        fs::remove_file(dest).map_err(|error| error.to_string())?;
    }
    fs::rename(partial, dest).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn download_http_client() -> Result<Client, String> {
    download_client()
}
