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

/// Prosody score from p10–p90 F0 spread (semitones). Bell curve — no flat 100 plateau.
pub fn prosody_intonation_score(spread_semitones: f32, ideal_lo: f32, ideal_hi: f32) -> f32 {
    let mid = (ideal_lo + ideal_hi) * 0.5;
    bell_score(spread_semitones, mid - 0.75, mid + 0.75, 3.5)
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
        let hyper = prosody_intonation_score(9.0, 2.0, 5.5);
        assert!(live >= 85.0 && live < 100.0);
        assert!(flat < 50.0);
        assert!(hyper < 75.0);
    }
}
