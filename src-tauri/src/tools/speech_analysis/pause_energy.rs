use super::config::SpeechAnalysisConfig;
use super::qc::SpeechSpan;

const FRAME_MS: u64 = 10;

pub fn frame_rms_series(mono: &[f32], sample_rate: u32) -> Vec<(u64, f32)> {
    if mono.is_empty() || sample_rate == 0 {
        return Vec::new();
    }
    let frame_samples = ((sample_rate as u64 * FRAME_MS) / 1000).max(1) as usize;
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < mono.len() {
        let end = (pos + frame_samples).min(mono.len());
        let slice = &mono[pos..end];
        let rms = if slice.is_empty() {
            0.0
        } else {
            (slice.iter().map(|s| s * s).sum::<f32>() / slice.len() as f32).sqrt()
        };
        let time_ms = (pos as u64 * 1000) / sample_rate as u64;
        out.push((time_ms, rms));
        pos += frame_samples;
    }
    out
}

#[derive(Debug, Clone, Copy)]
pub struct PauseInterval {
    pub start_ms: u64,
    pub duration_ms: u64,
}

pub struct EnergyPauseMetrics {
    pub pauses: Vec<PauseInterval>,
    pub measurement_reliable: bool,
}

/// Pauses from short-time RMS dips relative to speech peak (not word alignment gaps).
pub fn detect_pauses_from_energy(
    mono: &[f32],
    sample_rate: u32,
    speech_spans: &[SpeechSpan],
    total_duration_ms: u64,
    config: &SpeechAnalysisConfig,
) -> EnergyPauseMetrics {
    if mono.is_empty() || sample_rate == 0 {
        return EnergyPauseMetrics {
            pauses: Vec::new(),
            measurement_reliable: false,
        };
    }
    let rms_frames = frame_rms_series(mono, sample_rate);
    if rms_frames.is_empty() {
        return EnergyPauseMetrics {
            pauses: Vec::new(),
            measurement_reliable: false,
        };
    }

    let speech_levels: Vec<f32> = rms_frames
        .iter()
        .filter(|(t, _)| in_any_span(*t, speech_spans))
        .map(|(_, r)| *r)
        .filter(|r| *r > 1e-6)
        .collect();
    let peak = if speech_levels.is_empty() {
        rms_frames.iter().map(|(_, r)| *r).fold(0.0f32, f32::max)
    } else {
        let mut sorted = speech_levels;
        sorted.sort_by(|a, b| b.total_cmp(a));
        let idx = (sorted.len() / 10).min(sorted.len().saturating_sub(1));
        sorted[idx.max(0)]
    };
    if peak <= 1e-6 {
        return EnergyPauseMetrics {
            pauses: Vec::new(),
            measurement_reliable: false,
        };
    }
    let factor = 10f32.powf(-config.pause_energy_db_below_peak / 20.0);
    let threshold = peak * factor;

    let mut pauses = Vec::new();
    let mut run_start: Option<u64> = None;
    for (time_ms, rms) in &rms_frames {
        if *rms < threshold {
            if run_start.is_none() {
                run_start = Some(*time_ms);
            }
        } else if let Some(start) = run_start.take() {
            let dur = time_ms.saturating_sub(start);
            if dur >= config.min_pause_ms {
                pauses.push(PauseInterval {
                    start_ms: start,
                    duration_ms: dur,
                });
            }
        }
    }
    if let Some(start) = run_start {
        let tail = total_duration_ms.saturating_sub(start);
        if tail >= config.min_pause_ms {
            pauses.push(PauseInterval {
                start_ms: start,
                duration_ms: tail,
            });
        }
    }

    let speech_ms: u64 = speech_spans
        .iter()
        .map(|(a, b)| b.saturating_sub(*a))
        .sum();
    let reliable = !(speech_ms >= config.pause_min_speech_ms_for_reliability
        && pauses.is_empty()
        && rms_frames.len() > 100);

    EnergyPauseMetrics {
        pauses,
        measurement_reliable: reliable,
    }
}

fn in_any_span(time_ms: u64, spans: &[SpeechSpan]) -> bool {
    spans
        .iter()
        .any(|(s, e)| time_ms >= *s && time_ms < *e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_amp(freq: f32, sample_rate: u32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * freq * t).sin() * amp
            })
            .collect()
    }

    #[test]
    fn detects_silence_gap_in_energy() {
        let sr = 16_000u32;
        let speech = sine_amp(200.0, sr, sr as usize / 2, 0.3);
        let silence = vec![0.0f32; sr as usize / 4];
        let speech2 = sine_amp(200.0, sr, sr as usize / 2, 0.3);
        let mut mono = speech;
        mono.extend(silence);
        mono.extend(speech2);
        let total_ms = (mono.len() as u64 * 1000) / sr as u64;
        let spans = vec![(0, total_ms)];
        let config = SpeechAnalysisConfig::default();
        let m = detect_pauses_from_energy(&mono, sr, &spans, total_ms, &config);
        assert!(
            !m.pauses.is_empty(),
            "expected energy pause in silent gap, got {:?}",
            m.pauses
        );
    }
}
