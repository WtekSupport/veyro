use std::fs;
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use tar::Archive;
use tracing::info;

use crate::error::ConfigError;
use crate::settings::{AppSettings, LocalSttEngine, LocalSttModelKind};
use crate::transcription::model_store::{download_client, DownloadProgress, resolve_models_dir};

#[derive(Clone, Serialize)]
pub struct LocalSttModelInfo {
    pub kind: LocalSttModelKind,
    pub engine: LocalSttEngine,
    pub path: String,
    pub exists: bool,
    pub size_mb: u32,
    pub selected: bool,
}

pub fn sherpa_bundle_path(settings: &AppSettings, kind: LocalSttModelKind) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?
        .join("sherpa")
        .join(kind.sherpa_bundle_dir()))
}

pub fn resolve_model_bundle(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    let kind = settings.local_stt_model;
    if kind.is_whisper() {
        if let Some(whisper) = kind.whisper_kind() {
            return crate::transcription::model_store::model_path_for(settings, whisper);
        }
    }
    sherpa_bundle_path(settings, kind)
}

pub fn bundle_ready(path: &Path, kind: LocalSttModelKind) -> bool {
    if kind.is_whisper() {
        return path.is_file();
    }
    required_sherpa_files(kind)
        .iter()
        .all(|relative| path.join(relative).exists())
}

pub fn list_models(settings: &AppSettings) -> Result<Vec<LocalSttModelInfo>, ConfigError> {
    let selected = settings.local_stt_model;
    LocalSttModelKind::all()
        .into_iter()
        .map(|kind| {
            let path = if kind.is_whisper() {
                crate::transcription::model_store::model_path_for(
                    settings,
                    kind.whisper_kind().expect("whisper kind"),
                )?
            } else {
                sherpa_bundle_path(settings, kind)?
            };
            Ok(LocalSttModelInfo {
                engine: kind.engine(),
                kind,
                path: path.display().to_string(),
                exists: bundle_ready(&path, kind),
                size_mb: kind.approx_size_mb(),
                selected: kind == selected,
            })
        })
        .collect()
}

fn required_sherpa_files(kind: LocalSttModelKind) -> Vec<&'static str> {
    match kind {
        LocalSttModelKind::ParakeetTdt06bV3 => vec![
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ],
        LocalSttModelKind::Qwen3Asr06b | LocalSttModelKind::Qwen3Asr17b => vec![
            "conv_frontend.onnx",
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "tokenizer",
        ],
        _ => vec![],
    }
}

pub async fn download_model<F>(
    http: &Client,
    settings: &AppSettings,
    kind: LocalSttModelKind,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    if kind.is_whisper() {
        let whisper = kind.whisper_kind().expect("whisper kind");
        return crate::transcription::model_store::download_model(http, settings, whisper, on_progress).await;
    }

    let url = kind
        .download_archive_url()
        .ok_or_else(|| format!("no download URL for {kind:?}"))?;
    let bundle_dir = sherpa_bundle_path(settings, kind).map_err(|e| e.to_string())?;
    if bundle_ready(&bundle_dir, kind) {
        return Ok(bundle_dir);
    }

    let models_dir = resolve_models_dir(settings).map_err(|e| e.to_string())?;
    fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let archive_name = kind
        .download_archive_file_name()
        .ok_or_else(|| "missing archive name".to_string())?;
    let archive_path = models_dir.join("_downloads").join(archive_name);
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    info!("downloading sherpa model {kind:?} from {url}");
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let total = response.content_length();
    let mut downloaded = 0_u64;
    let mut stream = response.bytes_stream();
    let mut file = fs::File::create(&archive_path).map_err(|e| e.to_string())?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        use std::io::Write;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        on_progress(DownloadProgress::new(downloaded, total));
    }
    drop(file);

    extract_sherpa_archive(&archive_path, &bundle_dir, kind)?;
    let _ = fs::remove_file(&archive_path);

    if !bundle_ready(&bundle_dir, kind) {
        return Err(format!(
            "extracted bundle incomplete at {}",
            bundle_dir.display()
        ));
    }
    Ok(bundle_dir)
}

fn extract_sherpa_archive(
    archive_path: &Path,
    bundle_dir: &Path,
    kind: LocalSttModelKind,
) -> Result<(), String> {
    if bundle_dir.exists() {
        fs::remove_dir_all(bundle_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(bundle_dir).map_err(|e| e.to_string())?;

    let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let decoder = BzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    let temp_parent = bundle_dir
        .parent()
        .ok_or_else(|| "invalid bundle parent".to_string())?
        .join(format!("_extract_{}", kind.sherpa_bundle_dir()));
    if temp_parent.exists() {
        fs::remove_dir_all(&temp_parent).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&temp_parent).map_err(|e| e.to_string())?;

    archive
        .unpack(&temp_parent)
        .map_err(|e| format!("tar extract failed: {e}"))?;

    let source_root = find_bundle_root(&temp_parent, kind)?;
    copy_dir_recursive(&source_root, bundle_dir)?;
    let _ = fs::remove_dir_all(&temp_parent);
    Ok(())
}

fn find_bundle_root(temp_parent: &Path, kind: LocalSttModelKind) -> Result<PathBuf, String> {
    if bundle_ready(temp_parent, kind) {
        return Ok(temp_parent.to_path_buf());
    }
    let mut candidates = Vec::new();
    if let Ok(read) = fs::read_dir(temp_parent) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() && bundle_ready(&path, kind) {
                candidates.push(path);
            }
        }
    }
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| format!("could not locate sherpa files under {}", temp_parent.display()))
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_file() {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(from, to).map_err(|e| e.to_string())?;
        return Ok(());
    }
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest = to.join(entry.file_name());
        copy_dir_recursive(&entry.path(), &dest)?;
    }
    Ok(())
}

pub fn download_client_for_stt() -> Result<Client, String> {
    download_client()
}
