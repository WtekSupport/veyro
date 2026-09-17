use serde::Serialize;

use crate::llm::model_store as llm_model_store;
use crate::settings::{
    local_llm_gpu_compiled, whisper_gpu_compiled, AppSettings, LlmModelKind, TextRewriteProvider,
    WhisperModelKind,
};
use crate::transcription::model_store as whisper_model_store;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HomemakerLocalSetup {
    pub local_whisper_model: WhisperModelKind,
    pub local_llm_model: LlmModelKind,
    pub local_whisper_use_gpu: bool,
    pub local_llm_use_gpu: bool,
    pub whisper_download_needed: bool,
    pub llm_download_needed: bool,
    pub whisper_size_mb: u32,
    pub llm_size_mb: u32,
}

pub fn recommend_homemaker_local_setup() -> HomemakerLocalSetup {
    let use_gpu = whisper_gpu_compiled() && local_llm_gpu_compiled();
    let local_whisper_model = if use_gpu {
        WhisperModelKind::LargeV3Turbo
    } else {
        WhisperModelKind::Base
    };
    let local_llm_model = LlmModelKind::Qwen3_4B;

    HomemakerLocalSetup {
        local_whisper_model,
        local_llm_model,
        local_whisper_use_gpu: use_gpu && whisper_gpu_compiled(),
        local_llm_use_gpu: use_gpu && local_llm_gpu_compiled(),
        whisper_download_needed: false,
        llm_download_needed: false,
        whisper_size_mb: local_whisper_model.approx_size_mb(),
        llm_size_mb: local_llm_model.approx_size_mb(),
    }
}

pub fn get_homemaker_local_setup(settings: &AppSettings) -> HomemakerLocalSetup {
    let recommendation = recommend_homemaker_local_setup();
    let whisper_path = whisper_model_store::model_path_for(
        settings,
        recommendation.local_whisper_model,
    )
    .ok();
    let llm_path = llm_model_store::model_path_for(settings, recommendation.local_llm_model).ok();

    HomemakerLocalSetup {
        whisper_download_needed: whisper_path
            .as_ref()
            .is_none_or(|path| !whisper_model_store::model_exists(path)),
        llm_download_needed: llm_path
            .as_ref()
            .is_none_or(|path| !llm_model_store::model_exists(path)),
        ..recommendation
    }
}

pub fn apply_homemaker_local_recommendations(settings: &mut AppSettings) -> bool {
    let recommendation = recommend_homemaker_local_setup();
    let mut changed = false;

    if settings.local_whisper_model != recommendation.local_whisper_model {
        settings.local_whisper_model = recommendation.local_whisper_model;
        changed = true;
    }
    if settings.local_llm_model != recommendation.local_llm_model {
        settings.local_llm_model = recommendation.local_llm_model;
        changed = true;
    }
    if settings.local_whisper_use_gpu != recommendation.local_whisper_use_gpu {
        settings.local_whisper_use_gpu = recommendation.local_whisper_use_gpu;
        changed = true;
    }
    if settings.local_llm_use_gpu != recommendation.local_llm_use_gpu {
        settings.local_llm_use_gpu = recommendation.local_llm_use_gpu;
        changed = true;
    }

    settings.transcription_provider = "local".to_string();
    if !matches!(settings.text_rewrite_provider, TextRewriteProvider::Local) {
        settings.text_rewrite_provider = TextRewriteProvider::Local;
        changed = true;
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::UiMode;

    #[test]
    fn recommends_gpu_models_when_gpu_compiled() {
        let setup = recommend_homemaker_local_setup();
        if whisper_gpu_compiled() && local_llm_gpu_compiled() {
            assert_eq!(setup.local_whisper_model, WhisperModelKind::LargeV3Turbo);
            assert!(setup.local_whisper_use_gpu);
            assert!(setup.local_llm_use_gpu);
        } else {
            assert_eq!(setup.local_whisper_model, WhisperModelKind::Base);
            assert!(!setup.local_whisper_use_gpu);
            assert!(!setup.local_llm_use_gpu);
        }
        assert_eq!(setup.local_llm_model, LlmModelKind::Qwen3_4B);
    }

    #[test]
    fn apply_homemaker_local_recommendations_sets_local_providers() {
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            transcription_provider: "openai".to_string(),
            text_rewrite_provider: TextRewriteProvider::Openai,
            ..Default::default()
        };
        assert!(apply_homemaker_local_recommendations(&mut settings));
        assert_eq!(settings.transcription_provider, "local");
        assert_eq!(settings.text_rewrite_provider, TextRewriteProvider::Local);
    }
}
