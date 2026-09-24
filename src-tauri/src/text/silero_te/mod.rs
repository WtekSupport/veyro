mod engine;
#[cfg(feature = "silero-te")]
pub mod combined_progress;
#[cfg(feature = "silero-te")]
pub mod model_store;
#[cfg(feature = "silero-te")]
pub mod runtime_store;
#[cfg(feature = "silero-te")]
mod process;
pub mod store;

pub use engine::SileroTeEngine;

use crate::settings::AppSettings;

/// Apply Silero text enhancement when enabled (local STT + Basic/Original).
pub fn apply_if_enabled(text: &str, settings: &AppSettings, language: &str) -> String {
    if !should_apply_silero_te(settings) {
        return text.to_string();
    }
    if text.trim().is_empty() {
        return text.to_string();
    }
    let Ok(engine) = SileroTeEngine::global().lock() else {
        return text.to_string();
    };
    match engine.enhance(text, language) {
        Ok(enhanced) => enhanced,
        Err(error) => {
            tracing::warn!("Silero TE failed, using raw transcription: {error}");
            text.to_string()
        }
    }
}

pub fn should_apply_silero_te(settings: &AppSettings) -> bool {
    if !compiled() {
        return false;
    }
    if !settings.silero_te {
        return false;
    }
    if settings.transcription_provider != "local" {
        return false;
    }
    if !store::assets_on_disk(settings) {
        return false;
    }
    #[cfg(feature = "silero-te")]
    if !runtime_store::runtime_ready(settings) {
        return false;
    }
    matches!(
        settings.immediate_transcription_mode(),
        crate::settings::TextProcessingMode::Original | crate::settings::TextProcessingMode::Basic
    )
}

#[cfg(feature = "silero-te")]
pub fn compiled() -> bool {
    true
}

#[cfg(not(feature = "silero-te"))]
pub fn compiled() -> bool {
    false
}
