use pitch_detection::detector::yin::YINDetector;

use pitch_detection::detector::PitchDetector;



use crate::audio::resampler::{to_mono, MonoResampler, TARGET_SAMPLE_RATE};

use crate::audio::segment::AudioSegment;



use super::config::SpeechAnalysisConfig;

use super::qc::SpeechSpan;

use super::pause_energy::frame_rms_series;
use super::types::{F0ContourPoint, PitchExpressiveness, ProsodyReport, VoiceQualityReport};
use super::voice_quality::analyze_voice_quality;



const YIN_BUFFER_SAMPLES: usize = 2048;

const MAX_FRAME_JUMP_SEMITONES: f32 = 12.0;



pub fn analyze_prosody(

    source: &AudioSegment,

    speech_spans: &[SpeechSpan],

    config: &SpeechAnalysisConfig,

) -> ProsodyReport {

    let mono = normalize_mono(source);

    let raw_series = extract_f0_series(&mono, speech_spans, config);
    let cleaned_series = clean_f0_series(&raw_series);
    let hz_values: Vec<f32> = cleaned_series.iter().map(|(_, h)| *h).collect();
    let f0_contour = downsample_contour(&cleaned_series, 220);

    let voiced_fraction = if speech_spans.is_empty() {

        0.0

    } else {

        hz_values.len() as f32 / expected_f0_frames(&mono, speech_spans, config).max(1) as f32

    }

    .min(1.0);



    let f0_median_hz = median_hz(&hz_values);

    let f0_range_semitones = range_semitones(&hz_values, f0_median_hz);

    let f0_std_semitones = robust_spread_semitones(&hz_values, f0_median_hz);

    let expressiveness = classify_expressiveness(f0_std_semitones, config);

    let low_confidence = hz_values.len() < 8;

    let vq = analyze_voice_quality(&cleaned_series, &frame_rms_series(&mono, TARGET_SAMPLE_RATE));
    let voice_quality = Some(VoiceQualityReport {
        jitter_local_percent: vq.jitter_local_percent,
        shimmer_local_percent: vq.shimmer_local_percent,
        cpps_db: vq.cpps_db,
        low_confidence: vq.low_confidence,
    });

    ProsodyReport {
        f0_median_hz,
        f0_range_semitones,
        f0_std_semitones,
        voiced_fraction,
        expressiveness,
        low_confidence,
        f0_contour,
        voice_quality,
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
) -> Vec<(u64, f32)> {

    let hop = ms_to_sample(config.pitch_hop_ms).max(128);

    if mono.len() < YIN_BUFFER_SAMPLES {

        return Vec::new();

    }

    let mut detector = YINDetector::<f32>::new(YIN_BUFFER_SAMPLES, 0);

    let mut values = Vec::new();

    let mut pos = 0usize;

    while pos + YIN_BUFFER_SAMPLES <= mono.len() {

        let time_ms = (pos as u64 * 1000) / TARGET_SAMPLE_RATE as u64;

        if in_speech_regions(time_ms, spans) {

            let window = &mono[pos..pos + YIN_BUFFER_SAMPLES];

            if rms(window) >= config.quiet_rms_threshold {

                if let Some(hz) = detector

                    .get_pitch(window, TARGET_SAMPLE_RATE as usize, 0.05, 0.15)

                    .map(|pitch| pitch.frequency)

                {

                    if hz >= config.pitch_min_hz && hz <= config.pitch_max_hz && hz.is_finite() {

                        values.push((time_ms, hz));

                    }

                }

            }

        }

        pos += hop;

    }

    values

}

fn clean_f0_series(values: &[(u64, f32)]) -> Vec<(u64, f32)> {
    if values.is_empty() {
        return Vec::new();
    }
    let hz_only: Vec<f32> = values.iter().map(|(_, h)| *h).collect();
    let smoothed = median_filter_3(&hz_only);
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



fn expected_f0_frames(mono: &[f32], spans: &[SpeechSpan], config: &SpeechAnalysisConfig) -> usize {

    let hop = ms_to_sample(config.pitch_hop_ms).max(128);

    if mono.len() < YIN_BUFFER_SAMPLES {

        return 0;

    }

    let mut count = 0usize;

    let mut pos = 0usize;

    while pos + YIN_BUFFER_SAMPLES <= mono.len() {

        let time_ms = (pos as u64 * 1000) / TARGET_SAMPLE_RATE as u64;

        if in_speech_regions(time_ms, spans) {

            count += 1;

        }

        pos += hop;

    }

    let _ = spans;

    count

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



fn range_semitones(values: &[f32], median: Option<f32>) -> Option<f32> {

    let base = median?;

    if values.len() < 2 || base <= 0.0 {

        return None;

    }

    let mut sorted = values.to_vec();

    sorted.sort_by(|a, b| a.total_cmp(b));

    let p5 = sorted[sorted.len() / 5];

    let p95 = sorted[sorted.len() * 4 / 5];

    if p5 > 0.0 && p95 > 0.0 {

        Some(12.0 * (p95 / p5).log2().abs())

    } else {

        None

    }

}



/// Robust spread of F0 around the median (IQR to sigma), in semitones.

fn robust_spread_semitones(values: &[f32], median: Option<f32>) -> f32 {

    let base = match median {

        Some(m) if m > 0.0 => m,

        _ => return 0.0,

    };

    let mut devs: Vec<f32> = values

        .iter()

        .filter(|hz| **hz > 0.0)

        .map(|hz| 12.0 * (hz / base).log2().abs())

        .collect();

    if devs.len() < 4 {

        if devs.is_empty() {

            return 0.0;

        }

        let mean = devs.iter().sum::<f32>() / devs.len() as f32;

        let var = devs.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / devs.len() as f32;

        return var.sqrt();

    }

    devs.sort_by(|a, b| a.total_cmp(b));

    let p25 = devs[devs.len() / 4];

    let p75 = devs[devs.len() * 3 / 4];

    ((p75 - p25) / 1.349).max(0.0)

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

    fn robust_spread_is_zero_for_flat_pitch() {

        let hz = vec![120.0; 20];

        let spread = robust_spread_semitones(&hz, Some(120.0));

        assert!(spread < 0.05);

    }



    #[test]

    fn octave_jump_removed_from_track() {

        let series = vec![(0, 120.0), (10, 120.0), (20, 240.0), (30, 122.0)];

        let cleaned = clean_f0_series(&series);

        assert!(!cleaned.iter().any(|(_, h)| (*h - 240.0).abs() < 1.0));

    }

}

