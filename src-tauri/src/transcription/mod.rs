pub mod factory;
#[cfg(feature = "local-whisper")]
pub mod local;
#[cfg(feature = "local-sherpa-stt")]
pub mod local_sherpa;
pub mod local_stt_model_store;
pub mod live_dictation;
pub mod streaming;
pub mod languages;
pub mod model_store;
#[cfg(feature = "local-sherpa-stt")]
pub mod sherpa;
pub mod models;
pub mod openai;
pub mod prompt;
pub mod provider;

pub use factory::create_transcriber;
pub use languages::{list_transcription_languages, TranscriptionLanguage};
pub use crate::timed_text::TimedTextSegment;
pub use models::{TranscriptionOptions, TranscriptionResult, WhisperDecodingOptions};
pub use openai::OpenAITranscriptionProvider;
pub use live_dictation::LiveDictationSession;
pub use provider::{TranscriptionError, TranscriptionProvider};
