/// Convert frame peak amplitude to the same 0–100 scale as the mic meter.
pub fn peak_level_percent(samples: &[f32]) -> u8 {
    if samples.is_empty() {
        return 0;
    }

    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0f32, f32::max);
    ((peak * 4.5).clamp(0.0, 1.0) * 100.0).round() as u8
}

/// Derive an auto threshold from ambient noise samples (p90 + margin).
pub fn compute_auto_threshold_percent(samples: &[u8]) -> u8 {
    if samples.is_empty() {
        return 12;
    }

    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() as f32 * 0.9).floor() as usize).min(sorted.len() - 1);
    let noise_floor = sorted[index];
    (noise_floor.saturating_add(8)).clamp(5, 50)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_level_matches_meter_scale() {
        assert!(peak_level_percent(&[0.2, -0.18, 0.15]) >= 20);
        assert!(peak_level_percent(&[0.0, 0.001]) < 5);
    }

    #[test]
    fn auto_threshold_adds_margin_to_noise_floor() {
        let samples = vec![3u8; 20];
        assert_eq!(compute_auto_threshold_percent(&samples), 11);
    }

    #[test]
    fn auto_threshold_clamps_high_values() {
        let samples = vec![45u8; 10];
        assert_eq!(compute_auto_threshold_percent(&samples), 50);
    }
}
