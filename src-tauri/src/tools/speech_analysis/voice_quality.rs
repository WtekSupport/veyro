/// Lightweight voice perturbation metrics from the F0 track and frame RMS (not clinical CPPS).
pub struct VoiceQualityMetrics {
    pub jitter_local_percent: Option<f32>,
    pub shimmer_local_percent: Option<f32>,
    pub cpps_db: Option<f32>,
    pub low_confidence: bool,
}

pub fn analyze_voice_quality(
    f0_hz: &[(u64, f32)],
    frame_rms: &[(u64, f32)],
) -> VoiceQualityMetrics {
    if f0_hz.len() < 6 {
        return VoiceQualityMetrics {
            jitter_local_percent: None,
            shimmer_local_percent: None,
            cpps_db: None,
            low_confidence: true,
        };
    }

    let periods: Vec<f32> = f0_hz
        .windows(2)
        .filter_map(|w| {
            let _dt_ms = w[1].0.saturating_sub(w[0].0).max(1) as f32;
            let avg_hz = (w[0].1 + w[1].1) * 0.5;
            if avg_hz <= 0.0 {
                return None;
            }
            Some(1000.0 / avg_hz)
        })
        .collect();

    let jitter = local_perturbation_percent(&periods);

    let rms_values: Vec<f32> = frame_rms.iter().map(|(_, r)| *r).filter(|r| *r > 1e-6).collect();
    let shimmer = local_perturbation_percent(&rms_values);

    let cpps_db = cpps_proxy_db(frame_rms);

    VoiceQualityMetrics {
        jitter_local_percent: jitter,
        shimmer_local_percent: shimmer,
        cpps_db,
        low_confidence: jitter.is_none() && shimmer.is_none(),
    }
}

fn local_perturbation_percent(values: &[f32]) -> Option<f32> {
    if values.len() < 5 {
        return None;
    }
    let mut diffs = Vec::new();
    for w in values.windows(2) {
        let a = w[0].max(1e-6);
        let b = w[1].max(1e-6);
        diffs.push((b - a).abs() / ((a + b) * 0.5));
    }
    let mean = diffs.iter().sum::<f32>() / diffs.len() as f32;
    Some((mean * 100.0).clamp(0.0, 100.0))
}

/// Cepstral peak prominence proxy from log-RMS frames (lower = breathier / noisier).
fn cpps_proxy_db(frames: &[(u64, f32)]) -> Option<f32> {
    if frames.len() < 32 {
        return None;
    }
    let log_rms: Vec<f32> = frames
        .iter()
        .map(|(_, r)| r.max(1e-8).log10())
        .collect();
    let mean = log_rms.iter().sum::<f32>() / log_rms.len() as f32;
    let variance = log_rms
        .iter()
        .map(|v| (v - mean).powi(2))
        .sum::<f32>()
        / log_rms.len() as f32;
    let peak = log_rms.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    Some(((peak - mean) / variance.sqrt().max(1e-4) * 10.0).clamp(0.0, 30.0))
}
