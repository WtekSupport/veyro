// Bell-shaped score: 100 at [ideal_lo, ideal_hi], linear falloff over soft_edge outside.
pub fn bell_score(value: f32, ideal_lo: f32, ideal_hi: f32, soft_edge: f32) -> f32 {
    if value >= ideal_lo && value <= ideal_hi {
        return 100.0;
    }
    if value < ideal_lo {
        let dist = ideal_lo - value;
        return (100.0 - (dist / soft_edge.max(0.01)) * 100.0).clamp(0.0, 100.0);
    }
    let dist = value - ideal_hi;
    (100.0 - (dist / soft_edge.max(0.01)) * 100.0).clamp(0.0, 100.0)
}

/// Prosody score from F0 spread (semitones). `ideal_lo`/`ideal_hi` from `calibration.toml`.
pub fn prosody_intonation_score(spread_semitones: f32, ideal_lo: f32, ideal_hi: f32) -> f32 {
    if spread_semitones >= ideal_lo && spread_semitones <= ideal_hi {
        return 100.0;
    }
    if spread_semitones < ideal_lo {
        let floor = 0.8f32;
        if spread_semitones <= floor {
            return 35.0;
        }
        return 35.0 + (spread_semitones - floor) / (ideal_lo - floor).max(0.01) * 45.0;
    }
    if spread_semitones <= ideal_hi + 2.5 {
        return (100.0 - (spread_semitones - ideal_hi) / 2.5 * 15.0).clamp(78.0, 100.0);
    }
    (78.0 - (spread_semitones - ideal_hi - 2.5) * 5.0).clamp(55.0, 78.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bell_peak_in_ideal_band() {
        assert!((bell_score(1.8, 1.0, 2.5, 1.5) - 100.0).abs() < 0.01);
    }

    #[test]
    fn bell_penalizes_monotone_and_hyper_variation() {
        let mono = bell_score(0.2, 1.0, 2.5, 1.5);
        let hyper = bell_score(5.0, 1.0, 2.5, 1.5);
        assert!(mono < 50.0);
        assert!(hyper < 50.0);
    }

    #[test]
    fn prosody_rewards_conversational_spread() {
        let live = prosody_intonation_score(3.8, 2.0, 5.5);
        let flat = prosody_intonation_score(0.6, 2.0, 5.5);
        let slightly_flat = prosody_intonation_score(1.51, 2.0, 5.5);
        assert!(live >= 95.0);
        assert!(flat < 50.0);
        assert!(slightly_flat < 75.0, "spread below ideal_lo should not score as excellent");
    }

    #[test]
    fn prosody_score_regression_fixed_spreads() {
        let cases = [(3.77, 100.0), (1.51, 62.0), (0.9, 43.0)];
        for (spread, expected_approx) in cases {
            let score = prosody_intonation_score(spread, 2.0, 5.5);
            assert!(
                (score - expected_approx).abs() < 8.0,
                "spread {spread} score {score}, expected ~{expected_approx}"
            );
        }
    }
}
