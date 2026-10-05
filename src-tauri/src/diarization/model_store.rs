//! Diarization model cache under `{models}/diarization` (polyvoice + speakrs subfolders).

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::ConfigError;
use crate::settings::AppSettings;
use crate::transcription::model_store::resolve_models_dir;

use super::engine::DiarizationQuality;

/// Approximate on-disk size of the polyvoice Balanced INT8 pair + VBx PLDA (~8.4 MB pair + extras).
pub const POLYVOICE_DOWNLOAD_MB: u32 = 12;
/// Typical CPU bundle on disk (~59 MB on HuggingFace: segmentation + wespeaker + `.onnx.data`).
pub const SPEAKRS_DOWNLOAD_MB: u32 = 60;

/// CPU bundle file list (keep in sync with speakrs 0.5 `required_files(ExecutionMode::Cpu)`).
pub const SPEAKRS_CPU_BUNDLE_FILES: &[&str] = &[
    "plda_lda.npy",
    "plda_tr.npy",
    "plda_mu.npy",
    "plda_psi.npy",
    "plda_mean1.npy",
    "plda_mean2.npy",
    "wespeaker-voxceleb-resnet34.min_num_samples.txt",
    "segmentation-3.0.onnx",
    "wespeaker-voxceleb-resnet34.onnx",
    "wespeaker-voxceleb-resnet34.onnx.data",
];

fn speakrs_file_min_bytes(file_name: &str) -> u64 {
    match file_name {
        // HF ships ~26.7 MB external weights (not hundreds of MB).
        "wespeaker-voxceleb-resnet34.onnx.data" => 24 * 1024 * 1024,
        "segmentation-3.0.onnx" => 5 * 1024 * 1024,
        "wespeaker-voxceleb-resnet34.onnx" => 24 * 1024 * 1024,
        name if name.ends_with(".npy") => 64,
        _ => 1,
    }
}

pub fn speakrs_bundle_file_valid(path: &Path, file_name: &str) -> bool {
    if !path_is_readable_model_file(path) {
        return false;
    }
    std::fs::metadata(path)
        .ok()
        .is_some_and(|meta| meta.len() >= speakrs_file_min_bytes(file_name))
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationModelStatus {
    pub path: String,
    /// Fast (polyvoice) weights present — kept for older UI (`exists`).
    pub exists: bool,
    pub size_mb: u32,
    pub available: bool,
    pub speakrs_available: bool,
    pub fast_ready: bool,
    pub accurate_ready: bool,
    pub accurate_size_mb: u32,
    pub speakrs_path: String,
    pub need_polyvoice_download: bool,
    pub need_speakrs_download: bool,
}

pub fn diarization_cache_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?.join("diarization"))
}

pub fn polyvoice_cache_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(diarization_cache_dir(settings)?.join("polyvoice"))
}

pub fn speakrs_cache_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(diarization_cache_dir(settings)?.join("speakrs"))
}

pub fn dir_has_polyvoice_bundle(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let mut has_segmenter = false;
    let mut has_embedder = false;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.contains("powerset") && name.ends_with(".onnx") {
                has_segmenter = true;
            }
            if name.contains("resnet34") && name.ends_with(".onnx") {
                has_embedder = true;
            }
        }
    }
    has_segmenter && has_embedder
}

pub fn dir_byte_size(dir: &Path) -> u64 {
    dir_tree_byte_size(dir)
}

fn dir_tree_byte_size(dir: &Path) -> u64 {
    if !dir.is_dir() {
        return 0;
    }
    let mut total = 0_u64;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            total += dir_tree_byte_size(&path);
        } else {
            total += meta.len();
        }
    }
    total
}

/// Written after a successful speakrs download; points at the HF snapshot directory.
const SPEAKRS_SNAPSHOT_MARKER: &str = ".veyro-speakrs-snapshot";

/// HF hub stores snapshots under nested `models--…/snapshots/…` inside the cache root.
pub fn speakrs_snapshot_dir(cache_root: &Path) -> Option<PathBuf> {
    read_speakrs_snapshot_marker(cache_root).or_else(|| {
        find_file_by_name(cache_root, "segmentation-3.0.onnx").and_then(|path| {
            path.parent()
                .filter(|parent| speakrs_snapshot_has_bundle(parent))
                .map(|parent| parent.to_path_buf())
        })
    })
}

fn read_speakrs_snapshot_marker(cache_root: &Path) -> Option<PathBuf> {
    let marker = cache_root.join(SPEAKRS_SNAPSHOT_MARKER);
    let text = std::fs::read_to_string(&marker).ok()?;
    let path = PathBuf::from(text.trim());
    speakrs_snapshot_has_bundle(&path).then_some(path)
}

pub fn speakrs_bundle_missing_files(snapshot: &Path) -> Vec<String> {
    SPEAKRS_CPU_BUNDLE_FILES
        .iter()
        .filter(|file_name| {
            !speakrs_bundle_file_valid(&snapshot.join(file_name), file_name)
        })
        .map(|file_name| (*file_name).to_string())
        .collect()
}

pub fn write_speakrs_snapshot_marker(cache_root: &Path, snapshot: &Path) -> Result<(), String> {
    let missing = speakrs_bundle_missing_files(snapshot);
    if !missing.is_empty() {
        return Err(format!(
            "speakrs bundle incomplete (missing or too small: {})",
            missing.join(", ")
        ));
    }
    std::fs::write(
        cache_root.join(SPEAKRS_SNAPSHOT_MARKER),
        snapshot.display().to_string(),
    )
    .map_err(|error| error.to_string())
}

fn file_name_matches(name: &std::ffi::OsStr, expected: &str) -> bool {
    #[cfg(windows)]
    {
        name.eq_ignore_ascii_case(expected)
    }
    #[cfg(not(windows))]
    {
        name == expected
    }
}

fn find_file_by_name(dir: &Path, file_name: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if file_name_matches(&entry.file_name(), file_name) && path_is_readable_model_file(&path) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_file_by_name(&path, file_name) {
                return Some(found);
            }
        }
    }
    None
}

/// HF snapshot entries may be symlinks to blobs; treat them as present when readable.
pub(crate) fn path_is_readable_model_file(path: &Path) -> bool {
    if path.is_file() {
        return true;
    }
    std::fs::OpenOptions::new().read(true).open(path).is_ok()
}

pub fn speakrs_snapshot_has_bundle(snapshot: &Path) -> bool {
    SPEAKRS_CPU_BUNDLE_FILES.iter().all(|file_name| {
        speakrs_bundle_file_valid(&snapshot.join(file_name), file_name)
    })
}

pub fn dir_has_speakrs_bundle(dir: &Path) -> bool {
    speakrs_snapshot_dir(dir).is_some()
}

#[cfg(feature = "local-diarization-speakrs")]
pub fn speakrs_models_dir(settings: &AppSettings) -> Result<PathBuf, String> {
    let cache = speakrs_cache_dir(settings).map_err(|error| error.to_string())?;
    speakrs_snapshot_dir(&cache).ok_or_else(|| "tools.audioSrt.diarizationModelRequired".to_string())
}

#[cfg(feature = "local-diarization")]
pub fn polyvoice_model_ready(settings: &AppSettings) -> bool {
    let Ok(polyvoice_dir) = polyvoice_cache_dir(settings) else {
        return false;
    };
    if dir_has_polyvoice_bundle(&polyvoice_dir) {
        return true;
    }
    // Legacy: weights lived directly under `diarization/` before subfolders.
    diarization_cache_dir(settings)
        .ok()
        .is_some_and(|dir| dir_has_polyvoice_bundle(&dir))
}

#[cfg(not(feature = "local-diarization"))]
pub fn polyvoice_model_ready(_settings: &AppSettings) -> bool {
    false
}

#[cfg(feature = "local-diarization-speakrs")]
pub fn speakrs_model_ready(settings: &AppSettings) -> bool {
    speakrs_cache_dir(settings)
        .ok()
        .is_some_and(|dir| dir_has_speakrs_bundle(&dir))
}

#[cfg(not(feature = "local-diarization-speakrs"))]
pub fn speakrs_model_ready(_settings: &AppSettings) -> bool {
    false
}

pub fn accurate_model_ready(settings: &AppSettings) -> bool {
    if cfg!(feature = "local-diarization-speakrs") {
        return speakrs_model_ready(settings);
    }
    polyvoice_model_ready(settings)
}

pub fn status(settings: &AppSettings) -> Result<DiarizationModelStatus, ConfigError> {
    let path = diarization_cache_dir(settings)?;
    let speakrs_path = speakrs_cache_dir(settings)
        .map(|dir| dir.display().to_string())
        .unwrap_or_default();
    let fast_ready = polyvoice_model_ready(settings);
    let accurate_ready = accurate_model_ready(settings);
    let speakrs_available = cfg!(feature = "local-diarization-speakrs");
    let need_polyvoice_download = cfg!(feature = "local-diarization") && !fast_ready;
    let need_speakrs_download = speakrs_available && !speakrs_model_ready(settings);
    Ok(DiarizationModelStatus {
        path: path.display().to_string(),
        exists: fast_ready,
        size_mb: POLYVOICE_DOWNLOAD_MB,
        available: cfg!(feature = "local-diarization"),
        speakrs_available,
        fast_ready,
        accurate_ready,
        accurate_size_mb: if speakrs_available {
            if speakrs_model_ready(settings) {
                speakrs_cache_dir(settings)
                    .ok()
                    .map(|dir| (dir_tree_byte_size(&dir) / (1024 * 1024)) as u32)
                    .filter(|mb| *mb > 0)
                    .unwrap_or(SPEAKRS_DOWNLOAD_MB)
            } else {
                SPEAKRS_DOWNLOAD_MB
            }
        } else {
            POLYVOICE_DOWNLOAD_MB
        },
        speakrs_path,
        need_polyvoice_download,
        need_speakrs_download,
    })
}

/// Ensure weights for the selected quality tier (downloads if missing).
#[cfg(feature = "local-diarization")]
pub fn ensure_models(settings: &AppSettings, quality: DiarizationQuality) -> Result<PathBuf, String> {
    ensure_models_with_progress(settings, quality, |_| {})
}

/// Same as [`ensure_models`], emitting download progress for the UI.
#[cfg(feature = "local-diarization")]
pub fn ensure_models_with_progress(
    settings: &AppSettings,
    quality: DiarizationQuality,
    mut on_progress: impl FnMut(crate::transcription::model_store::DownloadProgress),
) -> Result<PathBuf, String> {
    match quality {
        DiarizationQuality::Fast => ensure_polyvoice_models_with_progress(settings, &mut on_progress),
        DiarizationQuality::Accurate => {
            #[cfg(feature = "local-diarization-speakrs")]
            {
                return super::engine_speakrs::ensure_speakrs_models_with_progress(
                    settings,
                    &mut on_progress,
                );
            }
            #[cfg(not(feature = "local-diarization-speakrs"))]
            {
                ensure_polyvoice_models_with_progress(settings, &mut on_progress)
            }
        }
    }
}

#[cfg(feature = "local-diarization")]
pub fn polyvoice_models_dir(settings: &AppSettings) -> Result<PathBuf, String> {
    let polyvoice_dir = polyvoice_cache_dir(settings).map_err(|error| error.to_string())?;
    if dir_has_polyvoice_bundle(&polyvoice_dir) {
        return Ok(polyvoice_dir);
    }
    let legacy = diarization_cache_dir(settings).map_err(|error| error.to_string())?;
    if dir_has_polyvoice_bundle(&legacy) {
        return Ok(legacy);
    }
    Err("tools.audioSrt.diarizationModelRequired".to_string())
}

#[cfg(feature = "local-diarization")]
pub fn ensure_polyvoice_models_with_progress(
    settings: &AppSettings,
    on_progress: &mut impl FnMut(crate::transcription::model_store::DownloadProgress),
) -> Result<PathBuf, String> {
    use polyvoice::models::ModelRegistry;
    use polyvoice::types::Profile;

    use crate::transcription::model_store::DownloadProgress;

    let dir = polyvoice_cache_dir(settings).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let registry = ModelRegistry::with_cache_dir(&dir).map_err(|error| error.to_string())?;
    on_progress(DownloadProgress::new(0, Some(2)));
    let _models = registry
        .ensure_for_profile(Profile::Balanced)
        .map_err(|error| format!("tools.audioSrt.diarizationDownloadFailed|{error}"))?;
    on_progress(DownloadProgress::new(1, Some(2)));
    let _ = registry.ensure_vbx_plda_dir();
    on_progress(DownloadProgress::new(2, Some(2)));
    Ok(dir)
}

#[cfg(not(feature = "local-diarization"))]
pub fn ensure_polyvoice_models(_settings: &AppSettings) -> Result<PathBuf, String> {
    Err("tools.audioSrt.diarizationUnavailable".to_string())
}

#[cfg(not(feature = "local-diarization"))]
pub fn ensure_models(_settings: &AppSettings, _quality: DiarizationQuality) -> Result<PathBuf, String> {
    Err("tools.audioSrt.diarizationUnavailable".to_string())
}

#[cfg(not(feature = "local-diarization"))]
pub fn ensure_models_with_progress(
    _settings: &AppSettings,
    _quality: DiarizationQuality,
    _on_progress: impl FnMut(crate::transcription::model_store::DownloadProgress),
) -> Result<PathBuf, String> {
    Err("tools.audioSrt.diarizationUnavailable".to_string())
}

#[cfg(test)]
mod speakrs_bundle_tests {
    use super::*;
    use std::fs;

    fn write_valid_cpu_bundle(dir: &Path) {
        fs::create_dir_all(dir).expect("bundle dir");
        for name in SPEAKRS_CPU_BUNDLE_FILES {
            let path = dir.join(name);
            if *name == "wespeaker-voxceleb-resnet34.onnx.data" {
                let file = fs::File::create(&path).expect("data file");
                file.set_len(25 * 1024 * 1024).expect("data len");
            } else if name.ends_with(".onnx") {
                fs::write(&path, vec![0_u8; 600_000]).expect("onnx");
            } else {
                fs::write(&path, b"x").expect("small file");
            }
        }
    }

    fn temp_speakrs_cache() -> (PathBuf, impl Drop) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "veyro-speakrs-test-{}-{}",
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("temp cache");
        let guard = TempDirGuard(base.clone());
        (base, guard)
    }

    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn detects_flat_bundle_layout() {
        let (cache, _guard) = temp_speakrs_cache();
        let bundle = cache.join("bundle");
        write_valid_cpu_bundle(&bundle);
        write_speakrs_snapshot_marker(&cache, &bundle).expect("marker");
        assert!(dir_has_speakrs_bundle(&cache));
    }

    #[test]
    fn incomplete_bundle_without_onnx_data_is_not_ready() {
        let (cache, _guard) = temp_speakrs_cache();
        let bundle = cache.join("bundle");
        fs::create_dir_all(&bundle).expect("bundle");
        fs::write(bundle.join("segmentation-3.0.onnx"), vec![0_u8; 600_000]).expect("seg");
        fs::write(
            bundle.join("wespeaker-voxceleb-resnet34.onnx"),
            vec![0_u8; 60_000],
        )
        .expect("emb");
        assert!(!speakrs_snapshot_has_bundle(&bundle));
        assert!(!dir_has_speakrs_bundle(&cache));
    }

    #[test]
    fn marker_round_trip() {
        let (cache, _guard) = temp_speakrs_cache();
        let bundle = cache.join("bundle");
        write_valid_cpu_bundle(&bundle);
        write_speakrs_snapshot_marker(&cache, &bundle).expect("marker");
        assert_eq!(speakrs_snapshot_dir(&cache).as_deref(), Some(bundle.as_path()));
    }
}
