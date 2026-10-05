use serde::Deserialize;

use super::types::SpeechRegister;

const CALIBRATION_TOML: &str = include_str!("../../../resources/speech_analysis/calibration.toml");

#[derive(Debug, Clone, Copy)]
pub struct RegisterNorms {
    pub wpm_ideal_lo: f32,
    pub wpm_ideal_hi: f32,
    pub wpm_soft_edge: f32,
    pub long_pause_ms: u64,
    pub min_speech_ms_articulation: u64,
    pub prosody_spread_ideal_lo: f32,
    pub prosody_spread_ideal_hi: f32,
}

#[derive(Debug, Clone)]
pub struct SpeechAnalysisConfig {
    pub long_pause_ms: u64,
    pub very_long_pause_ms: u64,
    /// Gaps shorter than this are inter-word spacing, not pauses.
    pub min_pause_ms: u64,
    /// RMS pause threshold: this many dB below speech peak (see `resources/speech_analysis/calibration.toml`).
    pub pause_energy_db_below_peak: f32,
    /// If VAD speech is longer than this and energy finds zero pauses, pause metrics are unreliable.
    pub pause_min_speech_ms_for_reliability: u64,
    /// Prosody F0 spread (st): below `prosody_spread_ideal_lo` is treated as monotone-ish.
    pub prosody_spread_ideal_lo: f32,
    pub prosody_spread_ideal_hi: f32,
    pub quiet_rms_threshold: f32,
    pub pitch_hop_ms: u64,
    pub pitch_min_hz: f32,
    pub pitch_max_hz: f32,
    pub filler_top_n: usize,
    pub confusion_top_n: usize,
    pub low_gop_threshold: f32,
    pub min_speech_ms_fluency: u64,
    pub min_speech_ms_prosody: u64,
    pub min_words_intelligibility: usize,
    pub min_speech_ms_articulation: u64,
    pub min_tokens_articulation: usize,
    pub weak_symbol_top_n: usize,
    pub min_weak_symbol_samples: usize,
    pub min_substitution_count: usize,
    /// Minimum error-rate excess (percentage points) above recording baseline to list a letter.
    pub weak_symbol_baseline_margin_pp: f32,
    /// Diction axes with Available scores required before showing a numeric overall.
    pub min_axes_for_overall: u8,
    /// Word count before OOV/suspicious tokens can lower intelligibility score.
    pub min_words_transcript_qc_score: usize,
    /// PER at or below this on short self-consistency runs is treated as saturated (no proxy score).
    pub articulation_per_saturated_max: f32,
}

impl Default for SpeechAnalysisConfig {
    fn default() -> Self {
        load_speech_analysis_config()
    }
}

pub fn load_speech_analysis_config() -> SpeechAnalysisConfig {
    let mut config = SpeechAnalysisConfig {
            long_pause_ms: 600,
            very_long_pause_ms: 1500,
            min_pause_ms: 200,
            pause_energy_db_below_peak: 38.0,
            pause_min_speech_ms_for_reliability: 30_000,
            prosody_spread_ideal_lo: 2.0,
            prosody_spread_ideal_hi: 5.5,
            quiet_rms_threshold: 0.012,
            pitch_hop_ms: 20,
            pitch_min_hz: 75.0,
            pitch_max_hz: 400.0,
            filler_top_n: 8,
            confusion_top_n: 5,
            low_gop_threshold: -0.35,
            min_speech_ms_fluency: 25_000,
            min_speech_ms_prosody: 20_000,
            min_words_intelligibility: 15,
            min_speech_ms_articulation: 45_000,
            min_tokens_articulation: 200,
            weak_symbol_top_n: 8,
            min_weak_symbol_samples: 8,
            min_substitution_count: 3,
            weak_symbol_baseline_margin_pp: 3.0,
            min_axes_for_overall: 2,
            min_words_transcript_qc_score: 100,
            articulation_per_saturated_max: 0.02,
        };
    #[derive(Deserialize)]
    struct CalibrationFile {
        prosody: Option<ProsodyCalibration>,
        pause: Option<PauseCalibration>,
    }
    #[derive(Deserialize)]
    struct ProsodyCalibration {
        spread_ideal_lo: Option<f32>,
        spread_ideal_hi: Option<f32>,
    }
    #[derive(Deserialize)]
    struct PauseCalibration {
        energy_db_below_peak: Option<f32>,
        min_ms: Option<u64>,
        long_ms: Option<u64>,
        very_long_ms: Option<u64>,
    }
    if let Ok(file) = toml::from_str::<CalibrationFile>(CALIBRATION_TOML) {
        if let Some(p) = file.prosody {
            if let Some(v) = p.spread_ideal_lo {
                config.prosody_spread_ideal_lo = v;
            }
            if let Some(v) = p.spread_ideal_hi {
                config.prosody_spread_ideal_hi = v;
            }
        }
        if let Some(p) = file.pause {
            if let Some(v) = p.energy_db_below_peak {
                config.pause_energy_db_below_peak = v;
            }
            if let Some(v) = p.min_ms {
                config.min_pause_ms = v;
            }
            if let Some(v) = p.long_ms {
                config.long_pause_ms = v;
            }
            if let Some(v) = p.very_long_ms {
                config.very_long_pause_ms = v;
            }
        }
    }
    config
}

pub fn register_norms(base: &SpeechAnalysisConfig, register: SpeechRegister) -> RegisterNorms {
    let mut norms = RegisterNorms {
        wpm_ideal_lo: 110.0,
        wpm_ideal_hi: 150.0,
        wpm_soft_edge: 35.0,
        long_pause_ms: base.long_pause_ms,
        min_speech_ms_articulation: base.min_speech_ms_articulation,
        prosody_spread_ideal_lo: base.prosody_spread_ideal_lo,
        prosody_spread_ideal_hi: base.prosody_spread_ideal_hi,
    };
    #[derive(Deserialize)]
    struct RegisterCalibration {
        wpm_ideal_lo: Option<f32>,
        wpm_ideal_hi: Option<f32>,
        wpm_soft_edge: Option<f32>,
        long_pause_ms: Option<u64>,
        min_speech_ms_articulation: Option<u64>,
        spread_ideal_lo: Option<f32>,
        spread_ideal_hi: Option<f32>,
    }
    #[derive(Deserialize)]
    struct CalibrationRegisters {
        reading: Option<RegisterCalibration>,
        spontaneous: Option<RegisterCalibration>,
    }
    let section = match register {
        SpeechRegister::Reading => toml::from_str::<CalibrationRegisters>(CALIBRATION_TOML)
            .ok()
            .and_then(|f| f.reading),
        SpeechRegister::Spontaneous => toml::from_str::<CalibrationRegisters>(CALIBRATION_TOML)
            .ok()
            .and_then(|f| f.spontaneous),
    };
    if let Some(r) = section {
        if let Some(v) = r.wpm_ideal_lo {
            norms.wpm_ideal_lo = v;
        }
        if let Some(v) = r.wpm_ideal_hi {
            norms.wpm_ideal_hi = v;
        }
        if let Some(v) = r.wpm_soft_edge {
            norms.wpm_soft_edge = v;
        }
        if let Some(v) = r.long_pause_ms {
            norms.long_pause_ms = v;
        }
        if let Some(v) = r.min_speech_ms_articulation {
            norms.min_speech_ms_articulation = v;
        }
        if let Some(v) = r.spread_ideal_lo {
            norms.prosody_spread_ideal_lo = v;
        }
        if let Some(v) = r.spread_ideal_hi {
            norms.prosody_spread_ideal_hi = v;
        }
    }
    norms
}

pub fn load_fillers(language_hint: Option<&str>) -> Vec<String> {
    let ru = include_str!("../../../resources/speech_analysis/fillers_ru.toml");
    let en = include_str!("../../../resources/speech_analysis/fillers_en.toml");
    let use_ru = language_hint
        .map(|lang| lang.starts_with("ru"))
        .unwrap_or(true);
    let raw = if use_ru { ru } else { en };
    #[derive(Deserialize)]
    struct FillersFile {
        fillers: Vec<String>,
    }
    toml::from_str::<FillersFile>(raw)
        .map(|file| file.fillers)
        .unwrap_or_default()
}
