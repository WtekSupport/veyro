use std::sync::Arc;

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

pub type WhisperProgressCallback = Arc<dyn Fn(u8) + Send + Sync + 'static>;

#[derive(Clone, Default)]
pub struct TranscriptionOptions {
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub model: String,
    pub whisper_decoding: Option<WhisperDecodingOptions>,
    pub dictionary_path: Option<String>,
    /// Local Whisper only: 0–100 during `whisper_full`.
    pub whisper_progress: Option<WhisperProgressCallback>,
}

impl std::fmt::Debug for TranscriptionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranscriptionOptions")
            .field("language", &self.language)
            .field("prompt", &self.prompt)
            .field("model", &self.model)
            .field("whisper_decoding", &self.whisper_decoding)
            .field("dictionary_path", &self.dictionary_path)
            .field(
                "whisper_progress",
                &self.whisper_progress.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

use crate::timed_text::TimedTextSegment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_segments: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timed_segments: Option<Vec<TimedTextSegment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_peak: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_rms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_language: Option<String>,
}
