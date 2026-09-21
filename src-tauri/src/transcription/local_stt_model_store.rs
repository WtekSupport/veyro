use std::fs;
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use tar::Archive;
use tracing::info;

use crate::error::ConfigError;
use crate::settings::{
    quants_for_family, variant_spec, whisper_download_url_for, AppSettings, LocalSttEngine,
    LocalSttFamily, LocalSttQuant, LocalSttVariant, SherpaOnnxLayout, SttVariantSpec,
};
use crate::transcription::model_store::{download_client, DownloadProgress, resolve_models_dir};

#[derive(Clone, Serialize)]
pub struct LocalSttModelInfo {
    pub family: LocalSttFamily,
    pub quant: LocalSttQuant,
    pub variant_id: String,
    pub engine: LocalSttEngine,
    pub path: String,
    pub exists: bool,
    pub size_mb: u32,
    pub selected: bool,
}

#[derive(Clone, Serialize)]
pub struct LocalSttFamilyInfo {
    pub family: LocalSttFamily,
    pub engine: LocalSttEngine,
    pub quants: Vec<LocalSttQuant>,
    pub name_i18n_key: String,
}

#[derive(Clone, Serialize)]
pub struct LocalSttVariantInfo {
    pub family: LocalSttFamily,
    pub quant: LocalSttQuant,
    pub variant_id: String,
    pub engine: LocalSttEngine,
    pub available: bool,
    pub download_size_mb: u32,
    pub disk_bytes: Option<u64>,
    pub path: String,
    pub exists: bool,
    pub ram_mb: u32,
    pub vram_mb: Option<u32>,
    pub speed_tier: u8,
    pub accuracy_tier: u8,
    pub features_i18n_key: String,
}

pub fn quant_dir_name(quant: LocalSttQuant) -> &'static str {
    match quant {
        LocalSttQuant::Int8 => "int8",
        LocalSttQuant::Fp16 => "fp16",
        LocalSttQuant::Fp32 => "fp32",
        _ => "default",
    }
}

pub fn sherpa_bundle_path(settings: &AppSettings, variant: LocalSttVariant) -> Result<PathBuf, ConfigError> {
    let key = variant
        .family
        .sherpa_bundle_key()
        .ok_or_else(|| ConfigError::Read("not a sherpa family".to_string()))?;
    Ok(resolve_models_dir(settings)?
        .join("sherpa")
        .join(key)
        .join(quant_dir_name(variant.quant)))
}

pub fn legacy_sherpa_bundle_path(
    settings: &AppSettings,
    family: LocalSttFamily,
) -> Result<PathBuf, ConfigError> {
    let key = family
        .sherpa_bundle_key()
        .ok_or_else(|| ConfigError::Read("not a sherpa family".to_string()))?;
    Ok(resolve_models_dir(settings)?.join("sherpa").join(key))
}

pub fn whisper_model_path(settings: &AppSettings, variant: LocalSttVariant) -> Result<PathBuf, ConfigError> {
    let spec = variant_spec(variant);
    let file_name = spec
        .whisper_file_name
        .ok_or_else(|| ConfigError::Read("not a whisper variant".to_string()))?;
    Ok(resolve_models_dir(settings)?.join(file_name))
}

pub fn resolve_model_bundle(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    resolve_variant_path(settings, settings.local_stt_variant())
}

pub fn resolve_variant_path(
    settings: &AppSettings,
    variant: LocalSttVariant,
) -> Result<PathBuf, ConfigError> {
    if variant.family.is_whisper() {
        return whisper_model_path(settings, variant);
    }
    sherpa_bundle_path(settings, variant)
}

pub fn bundle_ready(path: &Path, variant: LocalSttVariant) -> bool {
    if variant.family.is_whisper() {
        if path.is_file() && required_whisper_file(variant).is_some_and(|name| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == name)
        }) {
            return true;
        }
        return path.is_file();
    }

    if required_sherpa_files(variant)
        .iter()
        .all(|relative| path.join(relative).exists())
    {
        return true;
    }

    false
}

pub fn bundle_ready_for_settings(settings: &AppSettings, variant: LocalSttVariant) -> bool {
    if let Ok(path) = resolve_variant_path(settings, variant) {
        if bundle_ready(&path, variant) {
            return true;
        }
    }
    if !variant.family.is_whisper() && variant.quant == LocalSttQuant::Int8 {
        if let Ok(legacy) = legacy_sherpa_bundle_path(settings, variant.family) {
            if bundle_ready(&legacy, variant) {
                return true;
            }
        }
    }
    false
}

pub fn effective_sherpa_bundle_dir(
    settings: &AppSettings,
    variant: LocalSttVariant,
) -> Result<PathBuf, ConfigError> {
    let path = sherpa_bundle_path(settings, variant)?;
    if bundle_ready(&path, variant) {
        return Ok(path);
    }
    if variant.quant == LocalSttQuant::Int8 {
        let legacy = legacy_sherpa_bundle_path(settings, variant.family)?;
        if bundle_ready(&legacy, variant) {
            return Ok(legacy);
        }
    }
    Ok(path)
}

fn required_whisper_file(variant: LocalSttVariant) -> Option<&'static str> {
    variant_spec(variant).whisper_file_name
}

pub fn required_sherpa_files(variant: LocalSttVariant) -> Vec<&'static str> {
    let layout = variant_spec(variant).sherpa_layout;
    match layout {
        Some(SherpaOnnxLayout::NemoInt8) => vec![
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ],
        Some(SherpaOnnxLayout::Qwen3Int8) => vec![
            "conv_frontend.onnx",
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "tokenizer",
        ],
        Some(SherpaOnnxLayout::NemoFpOnnx) => {
            if variant.quant == LocalSttQuant::Fp32 {
                vec![
                    "encoder.onnx",
                    "encoder.weights",
                    "decoder.onnx",
                    "joiner.onnx",
                    "tokens.txt",
                ]
            } else {
                vec![
                    "encoder.onnx",
                    "decoder.onnx",
                    "joiner.onnx",
                    "tokens.txt",
                ]
            }
        }
        None => vec![],
    }
}

pub fn list_families() -> Vec<LocalSttFamilyInfo> {
    LocalSttFamily::all()
        .into_iter()
        .map(|family| LocalSttFamilyInfo {
            engine: family.engine(),
            quants: quants_for_family(family)
                .iter()
                .copied()
                .filter(|quant| variant_spec(LocalSttVariant::new(family, *quant)).available)
                .collect(),
            name_i18n_key: family.name_i18n_key().to_string(),
            family,
        })
        .collect()
}

pub fn describe_variant(settings: &AppSettings, variant: LocalSttVariant) -> Result<LocalSttVariantInfo, ConfigError> {
    let variant = crate::settings::normalize_variant(variant);
    let spec = variant_spec(variant);
    let path = resolve_variant_path(settings, variant)?;
    let exists = bundle_ready_for_settings(settings, variant);
    let disk_bytes = if variant.family.is_whisper() && path.is_file() {
        path.metadata().ok().map(|meta| meta.len())
    } else if variant.family.is_whisper() {
        None
    } else if exists {
        dir_size_bytes(&path).ok()
    } else {
        None
    };

    Ok(LocalSttVariantInfo {
        family: variant.family,
        quant: variant.quant,
        variant_id: variant.as_api_id(),
        engine: spec.engine,
        available: spec.available,
        download_size_mb: spec.download_size_mb,
        disk_bytes,
        path: path.display().to_string(),
        exists,
        ram_mb: spec.ram_mb,
        vram_mb: spec.vram_mb,
        speed_tier: spec.speed_tier,
        accuracy_tier: spec.accuracy_tier,
        features_i18n_key: spec.features_i18n_key.to_string(),
    })
}

pub fn list_models(settings: &AppSettings) -> Result<Vec<LocalSttModelInfo>, ConfigError> {
    let selected = settings.local_stt_variant();
    let mut out = Vec::new();
    for family in LocalSttFamily::all() {
        for quant in quants_for_family(family) {
            let variant = LocalSttVariant::new(family, *quant);
            let spec = variant_spec(variant);
            if !spec.available {
                continue;
            }
            let path = resolve_variant_path(settings, variant)?;
            out.push(LocalSttModelInfo {
                family,
                quant: *quant,
                variant_id: variant.as_api_id(),
                engine: spec.engine,
                path: path.display().to_string(),
                exists: bundle_ready_for_settings(settings, variant),
                size_mb: spec.download_size_mb,
                selected: variant == selected,
            });
        }
    }
    Ok(out)
}

pub async fn download_model<F>(
    http: &Client,
    settings: &AppSettings,
    variant: LocalSttVariant,
    on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    let variant = crate::settings::normalize_variant(variant);
    let spec = variant_spec(variant);
    if !spec.available {
        return Err(format!("variant {:?} is not available for download", variant));
    }

    if variant.family.is_whisper() {
        return download_whisper_variant(http, settings, variant, spec, on_progress).await;
    }

    download_sherpa_variant(http, settings, variant, spec, on_progress).await
}

async fn download_whisper_variant<F>(
    http: &Client,
    settings: &AppSettings,
    variant: LocalSttVariant,
    spec: SttVariantSpec,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    let file_name = spec
        .whisper_file_name
        .ok_or_else(|| "missing whisper file name".to_string())?;
    let path = whisper_model_path(settings, variant).map_err(|e| e.to_string())?;
    if path.is_file() {
        return Ok(path);
    }

    let url = whisper_download_url_for(variant)
        .ok_or_else(|| "missing whisper download URL".to_string())?;

    info!("downloading whisper variant {variant:?} from {url}");
    let response = http
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let total = response.content_length();
    let mut downloaded = 0_u64;
    let mut stream = response.bytes_stream();
    let models_dir = resolve_models_dir(settings).map_err(|e| e.to_string())?;
    fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let mut file = fs::File::create(&path).map_err(|e| e.to_string())?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        use std::io::Write;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        on_progress(DownloadProgress::new(downloaded, total));
    }
    drop(file);

    crate::transcription::model_store::validate_whisper_file(&path, file_name)?;
    Ok(path)
}

async fn download_sherpa_variant<F>(
    http: &Client,
    settings: &AppSettings,
    variant: LocalSttVariant,
    spec: SttVariantSpec,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    let bundle_dir = sherpa_bundle_path(settings, variant).map_err(|e| e.to_string())?;
    if bundle_ready_for_settings(settings, variant) {
        return effective_sherpa_bundle_dir(settings, variant).map_err(|e| e.to_string());
    }

    let url = spec
        .sherpa_archive_url
        .ok_or_else(|| format!("no download URL for {variant:?}"))?;
    let archive_name = spec
        .sherpa_archive_name
        .ok_or_else(|| "missing archive name".to_string())?;

    let models_dir = resolve_models_dir(settings).map_err(|e| e.to_string())?;
    fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let archive_path = models_dir.join("_downloads").join(archive_name);
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    info!("downloading sherpa variant {variant:?} from {url}");
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

    extract_sherpa_archive(&archive_path, &bundle_dir, variant)?;
    let _ = fs::remove_file(&archive_path);

    if !bundle_ready(&bundle_dir, variant) {
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
    variant: LocalSttVariant,
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
        .join(format!(
            "_extract_{}_{}",
            variant.family.sherpa_bundle_key().unwrap_or("sherpa"),
            quant_dir_name(variant.quant)
        ));
    if temp_parent.exists() {
        fs::remove_dir_all(&temp_parent).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&temp_parent).map_err(|e| e.to_string())?;

    archive
        .unpack(&temp_parent)
        .map_err(|e| format!("tar extract failed: {e}"))?;

    let source_root = find_bundle_root(&temp_parent, variant)?;
    copy_dir_recursive(&source_root, bundle_dir)?;
    let _ = fs::remove_dir_all(&temp_parent);
    Ok(())
}

fn find_bundle_root(temp_parent: &Path, variant: LocalSttVariant) -> Result<PathBuf, String> {
    if bundle_ready(temp_parent, variant) {
        return Ok(temp_parent.to_path_buf());
    }
    let mut candidates = Vec::new();
    if let Ok(read) = fs::read_dir(temp_parent) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() && bundle_ready(&path, variant) {
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

fn dir_size_bytes(path: &Path) -> Result<u64, String> {
    let mut total = 0_u64;
    if path.is_file() {
        return path
            .metadata()
            .map(|m| m.len())
            .map_err(|e| e.to_string());
    }
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        if meta.is_dir() {
            total += dir_size_bytes(&entry.path())?;
        } else {
            total += meta.len();
        }
    }
    Ok(total)
}

pub fn download_client_for_stt() -> Result<Client, String> {
    download_client()
}

pub fn variant_ram_mb(variant: LocalSttVariant) -> u32 {
    variant_spec(variant).ram_mb
}
