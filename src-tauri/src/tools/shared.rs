use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use tracing::warn;

use crate::app::context::AppContext;
use crate::audio::decode_file::{
    decode_audio_file_with_progress, DecodeProgressCallback,
};
#[cfg(feature = "local-separation")]
use crate::audio::decode_file::decode_audio_file_raw_with_progress;
#[cfg(feature = "local-separation")]
use crate::audio::resampler::{resample_preserve_channels, SEPARATION_SAMPLE_RATE};
use crate::audio::preprocess::{preprocess_segment, PreprocessOptions, PreprocessResult};
use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;

/// UI should prompt for a fixed transcription language (auto mode failed).
pub const STT_SELECT_LANGUAGE_ERROR: &str = "tools.stt.selectLanguage";

pub fn stt_failed_in_auto_mode(settings: &AppSettings, language_override: Option<&str>) -> bool {
    settings.language.is_none() && language_override.is_none()
}

pub struct ToolsTranscriptionGuard<'a> {
    ctx: &'a AppContext,
    active: bool,
}

impl<'a> ToolsTranscriptionGuard<'a> {
    pub fn try_begin(ctx: &'a AppContext) -> Result<Self, String> {
        let mut controller = ctx
            .controller
            .lock()
            .map_err(|_| "application controller lock poisoned".to_string())?;
        if controller.blocks_tools_transcription() {
            return Err("tools.voiceFiles.pttBusy".to_string());
        }
        if controller.is_tools_transcription_busy() {
            return Err("tools.voiceFiles.busy".to_string());
        }
        controller.set_tools_transcription_busy(true);
        Ok(Self {
            ctx,
            active: true,
        })
    }
}

impl Drop for ToolsTranscriptionGuard<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut controller) = self.ctx.controller.lock() {
            controller.set_tools_transcription_busy(false);
        }
    }
}

pub fn validate_tool_file_path(path: &str, error_prefix: &str) -> Result<(String, PathBuf, String), String> {
    let path_key = path.trim().to_string();
    if path_key.is_empty() {
        return Err(format!("{error_prefix}.invalidPath"));
    }
    let path_buf = PathBuf::from(&path_key);
    let file_name = path_buf
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    if file_name.is_empty() {
        return Err(format!("{error_prefix}.invalidPath"));
    }
    Ok((path_key, path_buf, file_name))
}

fn map_decode_error(error: String, prefix: &str) -> String {
    if error == "empty_audio" {
        return format!("{prefix}.emptyAudio");
    }
    if error == "unsupported_format" {
        return format!("{prefix}.unsupportedFormat");
    }
    if error.contains("unsupported codec") || error.contains("unsupported feature") {
        return format!("{prefix}.unsupportedFormat|{error}");
    }
    warn!("tool audio decode failed: {error}");
    format!("{prefix}.readFailed|{error}")
}

pub fn throttled_percent_callback(
    on_percent: Arc<dyn Fn(u8) + Send + Sync + 'static>,
) -> Arc<dyn Fn(u8) + Send + Sync + 'static> {
    let last = Arc::new(AtomicU8::new(0));
    Arc::new(move |percent: u8| {
        let prev = last.load(Ordering::Relaxed);
        if percent != 0
            && percent != 100
            && percent < prev.saturating_add(2)
            && percent < 100
        {
            return;
        }
        last.store(percent, Ordering::Relaxed);
        on_percent(percent);
    })
}

pub struct PreparedToolAudio {
    pub captured: AudioSegment,
    pub segment: AudioSegment,
    pub skipped_as_silence: bool,
}

pub fn decode_and_preprocess_for_tools(
    path: &Path,
    settings: &AppSettings,
    decode_progress: Option<DecodeProgressCallback>,
    error_prefix: &str,
) -> Result<PreparedToolAudio, String> {
    let raw = decode_audio_file_with_progress(path, decode_progress.clone())
        .map_err(|error| map_decode_error(error, error_prefix))?;

    if let Some(callback) = &decode_progress {
        callback(92);
    }

    let captured = raw.clone();
    let preprocessed: PreprocessResult = preprocess_segment(
        captured.clone(),
        PreprocessOptions {
            enabled: settings.audio_preprocess_enabled,
            noise_reduction_enabled: settings.audio_preprocess_enabled
                && settings.audio_noise_reduction_enabled,
            normalize_only: false,
        },
    );

    if let Some(callback) = &decode_progress {
        callback(100);
    }

    Ok(PreparedToolAudio {
        captured,
        segment: preprocessed.segment,
        skipped_as_silence: preprocessed.skipped_as_silence,
    })
}

/// Decode for vocal separation: preserve stereo (down/up-mix to 2ch) at model sample rate.
#[cfg(feature = "local-separation")]
pub fn decode_for_separation(
    path: &Path,
    decode_progress: Option<DecodeProgressCallback>,
    error_prefix: &str,
) -> Result<(AudioSegment, u32), String> {
    let raw = decode_audio_file_raw_with_progress(path, decode_progress.clone())
        .map_err(|error| map_decode_error(error, error_prefix))?;

    let original_sample_rate = raw.sample_rate.max(1);

    if let Some(callback) = &decode_progress {
        callback(92);
    }

    let channels = if raw.channels <= 1 { 1 } else { 2 };
    let samples = if raw.channels <= 1 {
        raw.samples.clone()
    } else if raw.channels == 2 {
        raw.samples.clone()
    } else {
        let ch = raw.channels as usize;
        raw.samples
            .chunks(ch)
            .flat_map(|frame| {
                let left = frame.first().copied().unwrap_or(0.0);
                let right = frame.get(1).copied().unwrap_or(left);
                [left, right]
            })
            .collect()
    };

    let resampled = resample_preserve_channels(
        &samples,
        raw.sample_rate,
        SEPARATION_SAMPLE_RATE,
        channels,
    )
    .map_err(|error| format!("{error_prefix}.readFailed|{error}"))?;

    let stereo = interleaved_stereo(&resampled, channels);

    if let Some(callback) = &decode_progress {
        callback(100);
    }

    Ok((
        AudioSegment::new(stereo, SEPARATION_SAMPLE_RATE, 2),
        original_sample_rate,
    ))
}

#[cfg(feature = "local-separation")]
fn interleaved_stereo(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.iter().flat_map(|sample| [*sample, *sample]).collect();
    }
    samples.to_vec()
}
