use serde::Serialize;

use crate::llm::model_store as llm_model_store;
use crate::settings::{
    local_llm_gpu_compiled, whisper_gpu_compiled, AppSettings, LlmModelKind, LocalSttFamily,
    LocalSttModelKind, LocalSttQuant, LocalSttVariant, TextRewriteProvider, UiLocale, VadEngine,
};
use crate::transcription::local_stt_model_store::{self, variant_ram_mb};

use super::system_memory::total_physical_memory_mb;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HomemakerLocalSetup {
    pub local_stt_model: LocalSttModelKind,
    pub local_stt_family: LocalSttFamily,
    pub local_stt_quant: LocalSttQuant,
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
    let (variant, local_llm_model) = recommend_local_models_for_memory(total_mb, settings.ui_locale);

    let local_whisper_use_gpu = whisper_gpu_compiled();
    let local_llm_use_gpu = local_llm_gpu_compiled();
    let local_stt_model = variant
        .family
        .to_legacy_kind(variant.quant)
        .unwrap_or(LocalSttModelKind::WhisperBase);

    HomemakerLocalSetup {
        local_stt_model,
        local_stt_family: variant.family,
        local_stt_quant: variant.quant,
        local_llm_model,
        local_whisper_use_gpu,
        local_llm_use_gpu,
        whisper_download_needed: false,
        llm_download_needed: false,
        whisper_size_mb: crate::settings::variant_spec(variant).download_size_mb,
        llm_size_mb: local_llm_model.approx_size_mb(),
    }
}

/// Prefer Whisper Large V3 Turbo (Q5) and the Russian ~5 GB LLM when RAM allows; step down otherwise.
pub(crate) fn recommend_local_models_for_memory(
    total_mb: u64,
    locale: UiLocale,
) -> (LocalSttVariant, LlmModelKind) {
    let whisper_ladder = [
        LocalSttVariant::new(
            LocalSttFamily::WhisperLargeV3Turbo,
            LocalSttQuant::Q5,
        ),
        LocalSttVariant::new(LocalSttFamily::WhisperMedium, LocalSttQuant::Q5),
        LocalSttVariant::new(LocalSttFamily::WhisperSmall, LocalSttQuant::Q5),
        LocalSttVariant::new(LocalSttFamily::WhisperBase, LocalSttQuant::Q5),
    ];

    let mut llm_candidates = Vec::new();
    if locale == UiLocale::Ru {
        llm_candidates.push(LlmModelKind::TLiteIt21);
    }
    llm_candidates.push(LlmModelKind::Qwen3_4B);
    llm_candidates.push(LlmModelKind::Gec08B);

    for variant in whisper_ladder {
        for llm in &llm_candidates {
            if local_stack_fits(total_mb, variant, *llm) {
                return (variant, *llm);
            }
        }
    }

    (
        LocalSttVariant::new(LocalSttFamily::WhisperBase, LocalSttQuant::Q5),
        LlmModelKind::Gec08B,
    )
}

fn local_stack_fits(total_mb: u64, variant: LocalSttVariant, llm: LlmModelKind) -> bool {
    let stt_runtime = (variant_ram_mb(variant) as u64 * 3) / 2;
    let need = OS_RESERVE_MB + stt_runtime + llm.approx_size_mb() as u64 + LLM_RUNTIME_OVERHEAD_MB;
    total_mb >= need
}

pub fn get_homemaker_local_setup(settings: &AppSettings) -> HomemakerLocalSetup {
    let recommendation = recommend_homemaker_local_setup(settings);
    let variant = LocalSttVariant::new(
        recommendation.local_stt_family,
        recommendation.local_stt_quant,
    );
    let llm_path = llm_model_store::model_path_for(settings, recommendation.local_llm_model).ok();

    HomemakerLocalSetup {
        whisper_download_needed: !local_stt_model_store::bundle_ready_for_settings(settings, variant),
        llm_download_needed: llm_path
            .as_ref()
            .is_none_or(|path| !llm_model_store::model_exists(path)),
        ..recommendation
    }
}

pub fn apply_homemaker_local_recommendations(settings: &mut AppSettings) -> bool {
    let recommendation = recommend_homemaker_local_setup(settings);
    let mut changed = false;
    let variant = LocalSttVariant::new(
        recommendation.local_stt_family,
        recommendation.local_stt_quant,
    );

    if settings.local_stt_variant() != variant {
        settings.set_local_stt_variant(variant);
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

    if settings.vad_engine != VadEngine::Silero {
        settings.vad_engine = VadEngine::Silero;
        changed = true;
    }
    if !settings.silero_te {
        settings.silero_te = true;
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
        let (variant, llm) = recommend_local_models_for_memory(32 * 1024, UiLocale::Ru);
        assert_eq!(variant.family, LocalSttFamily::WhisperLargeV3Turbo);
        assert_eq!(variant.quant, LocalSttQuant::Q5);
        assert_eq!(llm, LlmModelKind::TLiteIt21);
    }

    #[test]
    fn high_memory_en_prefers_turbo_and_qwen() {
        let (variant, llm) = recommend_local_models_for_memory(32 * 1024, UiLocale::En);
        assert_eq!(variant.family, LocalSttFamily::WhisperLargeV3Turbo);
        assert_eq!(llm, LlmModelKind::Qwen3_4B);
    }

    #[test]
    fn low_memory_downgrades_models() {
        let (variant, llm) = recommend_local_models_for_memory(6 * 1024, UiLocale::Ru);
        assert!(matches!(
            variant.family,
            LocalSttFamily::WhisperSmall
                | LocalSttFamily::WhisperBase
                | LocalSttFamily::WhisperMedium
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
        assert_eq!(settings.vad_engine, VadEngine::Silero);
        assert!(settings.silero_te);
    }
}
