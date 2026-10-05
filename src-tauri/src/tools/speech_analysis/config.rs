use serde::Deserialize;

const CALIBRATION_TOML: &str = include_str!("../../../resources/speech_analysis/calibration.toml");

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
}

impl Default for SpeechAnalysisConfig {
    fn default() -> Self {
        load_speech_analysis_config()
    }
}

pub fn load_speech_analysis_config() -> SpeechAnalysisConfig {
    let mut config = SpeechAnalysisConfig {
            long_pause_ms: 250,
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
            min_speech_ms_articulation: 120_000,
            min_tokens_articulation: 200,
            weak_symbol_top_n: 8,
            min_weak_symbol_samples: 8,
            min_substitution_count: 3,
            weak_symbol_baseline_margin_pp: 3.0,
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
