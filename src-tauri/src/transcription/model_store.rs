use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use reqwest::StatusCode;
use serde::Serialize;
use tracing::info;

use crate::error::ConfigError;
use crate::network::retry::is_transient_status;
use crate::settings::{AppSettings, WhisperModelKind};

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
const DOWNLOAD_USER_AGENT: &str = "Veyro/1.0 (+https://github.com/ggerganov/whisper.cpp; model downloader)";
const DOWNLOAD_MAX_ATTEMPTS: u32 = 5;
const DOWNLOAD_RETRY_DELAY: Duration = Duration::from_secs(5);
// ggml tensor files store magic 0x67676d6c as bytes 6c 6d 67 67 on disk.
const GGML_FILE_MAGIC: [u8; 4] = [0x6c, 0x6d, 0x67, 0x67];

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: Option<f32>,
}

#[derive(Clone, Serialize)]
pub struct WhisperModelInfo {
    pub kind: WhisperModelKind,
    pub path: String,
    pub exists: bool,
    pub size_mb: u32,
    pub selected: bool,
}

impl DownloadProgress {
    fn new(downloaded: u64, total: Option<u64>) -> Self {
        let percent = total.filter(|value| *value > 0).map(|value| {
            ((downloaded as f64 / value as f64) * 100.0).min(100.0) as f32
        });
        Self {
            downloaded,
            total,
            percent,
        }
    }
}

pub fn default_models_dir() -> Result<PathBuf, ConfigError> {
    let base = dirs::config_dir().ok_or_else(|| {
        ConfigError::Read("unable to resolve OS config directory".to_string())
    })?;
    Ok(base.join("Veyro").join("models"))
}

pub fn resolve_models_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    if let Some(dir) = settings
        .local_whisper_models_dir
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(dir));
    }
    default_models_dir()
}

pub fn models_dir_for(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    resolve_models_dir(settings)
}

pub fn model_path_for(settings: &AppSettings, kind: WhisperModelKind) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?.join(kind.file_name()))
}

pub fn resolve_model_path(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    model_path_for(settings, settings.local_whisper_model)
}

pub fn model_exists(path: &Path) -> bool {
    path.is_file()
}

pub fn list_models(settings: &AppSettings) -> Result<Vec<WhisperModelInfo>, ConfigError> {
    let selected = settings.local_whisper_model;
    WhisperModelKind::all()
        .into_iter()
        .map(|kind| {
            let path = model_path_for(settings, kind)?;
            Ok(WhisperModelInfo {
                kind,
                path: path.display().to_string(),
                exists: model_exists(&path),
                size_mb: kind.approx_size_mb(),
                selected: kind == selected,
            })
        })
        .collect()
}

pub fn download_client() -> Result<Client, String> {
    Client::builder()
        .user_agent(DOWNLOAD_USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        // Large models (medium/large-v3) can take a long time on slow links.
        .timeout(Duration::from_secs(6 * 60 * 60))
        .build()
        .map_err(|error| error.to_string())
}

pub async fn download_model<F>(
    http: &Client,
    settings: &AppSettings,
    kind: WhisperModelKind,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    let path = model_path_for(settings, kind).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let partial_path = path.with_extension("bin.part");
    if partial_path.exists() {
        fs::remove_file(&partial_path).map_err(|error| error.to_string())?;
    }

    let download_url = kind.download_url();
    info!(
        "downloading whisper model {:?} ({}) to {}",
        kind,
        kind.file_name(),
        path.display()
    );

    let response = fetch_model_response(http, kind, &download_url).await?;

    let total = response.content_length();
    let mut downloaded = 0_u64;
    let mut last_emit = Instant::now();
    let mut stream = response.bytes_stream();
    let mut file = fs::File::create(&partial_path).map_err(|error| error.to_string())?;
    let mut header = [0_u8; 4];
    let mut header_len = 0_usize;

    let mut emit_progress = |downloaded: u64, total: Option<u64>, force: bool, on_progress: &mut F| {
        if force || last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
            on_progress(DownloadProgress::new(downloaded, total));
            last_emit = Instant::now();
        }
    };

    emit_progress(downloaded, total, true, &mut on_progress);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if header_len < header.len() {
            let take = (header.len() - header_len).min(chunk.len());
            header[header_len..header_len + take].copy_from_slice(&chunk[..take]);
            header_len += take;
        }
        file.write_all(&chunk).map_err(|error| error.to_string())?;
        downloaded += chunk.len() as u64;
        emit_progress(downloaded, total, false, &mut on_progress);
    }

    drop(file);
    emit_progress(downloaded, total, true, &mut on_progress);

    if header_len < header.len() || header != GGML_FILE_MAGIC {
        let _ = fs::remove_file(&partial_path);
        return Err(format!(
            "downloaded file is not a valid ggml Whisper model. \
             Hugging Face may have returned an error page instead of {}.",
            kind.file_name()
        ));
    }

    if let Some(expected) = total {
        if downloaded != expected {
            let _ = fs::remove_file(&partial_path);
            return Err(format!(
                "model download incomplete: expected {expected} bytes, got {downloaded}"
            ));
        }
    }

    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    fs::rename(&partial_path, &path).map_err(|error| error.to_string())?;
    info!("whisper model downloaded ({} bytes)", downloaded);
    Ok(path)
}

async fn fetch_model_response(
    http: &Client,
    kind: WhisperModelKind,
    download_url: &str,
) -> Result<reqwest::Response, String> {
    let mut last_error = String::new();

    for attempt in 1..=DOWNLOAD_MAX_ATTEMPTS {
        let mut request = http
            .get(download_url)
            .header(reqwest::header::ACCEPT, "application/octet-stream");
        if let Ok(token) = std::env::var("HF_TOKEN") {
            let token = token.trim();
            if !token.is_empty() {
                request = request.bearer_auth(token);
            }
        }

        match request.send().await {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                last_error = format_download_error(kind, download_url, Some(status), &body);
                if status == StatusCode::NOT_FOUND {
                    return Err(last_error);
                }
                if !is_transient_status(status.as_u16()) {
                    return Err(last_error);
                }
            }
            Err(error) => {
                last_error = format_download_error(
                    kind,
                    download_url,
                    None,
                    &format!("network error: {error}"),
                );
            }
        }

        if attempt < DOWNLOAD_MAX_ATTEMPTS {
            info!(
                "retrying whisper model download for {} (attempt {}/{})",
                kind.file_name(),
                attempt + 1,
                DOWNLOAD_MAX_ATTEMPTS
            );
            tokio::time::sleep(DOWNLOAD_RETRY_DELAY).await;
        }
    }

    Err(last_error)
}

fn format_download_error(
    kind: WhisperModelKind,
    url: &str,
    status: Option<StatusCode>,
    body: &str,
) -> String {
    let status_text = status
        .map(|value| value.as_u16().to_string())
        .unwrap_or_else(|| "network".to_string());
    let detail = body.trim();
    let detail = if detail.is_empty() {
        "no response body"
    } else {
        detail.lines().next().unwrap_or(detail)
    };

    if status == Some(StatusCode::NOT_FOUND) || detail.eq_ignore_ascii_case("Entry not found") {
        return format!(
            "Whisper model {} not found on Hugging Face (HTTP {status_text}). URL: {url}",
            kind.file_name()
        );
    }

    if status == Some(StatusCode::UNAUTHORIZED) || status == Some(StatusCode::FORBIDDEN) {
        return format!(
            "Hugging Face denied access to {} (HTTP {status_text}). \
             If downloads fail consistently, set HF_TOKEN and retry.",
            kind.file_name()
        );
    }

    format!(
        "failed to download {} (HTTP {status_text}): {detail}",
        kind.file_name()
    )
}

pub fn whisper_backend_label() -> &'static str {
    crate::settings::whisper_gpu_backend_label()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_urls_match_whisper_cpp_layout() {
        assert_eq!(
            WhisperModelKind::Small.download_url(),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
        );
        assert_eq!(
            WhisperModelKind::Medium.download_url(),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"
        );
        assert_eq!(
            WhisperModelKind::LargeV3Turbo.download_url(),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin"
        );
        assert_eq!(
            WhisperModelKind::LargeV3.download_url(),
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin"
        );
    }

    #[test]
    fn download_client_builds_with_redirects() {
        download_client().expect("download client");
    }

    #[tokio::test]
    #[ignore = "manual network check; run with: cargo test huggingface_models -- --ignored --nocapture"]
    async fn huggingface_models_are_downloadable() {
        let client = download_client().expect("download client");
        for kind in WhisperModelKind::all() {
            let url = kind.download_url();
            let response = client
                .get(&url)
                .header(reqwest::header::RANGE, "bytes=0-3")
                .send()
                .await
                .unwrap_or_else(|error| panic!("{} GET failed: {error}", kind.file_name()));
            let status = response.status();
            let body_prefix = response
                .bytes()
                .await
                .unwrap_or_else(|error| panic!("{} body read failed: {error}", kind.file_name()));
            assert!(
                status.is_success() || status == StatusCode::PARTIAL_CONTENT,
                "{}: HTTP {status}, body starts with {:?}",
                kind.file_name(),
                String::from_utf8_lossy(&body_prefix[..body_prefix.len().min(120)])
            );
            assert!(
                body_prefix.starts_with(&GGML_FILE_MAGIC),
                "{}: expected ggml magic, got {:?}",
                kind.file_name(),
                &body_prefix[..body_prefix.len().min(4)]
            );
        }
    }

    #[test]
    fn formats_hf_entry_not_found_error() {
        let message = format_download_error(
            WhisperModelKind::Small,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
            Some(StatusCode::NOT_FOUND),
            "Entry not found",
        );
        assert!(message.contains("ggml-small.bin"));
        assert!(message.contains("not found on Hugging Face"));
    }
}
