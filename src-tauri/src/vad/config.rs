use serde::{Deserialize, Serialize};

use crate::settings::VadThresholdMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VadConfig {
    pub pre_speech_buffer_ms: u32,
    pub minimum_speech_ms: u32,
    pub silence_timeout_ms: u32,
    pub maximum_segment_ms: u32,
    pub threshold_mode: VadThresholdMode,
    pub voice_threshold_percent: u8,
    pub auto_threshold_percent: u8,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            pre_speech_buffer_ms: 300,
            minimum_speech_ms: 250,
            silence_timeout_ms: 700,
            maximum_segment_ms: 30_000,
            threshold_mode: VadThresholdMode::Auto,
            voice_threshold_percent: 15,
            auto_threshold_percent: 12,
        }
    }
}

impl VadConfig {
    pub fn effective_threshold_percent(&self) -> u8 {
        match self.threshold_mode {
            VadThresholdMode::Auto => self.auto_threshold_percent,
            VadThresholdMode::Manual => self.voice_threshold_percent,
        }
    }
}

impl VadConfig {
    pub fn pre_speech_samples(&self, sample_rate: u32) -> usize {
        ((self.pre_speech_buffer_ms as u64 * sample_rate as u64) / 1000) as usize
    }

    pub fn minimum_speech_samples(&self, sample_rate: u32) -> usize {
        ((self.minimum_speech_ms as u64 * sample_rate as u64) / 1000) as usize
    }

    pub fn silence_timeout_samples(&self, sample_rate: u32) -> usize {
        ((self.silence_timeout_ms as u64 * sample_rate as u64) / 1000) as usize
    }

    pub fn maximum_segment_samples(&self, sample_rate: u32) -> usize {
        ((self.maximum_segment_ms as u64 * sample_rate as u64) / 1000) as usize
    }
}
