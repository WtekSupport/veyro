/// Energy-based alignment tuning (smart split uses speech-first timeline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyAlignConfig {
    /// Position between min and max log-RMS envelope (0..1). Higher = stricter gate (less audio counts as speech).
    pub speech_gate_ratio: f32,
    /// When true: detect speech blobs on the full waveform first, then place words only inside them.
    pub smart_speech_first: bool,
    /// Gaps shorter than this (ms) merge into one speech blob; aligns with subtitle pause split.
    pub acoustic_pause_ms: u64,
}

impl Default for EnergyAlignConfig {
    fn default() -> Self {
        Self {
            speech_gate_ratio: 0.25,
            smart_speech_first: false,
            acoustic_pause_ms: 400,
        }
    }
}

impl EnergyAlignConfig {
    pub fn from_smart_split(
        smart_split: bool,
        speech_gate_percent: u8,
        pause_split_ms: u64,
    ) -> Self {
        let percent = speech_gate_percent.clamp(1, 99) as f32;
        Self {
            speech_gate_ratio: percent / 100.0,
            smart_speech_first: smart_split,
            acoustic_pause_ms: pause_split_ms.clamp(150, 2_000),
        }
    }
}
