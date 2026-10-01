//! polyvoice offline diarization over an in-memory 16 kHz mono buffer.

use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;

use super::assign::SpeakerInterval;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeakerCountMode {
    Auto,
    Exact,
    Range,
}

#[derive(Debug, Clone)]
pub struct DiarizeOptions {
    pub count_mode: SpeakerCountMode,
    pub exact_count: u8,
    pub min_count: u8,
    pub max_count: u8,
    pub min_speech_secs: f32,
    /// Maps to AHC cosine threshold when not using default VBx; higher = stricter.
    pub sensitivity: f32,
}

impl Default for DiarizeOptions {
    fn default() -> Self {
        Self {
            count_mode: SpeakerCountMode::Auto,
            exact_count: 2,
            min_count: 1,
            max_count: 8,
            min_speech_secs: 0.25,
            sensitivity: 0.45,
        }
    }
}

#[cfg(feature = "local-diarization")]
pub fn diarize_segment(
    settings: &AppSettings,
    segment: &AudioSegment,
    options: &DiarizeOptions,
) -> Result<Vec<SpeakerInterval>, String> {
    use crate::audio::resampler::to_mono;
    use polyvoice::models::ModelRegistry;
    use polyvoice::types::Profile;
    use polyvoice::{Pipeline, PipelineConfig};

    use super::model_store;

    if segment.samples.is_empty() || segment.sample_rate == 0 {
        return Ok(Vec::new());
    }

    let cache = model_store::ensure_models(settings)?;
    let registry = ModelRegistry::with_cache_dir(&cache).map_err(|error| error.to_string())?;

    let mut config = PipelineConfig::default();
    config.profile = Profile::Balanced;
    config.min_speech_secs = options.min_speech_secs.clamp(0.05, 2.0);
    config.max_speakers = match options.count_mode {
        SpeakerCountMode::Auto => 20,
        SpeakerCountMode::Exact => options.exact_count.clamp(1, 20),
        SpeakerCountMode::Range => options.max_count.clamp(options.min_count.max(1), 20),
    };
    // Sensitivity: when user tunes it away from default, prefer AHC with that threshold.
    if (options.sensitivity - 0.45).abs() > 0.02 {
        config.clusterer = polyvoice::pipeline_v2::ClustererKind::Ahc {
            threshold: options.sensitivity.clamp(0.15, 0.9),
        };
    }

    let pipeline = Pipeline::builder()
        .config(config)
        .with_models_from(registry)
        .build()
        .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;

    let mono = if segment.channels <= 1 {
        segment.samples.clone()
    } else {
        to_mono(&segment.samples, segment.channels)
    };

    if segment.sample_rate != 16_000 {
        let mut resampler = crate::audio::resampler::MonoResampler::new(segment.sample_rate, 16_000)
            .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;
        let resampled = resampler
            .push(&mono, 1)
            .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;
        let result = run_pipeline(&pipeline, &resampled)?;
        return Ok(filter_by_min_count(result, options));
    }

    let result = run_pipeline(&pipeline, &mono)?;
    Ok(filter_by_min_count(result, options))
}

#[cfg(feature = "local-diarization")]
fn run_pipeline(
    pipeline: &polyvoice::Pipeline,
    samples: &[f32],
) -> Result<Vec<SpeakerInterval>, String> {
    use polyvoice::types::SampleRate;

    let sr = SampleRate::new(16_000)
        .ok_or_else(|| "tools.audioSrt.diarizationFailed|bad sample rate".to_string())?;
    let result = pipeline
        .run(samples, sr)
        .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;

    let mut intervals: Vec<SpeakerInterval> = result
        .turns
        .iter()
        .map(|turn| SpeakerInterval {
            speaker_id: turn.speaker.0,
            start_ms: (turn.time.start.max(0.0) * 1000.0).round() as u64,
            end_ms: (turn.time.end.max(0.0) * 1000.0).round() as u64,
        })
        .filter(|interval| interval.end_ms > interval.start_ms)
        .collect();
    intervals.sort_by_key(|interval| (interval.start_ms, interval.speaker_id));
    Ok(intervals)
}

#[cfg(feature = "local-diarization")]
fn filter_by_min_count(
    intervals: Vec<SpeakerInterval>,
    options: &DiarizeOptions,
) -> Vec<SpeakerInterval> {
    if options.count_mode != SpeakerCountMode::Range {
        return intervals;
    }
    let min = options.min_count.max(1) as usize;
    let mut ids: Vec<u32> = intervals.iter().map(|i| i.speaker_id).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() >= min {
        return intervals;
    }
    // Too few speakers for requested range — keep raw result (auto fell short).
    intervals
}

#[cfg(not(feature = "local-diarization"))]
pub fn diarize_segment(
    _settings: &AppSettings,
    _segment: &AudioSegment,
    _options: &DiarizeOptions,
) -> Result<Vec<SpeakerInterval>, String> {
    Err("tools.audioSrt.diarizationUnavailable".to_string())
}
