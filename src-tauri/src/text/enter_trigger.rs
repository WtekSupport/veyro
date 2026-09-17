const DEFAULT_TRIGGERS: &[&str] = &["и строка", "новая строка", "new line", "and line"];

/// Extra words STT may insert immediately before a garbled trigger suffix.
const MAX_STT_GARBAGE_WORDS: usize = 3;
/// Minimum compact trigger length to allow fuzzy suffix matching.
const MIN_FUZZY_TRIGGER_LEN: usize = 4;
/// Levenshtein similarity high enough to match without STT garbage before the tail.
const FUZZY_SIMILARITY_STANDALONE: f64 = 0.85;
/// Levenshtein similarity when STT inserted garbage words before the garbled tail.
const FUZZY_SIMILARITY_WITH_GARBAGE: f64 = 0.72;

pub fn apply_enter_trigger(text: &str, enabled: bool, custom_phrase: &str) -> (String, bool) {
    if !enabled {
        return (text.to_string(), false);
    }

    let trimmed = text.trim_end();
    let triggers = active_enter_triggers(custom_phrase);

    for trigger in triggers {
        if let Some(cut) = strip_suffix_trigger(trimmed, &trigger) {
            return (cut, true);
        }
        if let Some(cut) = strip_suffix_trigger_fuzzy(trimmed, &trigger) {
            return (cut, true);
        }
    }

    (text.to_string(), false)
}

pub fn active_enter_triggers(custom_phrase: &str) -> Vec<String> {
    let custom = custom_phrase.trim();
    if custom.is_empty() {
        return DEFAULT_TRIGGERS
            .iter()
            .map(|trigger| trigger.to_lowercase())
            .collect();
    }

    vec![custom.to_lowercase()]
}

fn strip_suffix_trigger(text: &str, trigger: &str) -> Option<String> {
    let without_trailing_punct = trim_trailing_punctuation(text);
    let text_words = normalized_words(without_trailing_punct);
    let trigger_words = normalized_words(trigger);

    if trigger_words.is_empty() || text_words.len() < trigger_words.len() {
        return None;
    }

    let suffix_start = text_words.len() - trigger_words.len();
    if text_words[suffix_start..] != trigger_words[..] {
        return None;
    }

    Some(cut_original_words(without_trailing_punct, trigger_words.len()))
}

/// Fuzzy suffix match for the user's trigger phrase after STT distortion.
///
/// Requires either a high similarity score, or a tail that matches the trigger
/// with extra garbage words before it (typical Whisper split/insertion errors).
fn strip_suffix_trigger_fuzzy(text: &str, trigger: &str) -> Option<String> {
    let without_trailing_punct = trim_trailing_punctuation(text);
    let text_words = normalized_words(without_trailing_punct);
    let trigger_words = normalized_words(trigger);

    if trigger_words.is_empty() || text_words.is_empty() {
        return None;
    }

    let trigger_compact = compact_join(&trigger_words);
    if trigger_compact.chars().count() < MIN_FUZZY_TRIGGER_LEN {
        return None;
    }

    let min_suffix_words = trigger_words.len();
    let max_suffix_words = (trigger_words.len() + MAX_STT_GARBAGE_WORDS).min(text_words.len());

    for suffix_len in min_suffix_words..=max_suffix_words {
        if suffix_len >= text_words.len() {
            continue;
        }
        let suffix_start = text_words.len() - suffix_len;
        let suffix = &text_words[suffix_start..];

        if fuzzy_suffix_matches_trigger(&trigger_compact, suffix, trigger_words.len()) {
            let cut = cut_original_words(without_trailing_punct, suffix_len);
            if cut.is_empty() {
                continue;
            }
            return Some(cut);
        }
    }

    None
}

fn fuzzy_suffix_matches_trigger(
    trigger_compact: &str,
    suffix_words: &[String],
    trigger_word_count: usize,
) -> bool {
    let max_tail_words = suffix_words.len().min(trigger_word_count + 1);

    for tail_len in (1..=max_tail_words).rev() {
        let tail_start = suffix_words.len() - tail_len;
        let tail = &suffix_words[tail_start..];
        let candidate = compact_join(tail);

        if candidate.is_empty() {
            continue;
        }

        let garbage_words = suffix_words.len() - tail_len;
        let candidate_len = candidate.chars().count();
        let trigger_len = trigger_compact.chars().count();
        let ends_with_match = trigger_compact.ends_with(&candidate)
            && candidate_len * 5 >= trigger_len * 4;
        let similarity = levenshtein_similarity(&candidate, trigger_compact);

        if similarity >= FUZZY_SIMILARITY_STANDALONE {
            return true;
        }

        if garbage_words > 0
            && (ends_with_match || similarity >= FUZZY_SIMILARITY_WITH_GARBAGE)
            && candidate_len >= MIN_FUZZY_TRIGGER_LEN.saturating_sub(2)
        {
            return true;
        }

    }

    false
}

fn cut_original_words(without_trailing_punct: &str, words_to_strip: usize) -> String {
    let orig_words: Vec<&str> = without_trailing_punct.split_whitespace().collect();
    let cut_len = orig_words.len().saturating_sub(words_to_strip);
    let cut = orig_words[..cut_len].join(" ");
    trim_trailing_punctuation(&cut)
        .trim_end_matches(',')
        .trim_end()
        .to_string()
}

fn compact_join(words: &[String]) -> String {
    words
        .iter()
        .flat_map(|word| word.chars().filter(|ch| ch.is_alphanumeric()))
        .collect::<String>()
        .to_lowercase()
}

fn levenshtein_similarity(left: &str, right: &str) -> f64 {
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    if left_chars.is_empty() && right_chars.is_empty() {
        return 1.0;
    }

    let distance = levenshtein_distance(&left_chars, &right_chars);
    let max_len = left_chars.len().max(right_chars.len());
    1.0 - (distance as f64 / max_len as f64)
}

fn levenshtein_distance(left: &[char], right: &[char]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let mut prev: Vec<usize> = (0..=right.len()).collect();
    let mut curr = vec![0; right.len() + 1];

    for (i, left_ch) in left.iter().enumerate() {
        curr[0] = i + 1;
        for (j, right_ch) in right.iter().enumerate() {
            let cost = usize::from(left_ch != right_ch);
            curr[j + 1] = (curr[j] + 1)
                .min(prev[j + 1] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[right.len()]
}

fn normalized_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            trim_trailing_punctuation(word)
                .trim_matches(|ch: char| matches!(ch, '"' | '«' | '»' | '\''))
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

fn trim_trailing_punctuation(text: &str) -> &str {
    text.trim_end_matches(['.', ',', '!', '?', ';', ':', '…'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_default_russian_trigger() {
        let (text, press) = apply_enter_trigger("текст и строка", true, "");
        assert_eq!(text, "текст");
        assert!(press);
    }

    #[test]
    fn strips_custom_phrase_case_insensitive() {
        let (text, press) = apply_enter_trigger("Hello Enter Now", true, "enter now");
        assert_eq!(text, "Hello");
        assert!(press);
    }

    #[test]
    fn custom_phrase_overrides_defaults() {
        let (text, press) = apply_enter_trigger("текст и строка", true, "enter now");
        assert_eq!(text, "текст и строка");
        assert!(!press);
    }

    #[test]
    fn ignores_trigger_in_middle() {
        let (text, press) = apply_enter_trigger("новая строка в середине", true, "");
        assert_eq!(text, "новая строка в середине");
        assert!(!press);
    }

    #[test]
    fn disabled_leaves_text_unchanged() {
        let (text, press) = apply_enter_trigger("текст и строка", false, "новая строка");
        assert_eq!(text, "текст и строка");
        assert!(!press);
    }

    #[test]
    fn strips_trigger_with_trailing_period() {
        let (text, press) =
            apply_enter_trigger("раз два отправить сообщение.", true, "отправить сообщение");
        assert_eq!(text, "раз два");
        assert!(press);
    }

    #[test]
    fn strips_trigger_with_comma_before_phrase() {
        let (text, press) =
            apply_enter_trigger("раз два, отправить сообщение", true, "отправить сообщение");
        assert_eq!(text, "раз два");
        assert!(press);
    }

    #[test]
    fn strips_trigger_with_extra_whitespace() {
        let (text, press) =
            apply_enter_trigger("раз два   отправить   сообщение", true, "отправить сообщение");
        assert_eq!(text, "раз два");
        assert!(press);
    }

    #[test]
    fn lists_default_triggers_when_custom_empty() {
        assert_eq!(
            active_enter_triggers(""),
            vec![
                "и строка".to_string(),
                "новая строка".to_string(),
                "new line".to_string(),
                "and line".to_string(),
            ]
        );
    }

    #[test]
    fn fuzzy_matches_stt_garbled_custom_trigger() {
        let (text, press) = apply_enter_trigger(
            "раз два tri послать",
            true,
            "послать",
        );
        assert_eq!(text, "раз два tri");
        assert!(press);
    }

    #[test]
    fn fuzzy_does_not_match_similar_word_without_stt_garbage() {
        let (text, press) = apply_enter_trigger("скажи ему слать", true, "послать");
        assert_eq!(text, "скажи ему слать");
        assert!(!press);
    }

    #[test]
    fn fuzzy_matches_high_similarity_without_garbage() {
        let (text, press) = apply_enter_trigger(
            "раз два отправить сообщен",
            true,
            "отправить сообщение",
        );
        assert_eq!(text, "раз два");
        assert!(press);
    }

    #[test]
    fn fuzzy_respects_user_phrase_not_hardcoded_aliases() {
        let (text, press) = apply_enter_trigger(
            "заметка, число с лать",
            true,
            "отправить отчёт",
        );
        assert_eq!(text, "заметка, число с лать");
        assert!(!press);
    }
}
