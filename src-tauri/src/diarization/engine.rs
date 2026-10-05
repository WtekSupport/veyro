//! Offline diarization. Fast is polyvoice INT8 + VBx. Accurate is speakrs (pyannote FP32).

use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;

use super::assign::SpeakerInterval;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeakerCountMode {
    #[default]
    Auto,
    Exact,
    Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiarizationQuality {
    #[default]
    Fast,
    Accurate,
}

#[derive(Debug, Clone)]
pub struct DiarizeOptions {
    pub quality: DiarizationQuality,
    pub count_mode: SpeakerCountMode,
    pub exact_count: u8,
    pub min_count: u8,
    pub max_count: u8,
    pub min_speech_secs: f32,
    /// Accurate/speakrs: higher keeps more speakers (lower VBx `Fb` and AHC distance).
    /// Fast/polyvoice VBx ignores this value.
    pub sensitivity: f32,
}

impl Default for DiarizeOptions {
    fn default() -> Self {
        Self {
            quality: DiarizationQuality::Fast,
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
    #[cfg(feature = "local-diarization-speakrs")]
    if options.quality == DiarizationQuality::Accurate {
        return super::engine_speakrs::diarize_segment_speakrs(settings, segment, options);
    }

    diarize_segment_polyvoice(settings, segment, options)
}

#[cfg(feature = "local-diarization")]
fn diarize_segment_polyvoice(
    settings: &AppSettings,
    segment: &AudioSegment,
    options: &DiarizeOptions,
) -> Result<Vec<SpeakerInterval>, String> {
    use polyvoice::models::ModelRegistry;
    use polyvoice::pipeline_v2::ClustererKind;
    use polyvoice::types::Profile;
    use polyvoice::{Pipeline, PipelineConfig};

    use super::model_store;

    if segment.samples.is_empty() || segment.sample_rate == 0 {
        return Ok(Vec::new());
    }

    let cache = model_store::polyvoice_models_dir(settings)?;
    let registry = ModelRegistry::with_cache_dir(&cache).map_err(|error| error.to_string())?;

    let mut config = PipelineConfig::default();
    config.min_speech_secs = options.min_speech_secs.clamp(0.05, 2.0);
    config.max_speakers = match options.count_mode {
        SpeakerCountMode::Auto => 20,
        SpeakerCountMode::Exact => options.exact_count.clamp(1, 20),
        SpeakerCountMode::Range => options.max_count.clamp(options.min_count.max(1), 20),
    };

    // Fast and Balanced resolve the same INT8 weights. Accurate uses speakrs.
    config.clusterer = ClustererKind::Vbx;
    config.profile = Profile::Fast;

    let pipeline = Pipeline::builder()
        .config(config)
        .with_models_from(registry)
        .build()
        .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;

    let samples_16k = crate::audio::resampler::resample_mono_to_16k(
        &segment.samples,
        segment.channels,
        segment.sample_rate,
    )
    .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))?;

    let result = run_polyvoice_pipeline(&pipeline, &samples_16k)?;
    Ok(filter_by_min_count(result, options))
}

#[cfg(feature = "local-diarization")]
fn run_polyvoice_pipeline(
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
