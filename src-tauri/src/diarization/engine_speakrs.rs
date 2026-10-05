//! Accurate diarization via speakrs (pyannote segmentation-3.0 + WeSpeaker FP32).

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;
use crate::transcription::model_store::DownloadProgress;

use super::assign::SpeakerInterval;
use super::engine::DiarizeOptions;
use super::model_store::{self, speakrs_cache_dir};

/// Pyannote-class diarization. VBx `Fb` is what actually drops extra speakers;
/// the AHC threshold only seeds that step.
pub fn diarize_segment_speakrs(
    settings: &AppSettings,
    segment: &AudioSegment,
    options: &DiarizeOptions,
) -> Result<Vec<SpeakerInterval>, String> {
    if segment.samples.is_empty() || segment.sample_rate == 0 {
        return Ok(Vec::new());
    }

    let models_dir = super::model_store::speakrs_models_dir(settings)?;
    let samples = crate::audio::resampler::resample_mono_to_16k(
        &segment.samples,
        segment.channels,
        segment.sample_rate,
    )
    .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;
    if samples.is_empty() {
        return Ok(Vec::new());
    }

    let mode = speakrs_execution_mode();
    let config = speakrs_pipeline_config(options);
    let mut pipeline = speakrs::OwnedDiarizationPipeline::from_dir(&models_dir, mode)
        .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;
    let result = pipeline
        .run_with_config(&samples, "audio", &config)
        .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;

    let min_ms = (options.min_speech_secs.clamp(0.05, 2.0) * 1000.0).round() as u64;
    let intervals = map_speakrs_segments(result.segments, min_ms);
    let duration_secs = samples.len() as f64 / 16_000.0;
    if duration_secs >= 8.0 && intervals.is_empty() {
        return Err("tools.audioSrt.diarizationNoSegments".to_string());
    }

    let mut speaker_ids: Vec<u32> = intervals.iter().map(|interval| interval.speaker_id).collect();
    speaker_ids.sort_unstable();
    speaker_ids.dedup();
    tracing::info!(
        speakers = speaker_ids.len(),
        ahc_threshold = config.ahc.threshold,
        vbx_fb = config.vbx.fb,
        "speakrs diarization finished"
    );
    Ok(intervals)
}

/// Shared slider is 0.15..0.9. Speakrs only keeps extra speakers on the upper
/// part of that scale, so the whole slider is stretched onto that band.
const SPEAKRS_SLIDER_MIN: f32 = 0.15;
const SPEAKRS_SLIDER_MAX: f32 = 0.9;
const SPEAKRS_BAND_MIN: f32 = 0.68;
const SPEAKRS_BAND_MAX: f32 = 0.9;

fn speakrs_effective_sensitivity(ui: f32) -> f32 {
    let span = SPEAKRS_SLIDER_MAX - SPEAKRS_SLIDER_MIN;
    let t = ((ui.clamp(SPEAKRS_SLIDER_MIN, SPEAKRS_SLIDER_MAX) - SPEAKRS_SLIDER_MIN) / span)
        .clamp(0.0, 1.0);
    SPEAKRS_BAND_MIN + t * (SPEAKRS_BAND_MAX - SPEAKRS_BAND_MIN)
}

fn speakrs_pipeline_config(options: &DiarizeOptions) -> speakrs::PipelineConfig {
    let mut config = speakrs::PipelineConfig::for_mode(speakrs_execution_mode());
    let delta = f64::from(speakrs_effective_sensitivity(options.sensitivity) - 0.45);
    config.ahc.threshold = (0.6 - delta * 0.8).clamp(0.12, 0.85) as f32;
    config.vbx.fb = (0.8 - delta * 1.6).clamp(0.08, 1.6);
    config
}

fn map_speakrs_segments(
    segments: Vec<speakrs::Segment>,
    min_ms: u64,
) -> Vec<SpeakerInterval> {
    let mut speaker_ids: HashMap<String, u32> = HashMap::new();
    let mut next_id = 0u32;
    let mut intervals = Vec::new();
    for segment in segments {
        if segment.end <= segment.start {
            continue;
        }
        let start_ms = (segment.start.max(0.0) * 1000.0).round() as u64;
        let end_ms = (segment.end.max(0.0) * 1000.0).round() as u64;
        if end_ms.saturating_sub(start_ms) < min_ms {
            continue;
        }
        let speaker_id = *speaker_ids.entry(segment.speaker).or_insert_with(|| {
            let id = next_id;
            next_id += 1;
            id
        });
        intervals.push(SpeakerInterval {
            speaker_id,
            start_ms,
            end_ms,
        });
    }
    intervals.sort_by_key(|interval| (interval.start_ms, interval.speaker_id));
    intervals
}

const SPEAKRS_HF_REPO: &str = "avencera/speakrs-models";
/// Flat bundle dir under the speakrs cache (marker points here after download).
const SPEAKRS_BUNDLE_DIR: &str = "bundle";


fn speakrs_execution_mode() -> speakrs::inference::ExecutionMode {
    #[cfg(all(target_os = "macos", feature = "speakrs-coreml"))]
    {
        return speakrs::inference::ExecutionMode::CoreMl;
    }
    speakrs::inference::ExecutionMode::Cpu
}

fn speakrs_hf_resolve_url(file_name: &str) -> String {
    format!(
        "https://huggingface.co/{SPEAKRS_HF_REPO}/resolve/main/{file_name}"
    )
}

fn speakrs_bundle_dir(cache_root: &Path) -> PathBuf {
    cache_root.join(SPEAKRS_BUNDLE_DIR)
}

/// ureq agent that trusts the OS certificate store (Windows/macOS/Linux), not webpki-only roots.
fn speakrs_download_agent() -> Result<ureq::Agent, String> {
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

    let config = ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::Rustls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build();
    Ok(ureq::Agent::new_with_config(config))
}

fn apply_hf_token(request: ureq::RequestBuilder<ureq::typestate::WithoutBody>) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
    if let Ok(token) = std::env::var("HF_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            return request.header("Authorization", format!("Bearer {token}"));
        }
    }
    request
}

fn download_speakrs_file(agent: &ureq::Agent, file_name: &str, dest: &Path) -> Result<(), String> {
    if model_store::speakrs_bundle_file_valid(dest, file_name) {
        return Ok(());
    }
    if dest.exists() {
        let _ = std::fs::remove_file(dest);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let url = speakrs_hf_resolve_url(file_name);
    let partial = dest.with_extension("part");
    let mut response = apply_hf_token(agent.get(&url))
        .call()
        .map_err(|error| format!("tools.audioSrt.diarizationDownloadFailed|{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "tools.audioSrt.diarizationDownloadFailed|HTTP {} for {file_name}",
            response.status()
        ));
    }
    let mut reader = response.body_mut().with_config().reader();
    let mut file = std::fs::File::create(&partial).map_err(|error| error.to_string())?;
    std::io::copy(&mut reader, &mut file).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    if dest.is_file() {
        std::fs::remove_file(dest).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&partial, dest).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn ensure_speakrs_models_with_progress(
    settings: &AppSettings,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<PathBuf, String> {
    use speakrs::pipeline::OwnedDiarizationPipeline;

    let cache = speakrs_cache_dir(settings).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
    std::env::set_var("SPEAKRS_MODELS_DIR", cache.display().to_string());

    let models_dir = speakrs_bundle_dir(&cache);
    std::fs::create_dir_all(&models_dir).map_err(|error| error.to_string())?;

    let agent = speakrs_download_agent()?;
    let mode = speakrs_execution_mode();
    let files = model_store::SPEAKRS_CPU_BUNDLE_FILES;
    let total_files = files.len() as u64;
    on_progress(DownloadProgress::new(0, Some(total_files)));

    for (index, file) in files.iter().enumerate() {
        let dest = models_dir.join(file);
        download_speakrs_file(&agent, file, &dest)?;
        let done = (index + 1) as u64;
        on_progress(DownloadProgress::new(done, Some(total_files)));
    }

    let _pipeline = OwnedDiarizationPipeline::from_dir(&models_dir, mode)
        .map_err(|error| format!("tools.audioSrt.diarizationDownloadFailed|{error}"))?;

    model_store::write_speakrs_snapshot_marker(&cache, &models_dir).map_err(|error| {
        format!("tools.audioSrt.diarizationDownloadFailed|{error}")
    })?;

    if !model_store::speakrs_model_ready(settings) {
        return Err(
            "tools.audioSrt.diarizationDownloadFailed|speakrs bundle incomplete after download"
                .to_string(),
        );
    }

    Ok(models_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_at(sensitivity: f32) -> speakrs::PipelineConfig {
        let mut options = DiarizeOptions::default();
        options.sensitivity = sensitivity;
        speakrs_pipeline_config(&options)
    }

    #[test]
    fn slider_midpoint_lands_in_the_band_where_speakrs_separates() {
        let mild = config_at(0.15);
        let mid = config_at(0.45);
        let strict = config_at(0.9);

        assert!(mid.ahc.threshold < 0.4);
        assert!(mid.vbx.fb < 0.35);
        assert!(mild.ahc.threshold > mid.ahc.threshold);
        assert!(mid.ahc.threshold > strict.ahc.threshold);
        assert!(mild.vbx.fb > mid.vbx.fb);
        assert!(mid.vbx.fb > strict.vbx.fb);
        assert!((strict.vbx.fb - 0.08).abs() < 0.02);
    }
}
