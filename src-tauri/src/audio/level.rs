use std::sync::atomic::{AtomicU32, Ordering};

/// Shared mic level in permille (0..=1000).
pub fn update_mic_level(level: &AtomicU32, samples: &[f32]) {
    if samples.is_empty() {
        return;
    }

    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0f32, f32::max);

    // Typical speech peaks land around 0.05–0.35 after normalization.
    let instant = (peak * 4.5).clamp(0.0, 1.0);
    let current = level.load(Ordering::Relaxed) as f32 / 1000.0;
    let smoothed = instant.max(current * 0.7);
    level.store((smoothed * 1000.0) as u32, Ordering::Relaxed);
}

pub fn decay_mic_level(level: &AtomicU32) {
    let current = level.load(Ordering::Relaxed);
    if current == 0 {
        return;
    }
    let next = (current as f32 * 0.86) as u32;
    level.store(next, Ordering::Relaxed);
}

pub fn mic_level_percent(level: &AtomicU32) -> u32 {
    level.load(Ordering::Relaxed).min(1000) / 10
}

pub fn peak_level_percent(samples: &[f32]) -> u8 {
    crate::vad::threshold::peak_level_percent(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_stays_low() {
        let level = AtomicU32::new(0);
        update_mic_level(&level, &[0.0, 0.0, 0.001]);
        assert!(mic_level_percent(&level) < 5);
    }

    #[test]
    fn speech_like_signal_raises_meter() {
        let level = AtomicU32::new(0);
        update_mic_level(&level, &[0.2, -0.18, 0.15, -0.12]);
        assert!(mic_level_percent(&level) > 20);
    }
}
