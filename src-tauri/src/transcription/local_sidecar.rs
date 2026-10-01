//! Sidecar STT backends: Canary-Qwen (NeMo Python) and Granite Speech (CrispASR).
//!
//! Integration is **VAD/PTT batch only**: each audio segment is transcribed to a final
//! string via [`TranscriptionProvider::transcribe`]. CrispASR's `--stream` live-mic mode
//! and hypothesis/committed partials are **not** wired into the dictation pipeline; when
//! the injection field is unavailable, finalized segment text accumulates in
//! [`crate::injection::focus_defer_buffer::FocusDeferBuffer`] instead.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::audio::segment::AudioSegment;
use crate::settings::{variant_spec, AppSettings, LocalSttVariant, SidecarRuntime};
use crate::transcription::local_stt_model_store::{
    bundle_ready_for_settings, sidecar_bundle_path,
};
use crate::transcription::models::{TranscriptionOptions, TranscriptionResult};
use crate::transcription::provider::{TranscriptionError, TranscriptionProvider};

const CANARY_WORKER_SCRIPT: &str = include_str!("../../resources/sidecar/canary_worker.py");

#[derive(Debug, Serialize)]
struct SidecarRequest {
    cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    wav_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_rate: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SidecarResponse {
    ok: bool,
    #[serde(default)]
    text: String,
    #[serde(default)]
    error: String,
}

pub struct LocalSidecarProvider {
    settings: AppSettings,
    bundle_dir: PathBuf,
    cancel: CancellationToken,
}

impl LocalSidecarProvider {
    pub fn new(settings: AppSettings, bundle_dir: PathBuf, cancel: CancellationToken) -> Self {
        Self {
            settings,
            bundle_dir,
            cancel,
        }
    }

    pub fn new_from_settings(
        settings: &AppSettings,
        cancel: CancellationToken,
    ) -> Result<Self, TranscriptionError> {
        let variant = settings.local_stt_variant();
        let bundle_dir = sidecar_bundle_path(settings, variant)
            .map_err(|e| TranscriptionError::ModelNotFound(e.to_string()))?;
        Ok(Self::new(settings.clone(), bundle_dir, cancel))
    }

    fn variant(&self) -> LocalSttVariant {
        self.settings.local_stt_variant()
    }

    fn runtime(&self) -> Result<SidecarRuntime, TranscriptionError> {
        variant_spec(self.variant())
            .sidecar_runtime
            .ok_or_else(|| TranscriptionError::InferenceFailed("not a sidecar variant".into()))
    }

    fn ensure_ready(&self) -> Result<(), TranscriptionError> {
        if !bundle_ready_for_settings(&self.settings, self.variant()) {
            return Err(TranscriptionError::ModelNotFound(
                self.bundle_dir.display().to_string(),
            ));
        }
        Ok(())
    }

    fn write_temp_wav(audio: &AudioSegment) -> Result<PathBuf, TranscriptionError> {
        let dir = std::env::temp_dir().join("veyro-sidecar-stt");
        std::fs::create_dir_all(&dir).map_err(|e| TranscriptionError::InferenceFailed(e.to_string()))?;
        let path = dir.join(format!("seg-{}.wav", uuid_like()));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: audio.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec)
            .map_err(|e| TranscriptionError::InferenceFailed(e.to_string()))?;
        for sample in &audio.samples {
            let s = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(s)
                .map_err(|e| TranscriptionError::InferenceFailed(e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| TranscriptionError::InferenceFailed(e.to_string()))?;
        Ok(path)
    }

    fn transcribe_granite_batch(&self, wav: &Path) -> Result<String, TranscriptionError> {
        let model = granite_model_path(&self.bundle_dir, self.variant())?;
        let bin = resolve_crispasr_binary(&self.bundle_dir).ok_or_else(|| {
            TranscriptionError::InferenceFailed(
                "CrispASR binary not found. Place `crispasr` (or crispasr.exe) in the Granite sidecar folder."
                    .into(),
            )
        })?;

        // Batch mode for VAD segments. CrispASR also supports `--stream` for live mic;
        // that path is not integrated (no hypothesis/committed events in Veyro). Offline
        // VAD chunks use `-f`; field-unavailable text goes to FocusDeferBuffer after finalize.
        let output = Command::new(&bin)
            .arg("--backend")
            .arg("granite")
            .arg("-m")
            .arg(&model)
            .arg("-f")
            .arg(wav)
            .current_dir(&self.bundle_dir)
            .output()
            .map_err(|e| {
                TranscriptionError::InferenceFailed(format!("failed to run CrispASR: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TranscriptionError::InferenceFailed(format!(
                "CrispASR failed: {stderr}"
            )));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(stdout)
    }

    fn transcribe_canary_batch(&self, wav: &Path) -> Result<String, TranscriptionError> {
        let worker = self.bundle_dir.join("canary_worker.py");
        if !worker.is_file() {
            ensure_canary_worker(&self.bundle_dir)
                .map_err(TranscriptionError::InferenceFailed)?;
        }
        let python = resolve_python().ok_or_else(|| {
            TranscriptionError::InferenceFailed(
                "Python 3 not found for Canary-Qwen sidecar (install python3 on PATH)".into(),
            )
        })?;

        let mut child = Command::new(&python)
            .arg(&worker)
            .arg("--bundle")
            .arg(&self.bundle_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&self.bundle_dir)
            .spawn()
            .map_err(|e| {
                TranscriptionError::InferenceFailed(format!("failed to start Canary worker: {e}"))
            })?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| TranscriptionError::InferenceFailed("canary stdin missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TranscriptionError::InferenceFailed("canary stdout missing".into()))?;

        let req = SidecarRequest {
            cmd: "transcribe".into(),
            wav_path: Some(wav.display().to_string()),
            sample_rate: Some(16_000),
        };
        let line = serde_json::to_string(&req)
            .map_err(|e| TranscriptionError::InferenceFailed(e.to_string()))?;
        writeln!(stdin, "{line}")
            .map_err(|e| TranscriptionError::InferenceFailed(e.to_string()))?;
        let _ = writeln!(stdin, "{}", serde_json::json!({"cmd":"quit"}));
        drop(stdin);

        let mut reader = BufReader::new(stdout);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| TranscriptionError::InferenceFailed(e.to_string()))?;
        let _ = child.wait();

        let resp: SidecarResponse = serde_json::from_str(response_line.trim()).map_err(|e| {
            TranscriptionError::InferenceFailed(format!(
                "invalid Canary response ({e}): {response_line}"
            ))
        })?;
        if !resp.ok {
            return Err(TranscriptionError::InferenceFailed(if resp.error.is_empty() {
                "Canary transcription failed".into()
            } else {
                resp.error
            }));
        }
        Ok(resp.text)
    }
}

#[async_trait]
impl TranscriptionProvider for LocalSidecarProvider {
    async fn transcribe(
        &self,
        audio: AudioSegment,
        _options: TranscriptionOptions,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        if self.cancel.is_cancelled() {
            return Err(TranscriptionError::Cancelled);
        }
        self.ensure_ready()?;
        let runtime = self.runtime()?;
        let wav = Self::write_temp_wav(&audio)?;
        let text = match runtime {
            SidecarRuntime::GraniteCrispAsr => self.transcribe_granite_batch(&wav),
            SidecarRuntime::CanaryNemo => self.transcribe_canary_batch(&wav),
        };
        let _ = std::fs::remove_file(&wav);
        let text = text?;
        Ok(TranscriptionResult {
            text,
            confidence: None,
            whisper_segments: None,
            timed_segments: None,
            audio_peak: None,
            audio_rms: None,
            detected_language: None,
        })
    }
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

fn granite_model_path(
    bundle_dir: &Path,
    variant: LocalSttVariant,
) -> Result<PathBuf, TranscriptionError> {
    let file = variant_spec(variant)
        .sidecar_model_file
        .ok_or_else(|| TranscriptionError::ModelNotFound("granite model file missing".into()))?;
    let path = bundle_dir.join(file);
    if !path.is_file() {
        return Err(TranscriptionError::ModelNotFound(path.display().to_string()));
    }
    Ok(path)
}

pub fn resolve_crispasr_binary(bundle_dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    let names = ["crispasr.exe", "crispasr"];
    #[cfg(not(windows))]
    let names = ["crispasr"];

    for name in names {
        let candidate = bundle_dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // PATH fallback
    which_bin("crispasr").or_else(|| which_bin("crispasr.exe"))
}

fn which_bin(name: &str) -> Option<PathBuf> {
    let Ok(path_env) = std::env::var("PATH") else {
        return None;
    };
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn resolve_python() -> Option<PathBuf> {
    for name in ["python3", "python", "py"] {
        if let Some(path) = which_bin(name) {
            return Some(path);
        }
    }
    None
}

pub fn ensure_canary_worker(bundle_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(bundle_dir).map_err(|e| e.to_string())?;
    let dest = bundle_dir.join("canary_worker.py");
    std::fs::write(&dest, CANARY_WORKER_SCRIPT).map_err(|e| e.to_string())?;
    info!("wrote Canary worker to {}", dest.display());
    Ok(())
}

/// Try to fetch a CrispASR binary into the bundle. Soft-fail if unavailable.
pub async fn ensure_crispasr_binary(bundle_dir: &Path, http: &Client) -> Result<(), String> {
    if resolve_crispasr_binary(bundle_dir).is_some() {
        return Ok(());
    }
    std::fs::create_dir_all(bundle_dir).map_err(|e| e.to_string())?;

    // Official prebuilt assets are not always published; leave a README with build hints.
    let readme = bundle_dir.join("CRISPASR_README.txt");
    if !readme.is_file() {
        std::fs::write(
            &readme,
            "Granite Speech uses CrispASR (https://github.com/CrispStrobe/CrispASR).\n\
             Build the `crispasr` binary and place it in this folder as crispasr.exe (Windows)\n\
             or crispasr (macOS/Linux). The GGUF model is downloaded automatically.\n\
             Streaming flag `--stream` is supported by CrispASR for live mic; Veyro uses VAD\n\
             segments with batch `-f` by default.\n",
        )
        .map_err(|e| e.to_string())?;
    }

    // Optional: probe GitHub latest release for a matching asset (best-effort).
    let release_url = "https://api.github.com/repos/CrispStrobe/CrispASR/releases/latest";
    let response = http
        .get(release_url)
        .header("User-Agent", "veyro-stt")
        .send()
        .await;
    let Ok(response) = response else {
        return Ok(());
    };
    if !response.status().is_success() {
        return Ok(());
    }
    let Ok(body) = response.json::<serde_json::Value>().await else {
        return Ok(());
    };
    let Some(assets) = body.get("assets").and_then(|a| a.as_array()) else {
        return Ok(());
    };

    let prefer: &[&str] = if cfg!(windows) {
        &["windows", "win", "exe"]
    } else if cfg!(target_os = "macos") {
        &["macos", "darwin", "osx"]
    } else {
        &["linux", "ubuntu", "gnu"]
    };

    for asset in assets {
        let name = asset
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_lowercase();
        if !prefer.iter().any(|p| name.contains(p)) {
            continue;
        }
        if !(name.contains("crispasr") || name.contains("asr")) {
            continue;
        }
        let Some(url) = asset.get("browser_download_url").and_then(|u| u.as_str()) else {
            continue;
        };
        let dest_name = if cfg!(windows) {
            "crispasr.exe"
        } else {
            "crispasr"
        };
        let dest = bundle_dir.join(dest_name);
        info!("downloading CrispASR asset {name} → {}", dest.display());
        let Ok(bin_resp) = http.get(url).send().await else {
            continue;
        };
        let Ok(bytes) = bin_resp.bytes().await else {
            continue;
        };
        if std::fs::write(&dest, &bytes).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
            }
            return Ok(());
        }
    }

    warn!("no CrispASR release asset matched this platform; place binary manually");
    Ok(())
}

/// Whether CrispASR native streaming (`--stream`) binary is present.
///
/// Presence does **not** mean Veyro uses streaming STT — runtime still runs batch `-f`
/// per VAD segment. Kept for install/diagnostics only.
pub fn granite_native_stream_available(bundle_dir: &Path) -> bool {
    resolve_crispasr_binary(bundle_dir).is_some()
}
