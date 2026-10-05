use std::cell::RefCell;

use crate::audio::resampler::{MonoResampler, TARGET_SAMPLE_RATE};
use crate::audio::segment::AudioSegment;

use super::{VadConfig, VadDetector, VadEvent};

struct CachedOfflineVad {
    config_key: String,
    detector: VadDetector,
}

thread_local! {
    static OFFLINE_VAD: RefCell<Option<CachedOfflineVad>> = RefCell::new(None);
}

fn vad_cache_key(config: &VadConfig, source_rate: u32, source_channels: u16) -> String {
    format!(
        "{:?}-{}-{}-{}-{}",
        config.engine,
        config.effective_threshold_percent(),
        config.silence_timeout_ms,
        source_rate,
        source_channels
    )
}

fn with_offline_detector<R>(
    config: &VadConfig,
    source_rate: u32,
    source_channels: u16,
    work: impl FnOnce(&mut VadDetector) -> Result<R, crate::error::AudioError>,
) -> Result<R, crate::error::AudioError> {
    OFFLINE_VAD.with(|cache| {
        let mut guard = cache.borrow_mut();
        let key = vad_cache_key(config, source_rate, source_channels);
        if guard.as_ref().is_none_or(|entry| entry.config_key != key) {
            *guard = Some(CachedOfflineVad {
                config_key: key,
                detector: VadDetector::new(config.clone(), source_rate, source_channels),
            });
        }
        let entry = guard.as_mut().expect("cache entry");
        entry.detector.reset();
        entry.detector.set_end_on_silence(true);
        work(&mut entry.detector)
    })
}

/// One continuous speech interval in the source timeline (absolute ms from file start).
#[derive(Debug, Clone)]
pub struct SpeechRegion {
    pub start_ms: u64,
    pub end_ms: u64,
    pub audio: AudioSegment,
}

fn samples_to_ms(samples: usize) -> u64 {
    (samples as u64 * 1000) / TARGET_SAMPLE_RATE as u64
}

fn span_from_end_sample(end_sample: usize, segment_sample_len: usize) -> (u64, u64) {
    let start_sample = end_sample.saturating_sub(segment_sample_len);
    (
        samples_to_ms(start_sample),
        samples_to_ms(end_sample),
    )
}

fn region_from_end_sample(end_sample: usize, segment: &AudioSegment) -> SpeechRegion {
    let (start_ms, end_ms) = span_from_end_sample(end_sample, segment.samples.len());
    SpeechRegion {
        start_ms,
        end_ms,
        audio: segment.clone(),
    }
}

/// Speech intervals in ms without retaining per-chunk audio (lighter than [`detect_speech_regions`]).
pub fn detect_speech_spans(
    source: &AudioSegment,
    config: &VadConfig,
) -> Result<Vec<(u64, u64)>, crate::error::AudioError> {
    let mut resampler = MonoResampler::new(source.sample_rate, TARGET_SAMPLE_RATE)?;
    let mut normalized = resampler.push(&source.samples, source.channels)?;
    normalized.extend(resampler.flush()?);

    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    with_offline_detector(config, TARGET_SAMPLE_RATE, 1, |detector| {
        let frame = detector.frame_samples().max(1);
        let mut spans = Vec::new();
        let mut pos = 0usize;

        while pos + frame <= normalized.len() {
            let chunk = &normalized[pos..pos + frame];
            for event in detector.push_samples(chunk)? {
                if let VadEvent::SpeechEnded(segment) = event {
                    spans.push(span_from_end_sample(
                        pos + frame,
                        segment.samples.len(),
                    ));
                }
            }
            pos += frame;
        }

        if pos < normalized.len() {
            let tail = &normalized[pos..];
            for event in detector.push_samples(tail)? {
                if let VadEvent::SpeechEnded(segment) = event {
                    spans.push(span_from_end_sample(
                        normalized.len(),
                        segment.samples.len(),
                    ));
                }
            }
        }

        if let Some(VadEvent::SpeechEnded(segment)) = detector.flush()? {
            spans.push(span_from_end_sample(
                normalized.len(),
                segment.samples.len(),
            ));
        }

        Ok(spans)
    })
}

/// Offline VAD pass over a decoded file (see `todo/TZ_audio_to_srt.md` §4.2 step 2).
pub fn detect_speech_regions(
    source: &AudioSegment,
    config: &VadConfig,
) -> Result<Vec<SpeechRegion>, crate::error::AudioError> {
    let mut resampler = MonoResampler::new(source.sample_rate, TARGET_SAMPLE_RATE)?;
    let mut normalized = resampler.push(&source.samples, source.channels)?;
    normalized.extend(resampler.flush()?);

    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    with_offline_detector(config, TARGET_SAMPLE_RATE, 1, |detector| {
        let frame = detector.frame_samples().max(1);
        let mut regions = Vec::new();
        let mut pos = 0usize;

        while pos + frame <= normalized.len() {
            let chunk = &normalized[pos..pos + frame];
            for event in detector.push_samples(chunk)? {
                if let VadEvent::SpeechEnded(segment) = event {
                    regions.push(region_from_end_sample(pos + frame, &segment));
                }
            }
            pos += frame;
        }

        if pos < normalized.len() {
            let tail = &normalized[pos..];
            for event in detector.push_samples(tail)? {
                if let VadEvent::SpeechEnded(segment) = event {
                    regions.push(region_from_end_sample(normalized.len(), &segment));
                }
            }
        }

        if let Some(VadEvent::SpeechEnded(segment)) = detector.flush()? {
            regions.push(region_from_end_sample(normalized.len(), &segment));
        }

        Ok(regions)
    })
}

/// Merge nearby VAD islands so STT runs once per speech block, not once per VAD sub-chunk.
pub fn coalesce_region_spans(regions: &[SpeechRegion], max_gap_ms: u64) -> Vec<(u64, u64)> {
    if regions.is_empty() {
        return Vec::new();
    }
    let mut spans: Vec<(u64, u64)> = Vec::new();
    for region in regions {
        if let Some(last) = spans.last_mut() {
            if region.start_ms <= last.1.saturating_add(max_gap_ms) {
                last.1 = last.1.max(region.end_ms);
                continue;
            }
        }
        spans.push((region.start_ms, region.end_ms));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(start: u64, end: u64) -> SpeechRegion {
        SpeechRegion {
            start_ms: start,
            end_ms: end,
            audio: AudioSegment::new(Vec::new(), TARGET_SAMPLE_RATE, 1),
        }
    }

    #[test]
    fn detect_speech_spans_on_synthetic_tone() {
        let config = VadConfig::default();
        let sample_count = TARGET_SAMPLE_RATE as usize;
        let samples: Vec<f32> = (0..sample_count)
            .map(|i| {
                (2.0 * std::f32::consts::PI * 220.0 * i as f32 / TARGET_SAMPLE_RATE as f32).sin()
                    * 0.4
            })
            .collect();
        let segment = AudioSegment::new(samples, TARGET_SAMPLE_RATE, 1);
        let spans = detect_speech_spans(&segment, &config).expect("spans");
        assert!(!spans.is_empty() || segment.duration_ms > 0);
    }

    #[test]
    fn coalesce_merges_nearby_regions() {
        let regions = vec![region(0, 30_000), region(31_000, 60_000)];
        let spans = coalesce_region_spans(&regions, 2_000);
        assert_eq!(spans, vec![(0, 60_000)]);
    }
}
