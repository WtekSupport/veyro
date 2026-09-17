use crate::audio::denoise::denoise_mono_16k;
use crate::audio::segment::AudioSegment;

const HIGH_PASS_HZ: f32 = 80.0;
const TARGET_PEAK: f32 = 0.9;
const MIN_RMS: f32 = 0.003;

pub struct PreprocessOptions {
    pub enabled: bool,
    pub noise_reduction_enabled: bool,
    /// Peak normalize only — skip high-pass and denoise (safer for virtual mics).
    pub normalize_only: bool,
}

pub struct PreprocessResult {
    pub segment: AudioSegment,
    pub skipped_as_silence: bool,
}

pub fn audio_peak_rms(segment: &AudioSegment) -> (f32, f32) {
    let peak = segment
        .samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    (peak, compute_rms(&segment.samples))
}

/// True when the captured buffer is empty or has no measurable mic energy.
pub fn is_silent_segment(segment: &AudioSegment) -> bool {
    if segment.is_empty() {
        return true;
    }
    let (peak, rms) = audio_peak_rms(segment);
    peak <= f32::EPSILON && rms <= f32::EPSILON
}

pub fn preprocess_segment(segment: AudioSegment, options: PreprocessOptions) -> PreprocessResult {
    if !options.enabled || segment.is_empty() {
        return PreprocessResult {
            segment,
            skipped_as_silence: false,
        };
    }

    let mut samples = segment.samples.clone();
    if !options.normalize_only {
        if options.noise_reduction_enabled {
            denoise_mono_16k(&mut samples);
        }
        high_pass_filter(&mut samples, segment.sample_rate);
    }
    normalize_peak(&mut samples);

    let rms = compute_rms(&samples);
    if rms < MIN_RMS {
        return PreprocessResult {
            segment: AudioSegment::new(Vec::new(), segment.sample_rate, segment.channels),
            skipped_as_silence: true,
        };
    }

    PreprocessResult {
        segment: AudioSegment::new(samples, segment.sample_rate, segment.channels),
        skipped_as_silence: false,
    }
}

fn high_pass_filter(samples: &mut [f32], sample_rate: u32) {
    if samples.is_empty() || sample_rate == 0 {
        return;
    }

    let rc = 1.0 / (2.0 * std::f32::consts::PI * HIGH_PASS_HZ);
    let dt = 1.0 / sample_rate as f32;
    let alpha = rc / (rc + dt);

    // As if the signal had sat at its first sample forever: a DC input yields 0 from the start
    // instead of a click that decays over the first few milliseconds.
    let mut prev_input = samples[0];
    let mut prev_output = 0.0;
    for sample in samples.iter_mut() {
        let input = *sample;
        let output = alpha * (prev_output + input - prev_input);
        prev_input = input;
        prev_output = output;
        *sample = output;
    }
}

fn normalize_peak(samples: &mut [f32]) {
    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);

    if peak <= f32::EPSILON {
        return;
    }

    let gain = (TARGET_PEAK / peak).min(8.0);
    for sample in samples.iter_mut() {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
}

fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum: f32 = samples.iter().map(|sample| sample * sample).sum();
    (sum / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_peak_toward_target() {
        let samples = vec![0.1, -0.2, 0.05, -0.15];
        let segment = AudioSegment::new(samples, 16_000, 1);
        let result = preprocess_segment(
            segment,
            PreprocessOptions {
                enabled: true,
                noise_reduction_enabled: false,
                normalize_only: false,
            },
        );
        let peak = result
            .segment
            .samples
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        assert!((peak - TARGET_PEAK).abs() < 0.05);
    }

    #[test]
    fn detects_digital_silence() {
        let segment = AudioSegment::new(vec![0.0; 4_800], 16_000, 1);
        assert!(is_silent_segment(&segment));
        assert_eq!(segment.duration_ms, 300);
    }

    #[test]
    fn skips_near_silence_segments() {
        let samples = vec![0.0001; 800];
        let segment = AudioSegment::new(samples, 16_000, 1);
        let result = preprocess_segment(
            segment,
            PreprocessOptions {
                enabled: true,
                noise_reduction_enabled: false,
                normalize_only: false,
            },
        );
        assert!(result.skipped_as_silence);
        assert!(result.segment.is_empty());
    }

    #[test]
    fn high_pass_reduces_dc_offset() {
        let mut samples = vec![0.5; 256];
        high_pass_filter(&mut samples, 16_000);
        let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 0.05);
    }
}
