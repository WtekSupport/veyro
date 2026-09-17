use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct WhisperDecodingOptions {
    pub logprob_thold: f32,
    pub entropy_thold: f32,
    pub temperature_inc: f32,
}

impl Default for WhisperDecodingOptions {
    fn default() -> Self {
        Self {
            logprob_thold: -1.0,
            entropy_thold: 2.4,
            temperature_inc: 0.2,
        }
    }
}

impl WhisperDecodingOptions {
    /// Looser thresholds for short / processed mic audio that Whisper would otherwise discard.
    pub fn permissive() -> Self {
        Self {
            logprob_thold: -5.0,
            entropy_thold: 3.5,
            temperature_inc: 0.6,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TranscriptionOptions {
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub model: String,
    pub whisper_decoding: Option<WhisperDecodingOptions>,
    pub dictionary_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_segments: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_peak: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_rms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_language: Option<String>,
}
