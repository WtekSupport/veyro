use pitch_detection::detector::yin::YINDetector;
use pitch_detection::detector::PitchDetector;

use crate::audio::resampler::{to_mono, MonoResampler, TARGET_SAMPLE_RATE};
use crate::audio::segment::AudioSegment;

use super::config::SpeechAnalysisConfig;
use super::qc::SpeechSpan;
use super::types::{F0ContourPoint, PitchExpressiveness, ProsodyReport};

const YIN_BUFFER_SAMPLES: usize = 2048;
const MAX_FRAME_JUMP_SEMITONES: f32 = 6.0;
const WIDE_PITCH_MIN_HZ: f32 = 70.0;
const WIDE_PITCH_MAX_HZ: f32 = 500.0;

pub fn analyze_prosody(
    source: &AudioSegment,
    speech_spans: &[SpeechSpan],
    config: &SpeechAnalysisConfig,
) -> ProsodyReport {
    let mono = normalize_mono(source);
    let (raw_wide, candidates) =
        extract_f0_series(&mono, speech_spans, config, WIDE_PITCH_MIN_HZ, WIDE_PITCH_MAX_HZ);
    let coarse = clean_f0_series(&raw_wide);
    let coarse_hz: Vec<f32> = coarse.iter().map(|(_, h)| *h).collect();
    let coarse_median = median_hz(&coarse_hz);

    let (min_hz, max_hz) = refined_pitch_bounds(coarse_median, config);
    let (raw_refined, _) = extract_f0_series(&mono, speech_spans, config, min_hz, max_hz);
    let merged = if raw_refined.is_empty() { raw_wide } else { raw_refined };
    let cleaned_series = clean_f0_series(&merged);
    let hz_values: Vec<f32> = cleaned_series.iter().map(|(_, h)| *h).collect();
    let f0_contour = downsample_contour(&cleaned_series, 220);

    let voiced_fraction = if candidates == 0 {
        0.0
    } else {
        (hz_values.len() as f32 / candidates as f32).min(1.0)
    };

    let f0_median_hz = median_hz(&hz_values);
    let f0_range_semitones = percentile_range_semitones(&hz_values, f0_median_hz, 0.05, 0.95);
    let f0_std_semitones =
        percentile_range_semitones(&hz_values, f0_median_hz, 0.10, 0.90).unwrap_or(0.0);

    let expressiveness = classify_expressiveness(f0_std_semitones, config);
    let tracker_unreliable = f0_std_semitones > 8.5
        || f0_range_semitones.is_some_and(|r| r > 18.0)
        || voiced_fraction < 0.12;
    let low_confidence = hz_values.len() < 8 || tracker_unreliable;

    ProsodyReport {
        f0_median_hz,
        f0_range_semitones,
        f0_std_semitones,
        voiced_fraction,
        expressiveness,
        low_confidence,
        f0_contour,
        voice_quality: None,
    }
}

fn refined_pitch_bounds(median: Option<f32>, config: &SpeechAnalysisConfig) -> (f32, f32) {
    let Some(m) = median.filter(|v| *v > 0.0 && v.is_finite()) else {
        return (config.pitch_min_hz, config.pitch_max_hz);
    };
    let lo = (m * 0.72).max(config.pitch_min_hz).max(75.0);
    let hi = (m * 1.65).min(config.pitch_max_hz).min(380.0);
    if hi <= lo + 20.0 {
        (config.pitch_min_hz, config.pitch_max_hz)
    } else {
        (lo, hi)
    }
}

fn normalize_mono(source: &AudioSegment) -> Vec<f32> {
    if source.sample_rate == TARGET_SAMPLE_RATE {
        return to_mono(&source.samples, source.channels);
    }
    let mut resampler =
        MonoResampler::new(source.sample_rate, TARGET_SAMPLE_RATE).unwrap_or_else(|_| {
            MonoResampler::new(TARGET_SAMPLE_RATE, TARGET_SAMPLE_RATE).expect("identity resampler")
        });
    let mono = to_mono(&source.samples, source.channels);
    resampler
        .push(&mono, 1)
        .and_then(|mut out| {
            out.extend(resampler.flush()?);
            Ok(out)
        })
        .unwrap_or(mono)
}

fn ms_to_sample(ms: u64) -> usize {
    ((ms as u128 * TARGET_SAMPLE_RATE as u128) / 1000) as usize
}

fn in_speech_regions(time_ms: u64, spans: &[SpeechSpan]) -> bool {
    spans
        .iter()
        .any(|(start_ms, end_ms)| time_ms >= *start_ms && time_ms < *end_ms)
}

fn extract_f0_series(
    mono: &[f32],
    spans: &[SpeechSpan],
    config: &SpeechAnalysisConfig,
    pitch_min_hz: f32,
    pitch_max_hz: f32,
) -> (Vec<(u64, f32)>, usize) {
    let hop = ms_to_sample(config.pitch_hop_ms).max(128);
    if mono.len() < YIN_BUFFER_SAMPLES {
        return (Vec::new(), 0);
    }
    let mut detector = YINDetector::<f32>::new(YIN_BUFFER_SAMPLES, 0);
    let mut values = Vec::new();
    let mut candidates = 0usize;
    let mut pos = 0usize;
    while pos + YIN_BUFFER_SAMPLES <= mono.len() {
        let time_ms = (pos as u64 * 1000) / TARGET_SAMPLE_RATE as u64;
        if in_speech_regions(time_ms, spans) {
            let window = &mono[pos..pos + YIN_BUFFER_SAMPLES];
            if rms(window) >= config.quiet_rms_threshold {
                candidates += 1;
                if let Some(hz) = detector
                    .get_pitch(window, TARGET_SAMPLE_RATE as usize, 0.05, 0.15)
                    .map(|pitch| pitch.frequency)
                {
                    if hz >= pitch_min_hz && hz <= pitch_max_hz && hz.is_finite() {
                        values.push((time_ms, hz));
                    }
                }
            }
        }
        pos += hop;
    }
    (values, candidates)
}

fn clean_f0_series(values: &[(u64, f32)]) -> Vec<(u64, f32)> {
    if values.is_empty() {
        return Vec::new();
    }
    let hz_only: Vec<f32> = values.iter().map(|(_, h)| *h).collect();
    let smoothed = median_filter_5(&hz_only);
    let mut out = Vec::new();
    for (i, hz) in smoothed.into_iter().enumerate() {
        let time_ms = values[i.min(values.len() - 1)].0;
        if let Some((_, prev)) = out.last() {
            if *prev > 0.0 && hz > 0.0 {
                let prev_hz: f32 = *prev;
                let jump = 12.0 * (hz / prev_hz).log2().abs();
                if jump > MAX_FRAME_JUMP_SEMITONES {
                    continue;
                }
            }
        }
        out.push((time_ms, hz));
    }
    out
}

fn downsample_contour(series: &[(u64, f32)], max_points: usize) -> Vec<F0ContourPoint> {
    if series.is_empty() {
        return Vec::new();
    }
    let step = (series.len() / max_points.max(1)).max(1);
    series
        .iter()
        .step_by(step)
        .map(|(t, h)| F0ContourPoint {
            time_ms: *t,
            hz: Some(*h),
        })
        .collect()
}

fn median_filter_5(values: &[f32]) -> Vec<f32> {
    if values.len() < 5 {
        return median_filter_3(values);
    }
    let mut out = Vec::with_capacity(values.len());
    out.push(values[0]);
    out.push(values[1]);
    for i in 2..values.len().saturating_sub(2) {
        let mut window = [
            values[i - 2],
            values[i - 1],
            values[i],
            values[i + 1],
            values[i + 2],
        ];
        window.sort_by(|a, b| a.total_cmp(b));
        out.push(window[2]);
    }
    if values.len() > 3 {
        out.push(values[values.len() - 2]);
    }
    out.push(*values.last().unwrap());
    out
}

fn median_filter_3(values: &[f32]) -> Vec<f32> {
    if values.len() < 3 {
        return values.to_vec();
    }
    let mut out = vec![values[0]];
    for i in 1..values.len().saturating_sub(1) {
        let mut trio = [values[i - 1], values[i], values[i + 1]];
        trio.sort_by(|a, b| a.total_cmp(b));
        out.push(trio[1]);
    }
    if values.len() > 1 {
        out.push(*values.last().unwrap());
    }
    out
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let energy = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    energy.sqrt()
}

fn median_hz(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    Some(sorted[sorted.len() / 2])
}

fn percentile_range_semitones(
    values: &[f32],
    median: Option<f32>,
    lo_pct: f32,
    hi_pct: f32,
) -> Option<f32> {
    let base = median?;
    if values.len() < 4 || base <= 0.0 {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let lo_idx = ((sorted.len() as f32 - 1.0) * lo_pct).round() as usize;
    let hi_idx = ((sorted.len() as f32 - 1.0) * hi_pct).round() as usize;
    let p_lo = sorted[lo_idx.min(sorted.len() - 1)];
    let p_hi = sorted[hi_idx.min(sorted.len() - 1)];
    if p_lo > 0.0 && p_hi > 0.0 {
        Some(12.0 * (p_hi / p_lo).log2().abs())
    } else {
        None
    }
}

fn classify_expressiveness(
    spread_semitones: f32,
    config: &SpeechAnalysisConfig,
) -> PitchExpressiveness {
    if spread_semitones <= config.prosody_spread_ideal_lo * 0.75 {
        PitchExpressiveness::Monotone
    } else if spread_semitones >= config.prosody_spread_ideal_lo {
        PitchExpressiveness::Expressive
    } else {
        PitchExpressiveness::Moderate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p10_p90_spread_sane_for_stable_pitch() {
        let hz: Vec<f32> = (0..40).map(|i| 120.0 + (i as f32 % 3.0) * 0.5).collect();
        let spread = percentile_range_semitones(&hz, Some(120.0), 0.10, 0.90).unwrap_or(0.0);
        assert!(spread < 2.0, "spread {spread}");
    }

    #[test]
    fn octave_jump_removed_from_track() {
        let series = vec![(0, 120.0), (10, 120.0), (20, 240.0), (30, 122.0)];
        let cleaned = clean_f0_series(&series);
        assert!(!cleaned.iter().any(|(_, h)| (*h - 240.0).abs() < 1.0));
    }
}
