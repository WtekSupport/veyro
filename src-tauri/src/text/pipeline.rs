use reqwest::Client;

use crate::llm::LlmEngine;
use crate::settings::{AppSettings, TextProcessingMode};
use crate::text::corrections::apply_corrections;
use crate::text::dictionary::{load_dictionary, protected_terms};
use crate::text::enter_trigger::apply_enter_trigger;
use crate::text::normalize::{
    apply_basic_cleanup, apply_original, apply_spoken_punctuation,
    clean_raw_transcription_with_dictionary, ensure_spaces_after_punctuation,
};
use crate::text::numbers::apply_numbers_as_words;
use crate::text::rewrite::rewrite_transcription;

#[derive(Debug, Clone)]
pub struct ProcessedText {
    pub text: String,
    pub press_enter: bool,
    pub rewrite_fallback: bool,
    pub rewrite_fallback_reason: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TextProcessingError {
    #[error("text processing failed: {0}")]
    Failed(String),
}

pub async fn process_transcription(
    raw: &str,
    settings: &AppSettings,
    http: &Client,
    llm_engine: &LlmEngine,
    whisper_detected_language: Option<&str>,
) -> Result<ProcessedText, TextProcessingError> {
    let dictionary = load_dictionary(settings.transcription_dictionary_path.as_deref())
        .unwrap_or_default();
    let terms = protected_terms(&dictionary);
    let postprocess_lang = settings.postprocess_language(whisper_detected_language);
    let (raw, press_enter) = prepare_transcription_for_processing(
        raw,
        settings,
        &dictionary,
        &postprocess_lang,
    );

    let mut rewrite_fallback = false;
    let mut rewrite_fallback_reason = None;
    let mut text = match settings.text_processing_mode {
        TextProcessingMode::Original => apply_original(&raw),
        TextProcessingMode::Basic => apply_basic_cleanup(&raw),
        mode if mode.uses_ai() => {
            let outcome = rewrite_transcription(
                &raw,
                mode,
                settings,
                settings.ai_rewrite_skill.as_deref(),
                http,
                llm_engine,
                &terms,
            )
            .await
            .map_err(|error| TextProcessingError::Failed(error.to_string()))?;
            rewrite_fallback = outcome.used_fallback;
            rewrite_fallback_reason = outcome.fallback_reason;
            apply_corrections(&outcome.text, &dictionary.corrections)
        }
        _ => apply_basic_cleanup(&raw),
    };

    if settings.numbers_as_words {
        text = apply_numbers_as_words(&text, true, &postprocess_lang);
    }

    text = ensure_spaces_after_punctuation(&text);

    Ok(ProcessedText {
        text,
        press_enter,
        rewrite_fallback,
        rewrite_fallback_reason,
    })
}

fn prepare_transcription_for_processing(
    raw: &str,
    settings: &AppSettings,
    dictionary: &crate::text::dictionary::Dictionary,
    postprocess_lang: &str,
) -> (String, bool) {
    // Strip known Whisper artifacts and apply user corrections in every mode.
    let raw = clean_raw_transcription_with_dictionary(raw, dictionary);

    // Remove the enter trigger phrase from raw Whisper text before any AI rewrite.
    // If it reaches the LLM, the model reformulates it and Enter emulation breaks.
    let (mut text, press_enter) = apply_enter_trigger(
        &raw,
        settings.emulate_enter,
        &settings.enter_trigger_phrase,
    );

    if settings.spoken_punctuation {
        text = apply_spoken_punctuation(&text, postprocess_lang);
    }

    (text, press_enter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;

    #[tokio::test]
    async fn original_mode_adds_trailing_space_after_period() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed =
            process_transcription("первое предложение.", &settings, &client, &llm, None)
                .await
                .unwrap();
        assert_eq!(processed.text, "первое предложение. ");
    }

    #[tokio::test]
    async fn original_mode_keeps_whitespace() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed =
            process_transcription("  hello   world  ", &settings, &client, &llm, None)
                .await
                .unwrap();
        assert_eq!(processed.text, "hello   world");
    }

    #[tokio::test]
    async fn basic_mode_cleans_text() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Basic,
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed =
            process_transcription("  hello   world  ", &settings, &client, &llm, None)
                .await
                .unwrap();
        assert_eq!(processed.text, "Hello world");
    }

    #[tokio::test]
    async fn german_spoken_punctuation_runs_before_cleanup() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Basic,
            spoken_punctuation: true,
            language: Some("de".to_string()),
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed =
            process_transcription("Hallo Punkt Welt", &settings, &client, &llm, Some("de"))
                .await
                .unwrap();
        assert_eq!(processed.text, "Hallo. Welt");
    }

    #[tokio::test]
    async fn spoken_punctuation_runs_before_cleanup() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Basic,
            spoken_punctuation: true,
            language: Some("ru".to_string()),
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed =
            process_transcription("привет запятая мир", &settings, &client, &llm, Some("ru"))
                .await
                .unwrap();
        assert_eq!(processed.text, "Привет, мир");
    }

    #[tokio::test]
    async fn enter_trigger_strips_suffix() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            emulate_enter: true,
            enter_trigger_phrase: "и строка".to_string(),
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed =
            process_transcription("текст и строка", &settings, &client, &llm, None)
                .await
                .unwrap();
        assert_eq!(processed.text, "текст");
        assert!(processed.press_enter);
    }

    #[tokio::test]
    async fn subtitle_hallucination_is_dropped_in_original_mode() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed = process_transcription(
            "Редактор субтитров: А. Синецкая. Корректор: А. Егорова.",
            &settings,
            &client,
            &llm,
            None,
        )
        .await
        .unwrap();
        assert!(processed.text.is_empty());
    }

    #[tokio::test]
    async fn numbers_as_words_use_whisper_language_when_auto() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Basic,
            numbers_as_words: true,
            language: None,
            ui_locale: crate::settings::UiLocale::En,
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed =
            process_transcription("1985 год", &settings, &client, &llm, Some("ru"))
                .await
                .unwrap();
        assert!(processed.text.contains("тысяч"));
        assert!(!processed.text.contains("1985"));
    }

    #[tokio::test]
    async fn enter_trigger_runs_before_text_processing() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Basic,
            emulate_enter: true,
            enter_trigger_phrase: "отправить сообщение".to_string(),
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed =
            process_transcription("раз два отправить сообщение", &settings, &client, &llm, None)
                .await
                .unwrap();
        assert!(processed.press_enter);
        assert!(
            !processed.text.to_lowercase().contains("отправ"),
            "trigger phrase must not appear in output: {}",
            processed.text
        );
    }

    #[test]
    fn prepare_transcription_strips_enter_trigger_before_downstream_processing() {
        let settings = AppSettings {
            emulate_enter: true,
            enter_trigger_phrase: "отправить сообщение".to_string(),
            ..Default::default()
        };
        let dictionary = load_dictionary(None).unwrap_or_default();
        let (raw, press_enter) = prepare_transcription_for_processing(
            "раз два отправить сообщение",
            &settings,
            &dictionary,
            "ru",
        );
        assert!(press_enter);
        assert_eq!(raw, "раз два");
    }
}
