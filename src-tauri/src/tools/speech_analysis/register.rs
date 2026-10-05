//! Speech register (read aloud vs spontaneous) — norms and auto-detection.

use super::types::{FluencyReport, SpeechRegister, SpeechRegisterHint};

pub fn resolve_register(
    hint: SpeechRegisterHint,
    fluency: &FluencyReport,
    file_duration_ms: u64,
) -> (SpeechRegister, bool) {
    match hint {
        SpeechRegisterHint::Reading => (SpeechRegister::Reading, false),
        SpeechRegisterHint::Spontaneous => (SpeechRegister::Spontaneous, false),
        SpeechRegisterHint::Auto => (detect_register(fluency, file_duration_ms), true),
    }
}

fn detect_register(fluency: &FluencyReport, file_duration_ms: u64) -> SpeechRegister {
    let total = file_duration_ms.max(1) as f32;
    let pause_share = fluency.pause_total_ms as f32 / total;
    let wpm = fluency.wpm_phonation.unwrap_or(0.0);

    let mut reading = 0.0f32;
    let mut spontaneous = 0.0f32;

    if wpm >= 132.0 {
        reading += 2.0;
    } else if wpm <= 105.0 {
        spontaneous += 1.5;
    }
    if pause_share >= 0.38 {
        spontaneous += 2.5;
    } else if pause_share <= 0.18 {
        reading += 1.5;
    }
    if fluency.mean_pause_ms >= 550.0 {
        spontaneous += 1.5;
    } else if fluency.mean_pause_ms <= 320.0 && fluency.pause_count > 0 {
        reading += 1.0;
    }
    spontaneous += (fluency.fillers_per_100_words * 0.35).min(3.0);
    if fluency.pause_count_mid_phrase > fluency.pause_count_punctuation.saturating_add(2) {
        spontaneous += 1.0;
    }

    if reading > spontaneous + 0.75 {
        SpeechRegister::Reading
    } else {
        SpeechRegister::Spontaneous
    }
}

