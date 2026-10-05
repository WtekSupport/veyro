//! Phrase boundaries from prosody (F0 reset, long pause) — not STT punctuation.

use super::pause_energy::PauseInterval;
use super::types::F0ContourPoint;

const F0_RESET_SEMITONES: f32 = 1.15;

pub fn pause_is_prosodic_phrase_boundary(
    pause: &PauseInterval,
    f0_contour: &[F0ContourPoint],
    long_pause_ms: u64,
) -> bool {
    if pause.duration_ms >= long_pause_ms {
        return true;
    }
    if pause.duration_ms >= 900 {
        return true;
    }
    f0_reset_before_pause(pause.start_ms, f0_contour)
}

fn f0_reset_before_pause(pause_start_ms: u64, contour: &[F0ContourPoint]) -> bool {
    let voiced: Vec<(u64, f32)> = contour
        .iter()
        .filter_map(|p| p.hz.filter(|h| *h > 0.0).map(|h| (p.time_ms, h)))
        .collect();
    if voiced.len() < 6 {
        return false;
    }
    let early = median_hz_window(&voiced, pause_start_ms.saturating_sub(900), pause_start_ms.saturating_sub(250));
    let late = median_hz_window(&voiced, pause_start_ms.saturating_sub(250), pause_start_ms.saturating_add(80));
    match (early, late) {
        (Some(e), Some(l)) if e > 0.0 && l > 0.0 => {
            let drop = 12.0 * (e / l).log2();
            drop >= F0_RESET_SEMITONES
        }
        (Some(_), None) => true,
        _ => false,
    }
}

fn median_hz_window(samples: &[(u64, f32)], start_ms: u64, end_ms: u64) -> Option<f32> {
    if end_ms <= start_ms {
        return None;
    }
    let mut hz: Vec<f32> = samples
        .iter()
        .filter(|(t, _)| *t >= start_ms && *t < end_ms)
        .map(|(_, h)| *h)
        .collect();
    if hz.is_empty() {
        return None;
    }
    hz.sort_by(|a, b| a.total_cmp(b));
    Some(hz[hz.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_pause_is_boundary() {
        let p = PauseInterval {
            start_ms: 1000,
            duration_ms: 700,
        };
        assert!(pause_is_prosodic_phrase_boundary(&p, &[], 600));
    }
}
