use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("audio error: {0}")]
    Audio(#[from] AudioError),
    #[error("configuration error: {0}")]
    Configuration(#[from] ConfigError),
    #[error("transcription error: {0}")]
    Transcription(String),
    #[error("injection error: {0}")]
    Injection(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("hotkey error: {0}")]
    Hotkey(String),
    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("microphone unavailable")]
    MicrophoneUnavailable,
    #[error("permission denied")]
    PermissionDenied,
    #[error("audio processing failed: {0}")]
    Processing(String),
    #[error("audio stream error: {0}")]
    Stream(String),
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read configuration: {0}")]
    Read(String),
    #[error("failed to write configuration: {0}")]
    Write(String),
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn to_payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: match self {
                Self::Audio(_) => "audio",
                Self::Configuration(_) => "configuration",
                Self::Transcription(_) => "transcription",
                Self::Injection(_) => "injection",
                Self::Network(_) => "network",
                Self::Hotkey(_) => "hotkey",
                Self::InvalidTransition { .. } => "state",
                Self::Internal(_) => "internal",
            }
            .to_string(),
            message: self.to_string(),
        }
    }
}

impl From<AppError> for String {
    fn from(error: AppError) -> Self {
        error.to_string()
    }
}
