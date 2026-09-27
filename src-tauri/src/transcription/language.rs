use crate::settings::AppSettings;

/// Post-STT fallback when transcription language is auto and Whisper did not report a code.
pub const STT_AUTO_FALLBACK_LANG: &str = "en";

/// Language passed to Whisper / OpenAI STT: fixed setting only; `None` means auto-detect per request.
pub fn whisper_transcription_language(settings: &AppSettings) -> Option<String> {
    settings.language.clone()
}

/// Language for text cleanup after STT (numbers-as-words, Silero TE, etc.).
pub fn postprocess_language(
    settings: &AppSettings,
    whisper_detected: Option<&str>,
    session_detected_hint: Option<&str>,
) -> String {
    if let Some(language) = &settings.language {
        return language.clone();
    }
    whisper_detected
        .or(session_detected_hint)
        .map(normalize_stt_language_code)
        .unwrap_or_else(|| STT_AUTO_FALLBACK_LANG.to_string())
}

/// Soft hint for Whisper `initial_prompt` context filtering — never forces decode language.
pub fn whisper_prompt_sticky_language<'a>(
    settings: &'a AppSettings,
    session_detected_hint: Option<&'a str>,
) -> Option<&'a str> {
    settings.language.as_deref().or(session_detected_hint)
}

pub fn normalize_stt_language_code(code: &str) -> String {
    code.split('-')
        .next()
        .unwrap_or(code)
        .trim()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AppSettings, UiLocale};

    #[test]
    fn whisper_uses_fixed_setting_only() {
        let fixed = AppSettings {
            language: Some("de".to_string()),
            ..Default::default()
        };
        assert_eq!(
            whisper_transcription_language(&fixed).as_deref(),
            Some("de")
        );

        let auto = AppSettings {
            language: None,
            ui_locale: UiLocale::Ru,
            ..Default::default()
        };
        assert!(whisper_transcription_language(&auto).is_none());
    }

    #[test]
    fn postprocess_auto_prefers_detection_then_en_not_ui_locale() {
        let auto_ru_ui = AppSettings {
            language: None,
            ui_locale: UiLocale::Ru,
            ..Default::default()
        };
        assert_eq!(
            postprocess_language(&auto_ru_ui, Some("fr"), None),
            "fr"
        );
        assert_eq!(
            postprocess_language(&auto_ru_ui, None, Some("uk")),
            "uk"
        );
        assert_eq!(postprocess_language(&auto_ru_ui, None, None), "en");
    }

    #[test]
    fn postprocess_fixed_setting_always_wins() {
        let settings = AppSettings {
            language: Some("ru".to_string()),
            ..Default::default()
        };
        assert_eq!(
            postprocess_language(&settings, Some("en"), Some("de")),
            "ru"
        );
    }

    #[test]
    fn prompt_sticky_uses_fixed_or_session_hint() {
        let fixed = AppSettings {
            language: Some("ru".to_string()),
            ..Default::default()
        };
        assert_eq!(
            whisper_prompt_sticky_language(&fixed, Some("en")),
            Some("ru")
        );

        let auto = AppSettings::default();
        assert_eq!(
            whisper_prompt_sticky_language(&auto, Some("en")),
            Some("en")
        );
        assert!(whisper_prompt_sticky_language(&auto, None).is_none());
    }
}
