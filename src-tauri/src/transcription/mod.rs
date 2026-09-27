pub mod factory;
#[cfg(feature = "local-whisper")]
pub mod local;
#[cfg(feature = "local-sherpa-stt")]
pub mod local_sherpa;
pub mod local_stt_model_store;
pub mod language;
#[cfg(feature = "local-whisper")]
pub mod whisper_auto_lang;
pub mod languages;
pub mod model_store;
#[cfg(feature = "local-sherpa-stt")]
pub mod sherpa;
pub mod models;
pub mod openai;
pub mod prompt;
pub mod provider;

pub use factory::create_transcriber;
pub use language::{
    normalize_stt_language_code, postprocess_language, whisper_prompt_sticky_language,
    whisper_transcription_language, STT_AUTO_FALLBACK_LANG,
};
pub use languages::{list_transcription_languages, TranscriptionLanguage};
pub use crate::timed_text::TimedTextSegment;
pub use models::{TranscriptionOptions, TranscriptionResult, WhisperDecodingOptions};
pub use openai::OpenAITranscriptionProvider;
pub use provider::{TranscriptionError, TranscriptionProvider};
