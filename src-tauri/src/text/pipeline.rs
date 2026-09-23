use reqwest::Client;

use crate::llm::LlmEngine;
use crate::settings::{AppSettings, TextProcessingMode};
use crate::text::corrections::apply_corrections;
use crate::text::dictionary::{protected_terms, Dictionary};
use crate::text::enter_trigger::apply_enter_trigger;
use crate::text::normalize::{
    apply_basic_cleanup, apply_optimization_paragraphs, apply_original, apply_spoken_punctuation,
    clean_raw_transcription, clean_raw_transcription_with_dictionary,
    ensure_spaces_after_punctuation,
    ensure_trailing_space_after_terminal_punctuation,
};
use crate::text::numbers::apply_numbers_as_words;
use crate::text::ru_numeral_genitive::apply_russian_numeral_inflection;
use crate::text::silero_te::{apply_if_enabled, should_apply_silero_te};
use crate::text::rewrite::rewrite_transcription;
use crate::timed_text::TimedTextSegment;

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
    timed_segments: Option<&[TimedTextSegment]>,
    settings: &AppSettings,
    http: &Client,
    llm_engine: &LlmEngine,
    whisper_detected_language: Option<&str>,
) -> Result<ProcessedText, TextProcessingError> {
    if settings.ai_postprocess_mode().is_some() {
        return process_transcription_immediate(
            raw,
            timed_segments,
            settings,
            whisper_detected_language,
        )
        .await;
    }

    let immediate = process_transcription_immediate(
        raw,
        timed_segments,
        settings,
        whisper_detected_language,
    )
    .await?;

    let processing_mode = settings.effective_text_processing_mode();
    if !processing_mode.uses_ai() || !settings.push_to_talk {
        return Ok(immediate);
    }

    let dictionary = crate::text::dictionary::load_dictionary_for_settings(settings)
        .unwrap_or_default();
    let terms = protected_terms(&dictionary);

    rewrite_processed_text(
        &immediate.text,
        processing_mode,
        settings,
        settings.ai_rewrite_skill.as_deref(),
        http,
        llm_engine,
        &terms,
        &dictionary,
        immediate.press_enter,
    )
    .await
}

/// Phase 1: transcription cleanup without AI rewrite (sync — safe from preview worker threads).
pub fn process_transcription_immediate_sync(
    raw: &str,
    timed_segments: Option<&[TimedTextSegment]>,
    settings: &AppSettings,
    whisper_detected_language: Option<&str>,
) -> Result<ProcessedText, TextProcessingError> {
    let dictionary = crate::text::dictionary::load_dictionary_for_settings(settings)
        .unwrap_or_default();
    let postprocess_lang = settings.postprocess_language(whisper_detected_language);

    let stripped = clean_raw_transcription(raw);
    let source_text = if should_apply_silero_te(settings) {
        apply_if_enabled(&stripped, settings, &postprocess_lang)
    } else {
        stripped
    };
    let _ = timed_segments;

    let (raw, press_enter) = prepare_transcription_for_processing(
        &source_text,
        settings,
        &dictionary,
        &postprocess_lang,
    );

    let processing_mode = settings.immediate_transcription_mode();
    let mut text = apply_mode_cleanup(&raw, processing_mode);

    if settings.numbers_as_words {
        text = apply_numbers_as_words(&text, true, &postprocess_lang);
        if postprocess_lang.starts_with("ru") {
            text = apply_russian_numeral_inflection(&text);
        }
    }

    text = ensure_spaces_after_punctuation(&text);
    if processing_mode != TextProcessingMode::Original {
        text = crate::text::basic_cleanup::collapse_orphan_dot_artifacts(&text);
    }
    text = ensure_trailing_space_after_terminal_punctuation(&text);

    Ok(ProcessedText {
        text,
        press_enter,
        rewrite_fallback: false,
        rewrite_fallback_reason: None,
    })
}

/// Phase 1: transcription cleanup without AI rewrite.
pub async fn process_transcription_immediate(
    raw: &str,
    timed_segments: Option<&[TimedTextSegment]>,
    settings: &AppSettings,
    whisper_detected_language: Option<&str>,
) -> Result<ProcessedText, TextProcessingError> {
    let raw = raw.to_string();
    let settings = settings.clone();
    let whisper_lang = whisper_detected_language.map(str::to_string);
    let timed_owned: Option<Vec<TimedTextSegment>> =
        timed_segments.map(|segments| segments.to_vec());

    tokio::task::spawn_blocking(move || {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            process_transcription_immediate_sync(
                &raw,
                timed_owned.as_deref(),
                &settings,
                whisper_lang.as_deref(),
            )
        })) {
            Ok(result) => result,
            Err(_) => Err(TextProcessingError::Failed(
                "text processing panicked".to_string(),
            )),
        }
    })
    .await
    .map_err(|error| TextProcessingError::Failed(error.to_string()))?
}

/// Phase 2: AI rewrite on text that already went through phase 1.
#[allow(clippy::too_many_arguments)]
pub async fn rewrite_processed_text(
    light_text: &str,
    mode: TextProcessingMode,
    settings: &AppSettings,
    ai_rewrite_skill: Option<&str>,
    http: &Client,
    llm_engine: &LlmEngine,
    protected: &[String],
    dictionary: &Dictionary,
    press_enter: bool,
) -> Result<ProcessedText, TextProcessingError> {
    let outcome = rewrite_transcription(
        light_text,
        mode,
        settings,
        ai_rewrite_skill,
        http,
        llm_engine,
        protected,
    )
    .await
    .map_err(|error| TextProcessingError::Failed(error.to_string()))?;

    let mut text = apply_corrections(&outcome.text, &dictionary.corrections);
    text = ensure_spaces_after_punctuation(&text);
    text = crate::text::basic_cleanup::collapse_orphan_dot_artifacts(&text);
    text = ensure_spaces_after_punctuation(&text);
    if mode == TextProcessingMode::Optimization {
        text = apply_optimization_paragraphs(&text);
    }

    Ok(ProcessedText {
        text,
        press_enter,
        rewrite_fallback: outcome.used_fallback,
        rewrite_fallback_reason: outcome.fallback_reason,
    })
}

fn apply_mode_cleanup(raw: &str, processing_mode: TextProcessingMode) -> String {
    match processing_mode {
        TextProcessingMode::Original => apply_original(raw),
        TextProcessingMode::Basic => apply_basic_cleanup(raw),
        mode if mode.uses_ai() => apply_basic_cleanup(raw),
        _ => apply_basic_cleanup(raw),
    }
}

fn prepare_transcription_for_processing(
    raw: &str,
    settings: &AppSettings,
    dictionary: &Dictionary,
    postprocess_lang: &str,
) -> (String, bool) {
    let raw = clean_raw_transcription_with_dictionary(raw, dictionary);

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
    use crate::text::dictionary::load_dictionary;
    use crate::timed_text::TimedTextSegment;

    fn seg(text: &str, start: u64, end: u64) -> TimedTextSegment {
        TimedTextSegment {
            text: text.to_string(),
            start_ms: start,
            end_ms: end,
        }
    }

    async fn process(
        raw: &str,
        settings: &AppSettings,
        lang: Option<&str>,
    ) -> ProcessedText {
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        process_transcription(raw, None, settings, &client, &llm, lang)
            .await
            .unwrap()
    }

    async fn process_with_segments(
        raw: &str,
        segments: &[TimedTextSegment],
        settings: &AppSettings,
    ) -> ProcessedText {
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        process_transcription(raw, Some(segments), settings, &client, &llm, Some("ru"))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn original_mode_adds_trailing_space_after_period() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ui_mode: crate::settings::UiMode::Expert,
            ..Default::default()
        };
        let processed = process("первое предложение.", &settings, None).await;
        assert_eq!(processed.text, "первое предложение. ");
    }

    #[tokio::test]
    async fn original_mode_keeps_whitespace() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ui_mode: crate::settings::UiMode::Expert,
            ..Default::default()
        };
        let processed = process("  hello   world  ", &settings, None).await;
        assert_eq!(processed.text, "hello   world");
    }

    #[tokio::test]
    async fn basic_mode_cleans_text() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Basic,
            ..Default::default()
        };
        let processed = process("  hello   world  ", &settings, None).await;
        assert_eq!(processed.text, "Hello world");
    }

    #[tokio::test]
    async fn homemaker_original_runs_basic_cleanup() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ui_mode: crate::settings::UiMode::Homemaker,
            ..Default::default()
        };
        let processed = process("  hello   world  ", &settings, None).await;
        assert_eq!(processed.text, "Hello world");
    }

    #[tokio::test]
    async fn expert_original_skips_basic_cleanup() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ui_mode: crate::settings::UiMode::Expert,
            ..Default::default()
        };
        let processed = process("  hello   world  ", &settings, None).await;
        assert_eq!(processed.text, "hello   world");
    }

    #[tokio::test]
    async fn ptt_ai_mode_phase1_skips_inline_rewrite() {
        let settings = AppSettings {
            push_to_talk: true,
            text_processing_mode: TextProcessingMode::Optimization,
            ui_mode: crate::settings::UiMode::Expert,
            ..Default::default()
        };
        let processed = process("  hello   world  ", &settings, None).await;
        assert_eq!(processed.text, "Hello world");
        assert!(!processed.rewrite_fallback);
    }

    #[tokio::test]
    async fn german_spoken_punctuation_runs_before_cleanup() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Basic,
            spoken_punctuation: true,
            language: Some("de".to_string()),
            ..Default::default()
        };
        let processed = process("Hallo Punkt Welt", &settings, Some("de")).await;
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
        let processed = process("привет запятая мир", &settings, Some("ru")).await;
        assert_eq!(processed.text, "Привет, мир");
    }

    #[tokio::test]
    async fn silero_te_skipped_in_optimization_mode() {
        let settings = AppSettings {
            push_to_talk: true,
            text_processing_mode: TextProcessingMode::Optimization,
            transcription_provider: "local".to_string(),
            silero_te: true,
            ..Default::default()
        };
        let segments = [
            seg("первое", 0, 400),
            seg("второе", 1400, 1800),
        ];
        let processed = process_with_segments("первое второе", &segments, &settings).await;
        assert_eq!(processed.text, "Первое второе");
    }

    #[tokio::test]
    async fn enter_trigger_strips_suffix() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ui_mode: crate::settings::UiMode::Expert,
            emulate_enter: true,
            enter_trigger_phrase: "и строка".to_string(),
            ..Default::default()
        };
        let processed = process("текст и строка", &settings, None).await;
        assert_eq!(processed.text, "текст");
        assert!(processed.press_enter);
    }

    #[tokio::test]
    async fn subtitle_hallucination_is_dropped_in_original_mode() {
        let settings = AppSettings {
            text_processing_mode: TextProcessingMode::Original,
            ui_mode: crate::settings::UiMode::Expert,
            ..Default::default()
        };
        let client = Client::new();
        let llm = crate::llm::LlmEngine::unloaded(None);
        let processed = process_transcription(
            "Редактор субтитров: А. Синецкая. Корректор: А. Егорова.",
            None,
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
        let processed = process("1985 год", &settings, Some("ru")).await;
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
        let processed = process("раз два отправить сообщение", &settings, None).await;
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
