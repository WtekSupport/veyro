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
use crate::settings::secrets::has_api_key;
use crate::settings::{AppSettings, LlmModelKind, TextRewriteProvider};
pub use crate::transcription::model_store::download_client;

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
const DOWNLOAD_MAX_ATTEMPTS: u32 = 5;
const DOWNLOAD_RETRY_DELAY: Duration = Duration::from_secs(5);
const GGUF_FILE_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46];

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: Option<f32>,
}

#[derive(Clone, Serialize)]
pub struct LlmModelInfo {
    pub kind: LlmModelKind,
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
    Ok(crate::settings::default_data_storage_root()?.join(crate::settings::SUBDIR_LLM_MODELS))
}

pub fn resolve_models_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    crate::settings::resolve_llm_models_dir(settings)
}

pub fn model_path_for(settings: &AppSettings, kind: LlmModelKind) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?.join(kind.file_name()))
}

pub fn resolve_model_path(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    model_path_for(settings, settings.local_llm_model)
}

pub fn model_exists(path: &Path) -> bool {
    path.is_file()
}

pub fn selected_model_exists(settings: &AppSettings) -> bool {
    resolve_model_path(settings)
        .ok()
        .is_some_and(|path| model_exists(&path))
}

pub fn ai_rewrite_available_with_key(settings: &AppSettings, has_api_key: bool) -> bool {
    match settings.text_rewrite_provider {
        TextRewriteProvider::Openai => has_api_key,
        TextRewriteProvider::Local => {
            cfg!(feature = "local-llm") && selected_model_exists(settings)
        }
    }
}

pub fn ai_rewrite_available(settings: &AppSettings) -> bool {
    ai_rewrite_available_with_key(settings, has_api_key())
}

pub fn needs_local_whisper(settings: &AppSettings) -> bool {
    settings.transcription_provider == "local" && cfg!(feature = "local-whisper")
}

pub fn needs_local_llm(settings: &AppSettings) -> bool {
    settings.text_processing_mode.uses_ai()
        && matches!(settings.text_rewrite_provider, TextRewriteProvider::Local)
        && cfg!(feature = "local-llm")
        && selected_model_exists(settings)
}

pub fn list_models(settings: &AppSettings) -> Result<Vec<LlmModelInfo>, ConfigError> {
    let selected = settings.local_llm_model;
    LlmModelKind::all()
        .into_iter()
        .filter(|kind| kind.visible_for_ui_locale(settings.ui_locale))
        .map(|kind| {
            let path = model_path_for(settings, kind)?;
            Ok(LlmModelInfo {
                kind,
                path: path.display().to_string(),
                exists: model_exists(&path),
                size_mb: kind.approx_size_mb(),
                selected: kind == selected,
            })
        })
        .collect()
}

pub async fn download_model<F>(
    http: &Client,
    settings: &AppSettings,
    kind: LlmModelKind,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    let path = model_path_for(settings, kind).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let partial_path = path.with_extension("gguf.part");
    if partial_path.exists() {
        fs::remove_file(&partial_path).map_err(|error| error.to_string())?;
    }

    let download_url = kind.download_url();
    info!(
        "downloading LLM model {:?} ({}) to {}",
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

    if header_len < header.len() || header != GGUF_FILE_MAGIC {
        let _ = fs::remove_file(&partial_path);
        return Err(format!(
            "downloaded file is not a valid GGUF model. \
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
    info!("LLM model downloaded ({} bytes)", downloaded);
    Ok(path)
}

async fn fetch_model_response(
    http: &Client,
    kind: LlmModelKind,
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
                "retrying LLM model download for {} (attempt {}/{})",
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
    kind: LlmModelKind,
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
            "LLM model {} not found on Hugging Face (HTTP {status_text}). URL: {url}",
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

pub fn llm_backend_label() -> &'static str {
    crate::settings::local_llm_gpu_backend_label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AppSettings, TextProcessingMode};

    #[test]
    fn download_urls_use_huggingface_layout() {
        assert_eq!(
            LlmModelKind::Qwen3_4B.download_url(),
            "https://huggingface.co/bartowski/Qwen_Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen_Qwen3-4B-Instruct-2507-Q4_K_M.gguf"
        );
        assert_eq!(
            LlmModelKind::TLiteIt21.download_url(),
            "https://huggingface.co/t-tech/T-lite-it-2.1-GGUF/resolve/main/T-lite-it-2.1-Q4_K_M.gguf"
        );
        assert_eq!(
            LlmModelKind::Qwen25_7B.download_url(),
            "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
        );
        assert!(LlmModelKind::Gec08B
            .download_url()
            .ends_with("Qwen3.5-0.8B-GEC-KAZ-RUS-ENG.Q4_0.gguf"));
    }

    #[test]
    fn gguf_magic_is_four_ascii_bytes() {
        assert_eq!(&GGUF_FILE_MAGIC, b"GGUF");
    }

    #[test]
    fn ai_rewrite_openai_requires_api_key() {
        let settings = AppSettings {
            text_rewrite_provider: TextRewriteProvider::Openai,
            ..Default::default()
        };
        assert!(!ai_rewrite_available_with_key(&settings, false));
        assert!(ai_rewrite_available_with_key(&settings, true));
    }

    #[test]
    fn ai_rewrite_local_requires_model_on_disk() {
        let settings = AppSettings {
            text_rewrite_provider: TextRewriteProvider::Local,
            ..Default::default()
        };
        assert!(!ai_rewrite_available_with_key(&settings, true));
    }

    #[test]
    fn needs_local_llm_requires_ai_mode_and_local_provider() {
        let missing_models_dir = std::env::temp_dir().join(format!(
            "veyro-test-missing-llm-{}",
            std::process::id()
        ));
        let mut settings = AppSettings {
            text_processing_mode: TextProcessingMode::Optimization,
            text_rewrite_provider: TextRewriteProvider::Local,
            local_llm_models_dir: Some(missing_models_dir.display().to_string()),
            ..Default::default()
        };
        assert!(!needs_local_llm(&settings));

        settings.text_processing_mode = TextProcessingMode::Basic;
        assert!(!needs_local_llm(&settings));

        settings.text_processing_mode = TextProcessingMode::Optimization;
        settings.text_rewrite_provider = TextRewriteProvider::Openai;
        assert!(!needs_local_llm(&settings));
    }

    #[test]
    fn needs_local_whisper_follows_provider() {
        let mut settings = AppSettings::default();
        settings.transcription_provider = "local".to_string();
        assert_eq!(needs_local_whisper(&settings), cfg!(feature = "local-whisper"));

        settings.transcription_provider = "openai".to_string();
        assert!(!needs_local_whisper(&settings));
    }
}
