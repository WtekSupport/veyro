use async_trait::async_trait;

use crate::audio::segment::AudioSegment;
use crate::network::openai_error::OpenAiError;
use crate::settings::UiLocale;

use super::models::{TranscriptionOptions, TranscriptionResult};

#[derive(Debug, Clone, thiserror::Error)]
pub enum TranscriptionError {
    #[error("{0}")]
    OpenAi(#[from] OpenAiError),
    #[error("request cancelled")]
    Cancelled,
    #[error("invalid audio: {0}")]
    InvalidAudio(String),
    #[error("whisper model not found: {0}")]
    ModelNotFound(String),
    #[error("local inference failed: {0}")]
    InferenceFailed(String),
}

impl TranscriptionError {
    pub fn user_message(&self, locale: UiLocale) -> String {
        match self {
            Self::OpenAi(error) => error.user_message(locale),
            Self::Cancelled => crate::i18n::translate(locale, "openai.cancelled", &[]),
            Self::InvalidAudio(detail) => {
                crate::i18n::translate(locale, "openai.invalid_audio", &[("detail", detail)])
            }
            Self::ModelNotFound(path) => {
                crate::i18n::translate(locale, "openai.local_model_missing", &[("detail", path)])
            }
            Self::InferenceFailed(detail) => {
                crate::i18n::translate(locale, "openai.local_inference_failed", &[("detail", detail)])
            }
        }
    }

    /// Network/server failures after retries — safe to reload STT and return to ready.
    pub fn recoverable_after_retry(&self) -> bool {
        matches!(self, Self::OpenAi(error) if error.retryable())
    }
}

#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(
        &self,
        audio: AudioSegment,
        options: TranscriptionOptions,
    ) -> Result<TranscriptionResult, TranscriptionError>;

    /// Load local model weights and run a minimal inference pass when supported.
    async fn prewarm(&self) -> Result<(), TranscriptionError> {
        Ok(())
    }

    /// Partial transcription while audio is still being captured (local Whisper only).
    async fn transcribe_preview(
        &self,
        _audio: AudioSegment,
        _options: TranscriptionOptions,
    ) -> Result<Option<String>, TranscriptionError> {
        Ok(None)
    }

    /// Synchronous preview path for the dedicated preview worker thread.
    fn transcribe_preview_sync(
        &self,
        _audio: AudioSegment,
        _options: TranscriptionOptions,
    ) -> Result<Option<String>, TranscriptionError> {
        Ok(None)
    }

    /// Release resident model weights when switching away from local STT.
    async fn unload(&self) -> Result<(), TranscriptionError> {
        Ok(())
    }

    fn is_model_loaded(&self) -> bool {
        false
    }
}
