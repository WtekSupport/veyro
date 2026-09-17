use crate::settings::AppSettings;

const MAX_PROMPT_CHARS: usize = 800;
const MAX_CONTEXT_CHARS: usize = 200;

#[derive(Debug, Clone)]
pub struct WhisperPromptInput<'a> {
    pub settings: &'a AppSettings,
    pub vocabulary: &'a [String],
    pub previous_text: Option<&'a str>,
}

pub fn build_whisper_prompt(input: &WhisperPromptInput<'_>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    // Language is set separately via Whisper params; meta labels like "Русская диктовка."
    // are echoed back on silence and are not useful as initial_prompt text.

    let custom = input.settings.whisper_prompt_prefix.trim();
    if !custom.is_empty() {
        parts.push(custom.to_string());
    }

    let mut terms: Vec<String> = input
        .vocabulary
        .iter()
        .map(|term| term.trim().to_string())
        .filter(|term| !term.is_empty())
        .collect();

    if input.settings.emulate_enter {
        let trigger = input.settings.enter_trigger_phrase.trim();
        if !trigger.is_empty() && !terms.iter().any(|term| term.eq_ignore_ascii_case(trigger)) {
            terms.push(trigger.to_string());
        }
    }

    if !terms.is_empty() {
        parts.push(terms.join(", "));
    }

    if let Some(previous) = input.previous_text {
        let trimmed = previous.trim();
        if !trimmed.is_empty() {
            let context = tail_chars(trimmed, MAX_CONTEXT_CHARS);
            parts.push(context);
        }
    }

    let prompt = parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return None;
    }

    Some(tail_chars(&prompt, MAX_PROMPT_CHARS))
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    text.chars()
        .skip(char_count - max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;

    #[test]
    fn builds_prompt_with_language_vocabulary_and_context() {
        let settings = AppSettings {
            language: Some("ru".to_string()),
            whisper_prompt_prefix: "Деловой стиль.".to_string(),
            emulate_enter: true,
            enter_trigger_phrase: "послать".to_string(),
            ..Default::default()
        };
        let vocabulary = vec!["Veyro".to_string(), "React".to_string()];
        let prompt = build_whisper_prompt(&WhisperPromptInput {
            settings: &settings,
            vocabulary: &vocabulary,
            previous_text: Some("Предыдущее предложение для контекста."),
        })
        .expect("prompt");

        assert!(!prompt.contains("Русская диктовка."));
        assert!(!prompt.contains("Термины:"));
        assert!(prompt.contains("Деловой стиль."));
        assert!(prompt.contains("Veyro"));
        assert!(prompt.contains("послать"));
        assert!(prompt.contains("контекста."));
    }

    #[test]
    fn returns_none_for_empty_prompt() {
        let settings = AppSettings::default();
        assert!(build_whisper_prompt(&WhisperPromptInput {
            settings: &settings,
            vocabulary: &[],
            previous_text: None,
        })
        .is_none());
    }

    #[test]
    fn truncates_long_prompt_to_tail() {
        let settings = AppSettings {
            whisper_prompt_prefix: "x".repeat(900),
            ..Default::default()
        };
        let prompt = build_whisper_prompt(&WhisperPromptInput {
            settings: &settings,
            vocabulary: &[],
            previous_text: None,
        })
        .expect("prompt");
        assert!(prompt.chars().count() <= MAX_PROMPT_CHARS);
    }
}
