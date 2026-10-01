#![cfg(feature = "local-separation")]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use reqwest::StatusCode;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tracing::info;

use crate::error::ConfigError;
use crate::network::retry::is_transient_status;
use crate::settings::vocal_separator::VocalSeparatorProfile;
use crate::settings::AppSettings;
use crate::transcription::model_store::{
    authorized_download_request, download_client, DownloadProgress, resolve_models_dir,
};

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
const DOWNLOAD_MAX_ATTEMPTS: u32 = 5;
const DOWNLOAD_RETRY_DELAY: Duration = Duration::from_secs(5);

const SILVERDAW_ONNX: &str = "syhft_core_t1100.onnx";
const SILVERDAW_DATA: &str = "syhft_core_t1100.onnx.data";
const SILVERDAW_PAGE: &str = "https://huggingface.co/musetric/vocal-separation-roformer-onnx";

struct ExtraDownload {
    file_name: &'static str,
    url: &'static str,
    min_bytes: u64,
}

struct ModelSpec {
    profile: VocalSeparatorProfile,
    file_name: &'static str,
    url: &'static str,
    page_url: &'static str,
    extras: &'static [ExtraDownload],
    /// When set, weights live under the shared profile directory (Fast reuses Quality).
    shared_from: Option<VocalSeparatorProfile>,
    min_bytes: u64,
    expected_sha256: Option<&'static str>,
    download_size_mb: u32,
    ram_mb: u32,
    vram_mb: Option<u32>,
}

const SILVERDAW_EXTRAS: [ExtraDownload; 1] = [ExtraDownload {
    file_name: SILVERDAW_DATA,
    url: "https://huggingface.co/musetric/vocal-separation-roformer-onnx/resolve/main/syhft_core_t1100.onnx.data",
    min_bytes: 700_000_000,
}];

const MODELS: [ModelSpec; 4] = [
    ModelSpec {
        profile: VocalSeparatorProfile::Quality,
        file_name: SILVERDAW_ONNX,
        url: "https://huggingface.co/musetric/vocal-separation-roformer-onnx/resolve/main/syhft_core_t1100.onnx",
        page_url: SILVERDAW_PAGE,
        extras: &SILVERDAW_EXTRAS,
        shared_from: None,
        min_bytes: 8_000_000,
        expected_sha256: None,
        download_size_mb: 720,
        ram_mb: 12 * 1024,
        vram_mb: Some(8 * 1024),
    },
    ModelSpec {
        profile: VocalSeparatorProfile::Fast,
        file_name: SILVERDAW_ONNX,
        url: "https://huggingface.co/musetric/vocal-separation-roformer-onnx/resolve/main/syhft_core_t1100.onnx",
        page_url: SILVERDAW_PAGE,
        extras: &SILVERDAW_EXTRAS,
        shared_from: Some(VocalSeparatorProfile::Quality),
        min_bytes: 8_000_000,
        expected_sha256: None,
        download_size_mb: 720,
        ram_mb: 10 * 1024,
        vram_mb: Some(6 * 1024),
    },
    ModelSpec {
        profile: VocalSeparatorProfile::Legacy,
        file_name: "htdemucs_ft_vocals_fp16weights.onnx",
        url: "https://huggingface.co/StemSplitio/htdemucs-ft-vocals-onnx/resolve/main/htdemucs_ft_vocals_fp16weights.onnx",
        page_url: "https://huggingface.co/StemSplitio/htdemucs-ft-vocals-onnx",
        extras: &[],
        shared_from: None,
        min_bytes: 150_000_000,
        expected_sha256: None,
        download_size_mb: 166,
        ram_mb: 4 * 1024,
        vram_mb: Some(3 * 1024),
    },
    // Practical max open multi-stem model today (not an architectural limit).
    ModelSpec {
        profile: VocalSeparatorProfile::MultiStem,
        file_name: "htdemucs_6s_fp16weights.onnx",
        url: "https://huggingface.co/StemSplitio/htdemucs-6s-onnx/resolve/main/htdemucs_6s_fp16weights.onnx",
        page_url: "https://huggingface.co/StemSplitio/htdemucs-6s-onnx",
        extras: &[],
        shared_from: None,
        min_bytes: 120_000_000,
        expected_sha256: None,
        download_size_mb: 136,
        ram_mb: 4 * 1024,
        vram_mb: Some(3 * 1024),
    },
];

fn spec(profile: VocalSeparatorProfile) -> &'static ModelSpec {
    MODELS
        .iter()
        .find(|entry| entry.profile == profile)
        .expect("profile spec")
}

fn storage_profile(profile: VocalSeparatorProfile) -> VocalSeparatorProfile {
    spec(profile)
        .shared_from
        .unwrap_or(profile)
}

fn storage_spec(profile: VocalSeparatorProfile) -> &'static ModelSpec {
    spec(storage_profile(profile))
}

pub fn model_dir(settings: &AppSettings, profile: VocalSeparatorProfile) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?
        .join("separation")
        .join(storage_profile(profile).as_str()))
}

pub fn model_path(settings: &AppSettings, profile: VocalSeparatorProfile) -> Result<PathBuf, ConfigError> {
    let entry = storage_spec(profile);
    Ok(model_dir(settings, profile)?.join(entry.file_name))
}

fn extra_paths(dir: &Path, entry: &ModelSpec) -> Vec<PathBuf> {
    entry
        .extras
        .iter()
        .map(|extra| dir.join(extra.file_name))
        .collect()
}

pub fn bundle_ready(path: &Path, profile: VocalSeparatorProfile) -> bool {
    let entry = storage_spec(profile);
    if !path.is_file() {
        return false;
    }
    if !path
        .metadata()
        .map(|meta| meta.len() >= entry.min_bytes)
        .unwrap_or(false)
    {
        return false;
    }
    let dir = match path.parent() {
        Some(parent) => parent,
        None => return false,
    };
    for (extra, spec_extra) in extra_paths(dir, entry).iter().zip(entry.extras.iter()) {
        if !extra.is_file() {
            return false;
        }
        if !extra
            .metadata()
            .map(|meta| meta.len() >= spec_extra.min_bytes)
            .unwrap_or(false)
        {
            return false;
        }
    }
    true
}

pub fn bundle_ready_for_settings(settings: &AppSettings, profile: VocalSeparatorProfile) -> bool {
    model_path(settings, profile)
        .ok()
        .is_some_and(|path| bundle_ready(&path, profile))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeparationModelStatus {
    pub profile: VocalSeparatorProfile,
    pub path: String,
    pub exists: bool,
    pub download_size_mb: u32,
    pub ram_mb: u32,
    pub vram_mb: Option<u32>,
    pub selected: bool,
    pub hf_model_page_url: String,
}

pub fn list_status(settings: &AppSettings) -> Result<Vec<SeparationModelStatus>, ConfigError> {
    let selected = settings.vocal_separator_profile;
    Ok(MODELS
        .iter()
        .map(|entry| {
            let path = model_path(settings, entry.profile).unwrap_or_default();
            SeparationModelStatus {
                profile: entry.profile,
                path: path.display().to_string(),
                exists: bundle_ready(&path, entry.profile),
                download_size_mb: entry.download_size_mb,
                ram_mb: entry.ram_mb,
                vram_mb: entry.vram_mb,
                selected: entry.profile == selected,
                hf_model_page_url: entry.page_url.to_string(),
            }
        })
        .collect())
}

pub async fn download_model(
    settings: &AppSettings,
    profile: VocalSeparatorProfile,
    on_progress: impl Fn(DownloadProgress) + Send + Sync + 'static,
) -> Result<PathBuf, String> {
    let entry = storage_spec(profile);
    let dir = model_dir(settings, profile).map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(entry.file_name);
    if bundle_ready(&dest, profile) {
        return Ok(dest);
    }

    let client = download_client()?;
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match download_bundle(&client, entry, &dir, &dest, &on_progress).await {
            Ok(()) if bundle_ready(&dest, profile) => {
                if let Some(expected) = entry.expected_sha256 {
                    validate_sha256(&dest, expected)?;
                }
                info!(path = %dest.display(), "separation model ready");
                return Ok(dest);
            }
            Ok(()) => {
                cleanup_partial_bundle(&dir, entry, &dest);
                if attempt >= DOWNLOAD_MAX_ATTEMPTS {
                    return Err("tools.vocalSeparator.downloadInvalid".to_string());
                }
            }
            Err(error)
                if error == "tools.vocalSeparator.downloadUnauthorized"
                    || error.starts_with("tools.vocalSeparator.downloadFailed|HTTP 404") =>
            {
                return Err(error);
            }
            Err(error) if attempt >= DOWNLOAD_MAX_ATTEMPTS => return Err(error),
            Err(_) => {
                cleanup_partial_bundle(&dir, entry, &dest);
            }
        }
        tokio::time::sleep(DOWNLOAD_RETRY_DELAY).await;
    }
}

fn cleanup_partial_bundle(dir: &Path, entry: &ModelSpec, dest: &Path) {
    let _ = fs::remove_file(dest);
    for path in extra_paths(dir, entry) {
        let _ = fs::remove_file(path);
    }
}

async fn download_bundle(
    client: &Client,
    entry: &ModelSpec,
    dir: &Path,
    dest: &Path,
    on_progress: &(impl Fn(DownloadProgress) + Send + Sync),
) -> Result<(), String> {
    download_to_file(client, entry.url, dest, on_progress).await?;
    for extra in entry.extras {
        let extra_dest = dir.join(extra.file_name);
        if extra_dest.is_file()
            && extra_dest
                .metadata()
                .map(|meta| meta.len() >= extra.min_bytes)
                .unwrap_or(false)
        {
            continue;
        }
        download_to_file(client, extra.url, &extra_dest, on_progress).await?;
    }
    Ok(())
}

async fn download_to_file(
    client: &Client,
    url: &str,
    dest: &Path,
    on_progress: &(impl Fn(DownloadProgress) + Send + Sync),
) -> Result<(), String> {
    let mut last_error = String::new();
    for attempt in 1..=DOWNLOAD_MAX_ATTEMPTS {
        match authorized_download_request(client, url).send().await {
            Ok(response) if response.status().is_success() => {
                return stream_response_to_file(response, dest, on_progress).await;
            }
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                last_error = format_separation_download_error(status, &body);
                if status == StatusCode::NOT_FOUND
                    || status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::FORBIDDEN
                {
                    return Err(last_error);
                }
                if !is_transient_status(status.as_u16()) {
                    return Err(last_error);
                }
            }
            Err(error) => {
                last_error = format!(
                    "tools.vocalSeparator.downloadFailed|network error: {error}"
                );
            }
        }
        if attempt < DOWNLOAD_MAX_ATTEMPTS {
            tokio::time::sleep(DOWNLOAD_RETRY_DELAY).await;
        }
    }
    Err(last_error)
}

async fn stream_response_to_file(
    response: reqwest::Response,
    dest: &Path,
    on_progress: &(impl Fn(DownloadProgress) + Send + Sync),
) -> Result<(), String> {
    let total = response.content_length();
    let mut stream = response.bytes_stream();
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut downloaded = 0u64;
    let mut last_emit = Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
            on_progress(DownloadProgress::new(downloaded, total));
            last_emit = Instant::now();
        }
    }
    on_progress(DownloadProgress::new(downloaded, total));
    Ok(())
}

fn format_separation_download_error(status: StatusCode, body: &str) -> String {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return "tools.vocalSeparator.downloadUnauthorized".to_string();
    }
    let detail = body.trim().lines().next().unwrap_or(body.trim());
    if detail.is_empty() {
        format!("tools.vocalSeparator.downloadFailed|HTTP {}", status.as_u16())
    } else {
        format!(
            "tools.vocalSeparator.downloadFailed|HTTP {} ({detail})",
            status.as_u16()
        )
    }
}

fn validate_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(bytes);
    let hex = hex::encode(digest);
    if hex.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        let _ = fs::remove_file(path);
        Err("tools.vocalSeparator.downloadInvalid".to_string())
    }
}
