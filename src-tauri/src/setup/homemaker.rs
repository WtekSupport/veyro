use serde::Serialize;

use crate::llm::model_store as llm_model_store;
use crate::settings::{
    local_llm_gpu_compiled, whisper_gpu_compiled, AppSettings, LlmModelKind, TextRewriteProvider,
    UiLocale, WhisperModelKind,
};
use crate::transcription::model_store as whisper_model_store;

use super::system_memory::total_physical_memory_mb;

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

const OS_RESERVE_MB: u64 = 3_500;
const LLM_RUNTIME_OVERHEAD_MB: u64 = 2_000;

pub fn recommend_homemaker_local_setup(settings: &AppSettings) -> HomemakerLocalSetup {
    let total_mb = total_physical_memory_mb();
    let (local_whisper_model, local_llm_model) =
        recommend_local_models_for_memory(total_mb, settings.ui_locale);

    let local_whisper_use_gpu = whisper_gpu_compiled();
    let local_llm_use_gpu = local_llm_gpu_compiled();

    HomemakerLocalSetup {
        local_whisper_model,
        local_llm_model,
        local_whisper_use_gpu,
        local_llm_use_gpu,
        whisper_download_needed: false,
        llm_download_needed: false,
        whisper_size_mb: local_whisper_model.approx_size_mb(),
        llm_size_mb: local_llm_model.approx_size_mb(),
    }
}

/// Prefer Whisper Large V3 Turbo and the Russian ~5 GB LLM when RAM allows; step down otherwise.
pub(crate) fn recommend_local_models_for_memory(
    total_mb: u64,
    locale: UiLocale,
) -> (WhisperModelKind, LlmModelKind) {
    let whisper_ladder = [
        WhisperModelKind::LargeV3Turbo,
        WhisperModelKind::Medium,
        WhisperModelKind::Small,
        WhisperModelKind::Base,
    ];

    let mut llm_candidates = Vec::new();
    if locale == UiLocale::Ru {
        llm_candidates.push(LlmModelKind::TLiteIt21);
    }
    llm_candidates.push(LlmModelKind::Qwen3_4B);
    llm_candidates.push(LlmModelKind::Gec08B);

    for whisper in whisper_ladder {
        for llm in &llm_candidates {
            if local_stack_fits(total_mb, whisper, *llm) {
                return (whisper, *llm);
            }
        }
    }

    (WhisperModelKind::Base, LlmModelKind::Gec08B)
}

fn local_stack_fits(total_mb: u64, whisper: WhisperModelKind, llm: LlmModelKind) -> bool {
    let whisper_runtime = (whisper.approx_size_mb() as u64 * 3) / 2;
    let need = OS_RESERVE_MB + whisper_runtime + llm.approx_size_mb() as u64 + LLM_RUNTIME_OVERHEAD_MB;
    total_mb >= need
}

pub fn get_homemaker_local_setup(settings: &AppSettings) -> HomemakerLocalSetup {
    let recommendation = recommend_homemaker_local_setup(settings);
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
    let recommendation = recommend_homemaker_local_setup(settings);
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
    if settings.local_whisper_beam_size != 5 {
        settings.local_whisper_beam_size = 5;
        changed = true;
    }

    if settings.ui_locale == UiLocale::Ru && settings.language.as_deref() != Some("ru") {
        settings.language = Some("ru".to_string());
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
    fn high_memory_ru_prefers_turbo_and_t_lite() {
        let (whisper, llm) = recommend_local_models_for_memory(32 * 1024, UiLocale::Ru);
        assert_eq!(whisper, WhisperModelKind::LargeV3Turbo);
        assert_eq!(llm, LlmModelKind::TLiteIt21);
    }

    #[test]
    fn high_memory_en_prefers_turbo_and_qwen() {
        let (whisper, llm) = recommend_local_models_for_memory(32 * 1024, UiLocale::En);
        assert_eq!(whisper, WhisperModelKind::LargeV3Turbo);
        assert_eq!(llm, LlmModelKind::Qwen3_4B);
    }

    #[test]
    fn low_memory_downgrades_models() {
        let (whisper, llm) = recommend_local_models_for_memory(6 * 1024, UiLocale::Ru);
        assert!(matches!(
            whisper,
            WhisperModelKind::Small | WhisperModelKind::Base | WhisperModelKind::Medium
        ));
        assert!(matches!(llm, LlmModelKind::Gec08B | LlmModelKind::Qwen3_4B));
    }

    #[test]
    fn apply_homemaker_sets_max_beam_and_ru_language() {
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            ui_locale: UiLocale::Ru,
            local_whisper_beam_size: 1,
            transcription_provider: "openai".to_string(),
            text_rewrite_provider: TextRewriteProvider::Openai,
            ..Default::default()
        };
        assert!(apply_homemaker_local_recommendations(&mut settings));
        assert_eq!(settings.local_whisper_beam_size, 5);
        assert_eq!(settings.language.as_deref(), Some("ru"));
        assert_eq!(settings.transcription_provider, "local");
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
