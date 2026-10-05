use crate::audio::segment::AudioSegment;
use crate::audio::resampler::to_mono;

use super::types::{QcReport, ReliabilityLevel};

pub type SpeechSpan = (u64, u64);

pub fn analyze_qc(
    source: &AudioSegment,
    speech_spans: &[SpeechSpan],
    source_file_sample_rate_hz: Option<u32>,
    working_sample_rate_hz: u32,
) -> QcReport {
    let mono = to_mono(&source.samples, source.channels);
    let duration_ms = source.duration_ms.max(1);
    let speech_duration_ms = speech_spans
        .iter()
        .map(|(start, end)| end.saturating_sub(*start))
        .sum::<u64>();

    let clip_ratio = clip_ratio(&mono);
    let snr_db_estimate = estimate_snr_db(&mono, speech_spans, source.sample_rate);
    let narrowband = source.sample_rate < 12_000;

    let mut flags = Vec::new();
    let mut reliability = ReliabilityLevel::High;

    if let Some(snr) = snr_db_estimate {
        if snr < 10.0 {
            reliability = ReliabilityLevel::Low;
            flags.push("low_snr".to_string());
        } else if snr < 15.0 {
            reliability = ReliabilityLevel::Medium;
            flags.push("moderate_snr".to_string());
        } else if snr > 40.0 {
            flags.push("possibly_processed".to_string());
        }
    }

    if clip_ratio > 0.001 {
        if clip_ratio > 0.01 {
            reliability = ReliabilityLevel::Low;
        } else if reliability == ReliabilityLevel::High {
            reliability = ReliabilityLevel::Medium;
        }
        flags.push("clipping".to_string());
    }

    if narrowband {
        if reliability == ReliabilityLevel::High {
            reliability = ReliabilityLevel::Medium;
        }
        flags.push("narrowband".to_string());
    }

    if speech_duration_ms < 60_000 {
        flags.push("short_speech".to_string());
    }

    QcReport {
        duration_ms,
        speech_duration_ms,
        sample_rate_hz: working_sample_rate_hz,
        source_file_sample_rate_hz,
        snr_db_estimate,
        clip_ratio,
        narrowband,
        reliability,
        flags,
    }
}

fn clip_ratio(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let clipped = samples
        .iter()
        .filter(|sample| sample.abs() >= 0.99)
        .count();
    clipped as f32 / samples.len() as f32
}

fn estimate_snr_db(mono: &[f32], spans: &[SpeechSpan], sample_rate: u32) -> Option<f32> {
    if mono.is_empty() || sample_rate == 0 {
        return None;
    }
    let speech_energy = region_energy(mono, spans, sample_rate);
    let noise_energy = non_speech_energy(mono, spans, sample_rate);
    if speech_energy <= 0.0 || noise_energy <= 0.0 {
        return None;
    }
    Some(10.0 * (speech_energy / noise_energy).log10())
}

fn region_energy(mono: &[f32], spans: &[SpeechSpan], sample_rate: u32) -> f32 {
    let mut sum = 0.0_f64;
    let mut count = 0usize;
    for (start_ms, end_ms) in spans {
        let start = ms_to_sample(*start_ms, sample_rate);
        let end = ms_to_sample(*end_ms, sample_rate).min(mono.len());
        for sample in &mono[start..end] {
            sum += (*sample as f64) * (*sample as f64);
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    (sum / count as f64).sqrt() as f32
}

fn non_speech_energy(mono: &[f32], spans: &[SpeechSpan], sample_rate: u32) -> f32 {
    if spans.is_empty() {
        return rms(mono);
    }
    let mut sum = 0.0_f64;
    let mut count = 0usize;
    let mut cursor = 0usize;
    let mut sorted = spans.to_vec();
    sorted.sort_by_key(|span| span.0);
    for (start_ms, end_ms) in sorted {
        let start = ms_to_sample(start_ms, sample_rate);
        let end = ms_to_sample(end_ms, sample_rate);
        if cursor < start {
            for sample in &mono[cursor..start.min(mono.len())] {
                sum += (*sample as f64) * (*sample as f64);
                count += 1;
            }
        }
        cursor = end.min(mono.len());
    }
    if cursor < mono.len() {
        for sample in &mono[cursor..] {
            sum += (*sample as f64) * (*sample as f64);
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    (sum / count as f64).sqrt() as f32
}

fn ms_to_sample(ms: u64, sample_rate: u32) -> usize {
    ((ms as u128 * sample_rate as u128) / 1000) as usize
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let energy = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    energy.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::segment::AudioSegment;

    #[test]
    fn clip_ratio_counts_near_full_scale() {
        let samples = vec![0.0, 0.99, -1.0, 0.1];
        assert!((clip_ratio(&samples) - 0.5).abs() < 0.01);
    }

    #[test]
    fn qc_flags_narrowband() {
        let seg = AudioSegment::new(vec![0.1; 8000], 8000, 1);
        let qc = analyze_qc(&seg, &[(0, 500)], Some(8000), 8000);
        assert!(qc.narrowband);
    }
}
