use crate::text::corrections::apply_corrections;
use crate::text::dictionary::Dictionary;

/// Lightweight text cleanup without LLM involvement.
///
/// Remove known Whisper artifacts before any text-processing mode runs.
pub fn clean_raw_transcription(text: &str) -> String {
    let text = strip_whisper_hallucinations(text.trim());
    if is_whisper_hallucination_only(&text) || is_whisper_prompt_echo(&text) {
        return String::new();
    }
    collapse_speech_stutters(&text)
}

/// Drop transcriptions that mostly repeat the Whisper `initial_prompt` (common on silence).
pub fn strip_prompt_echo(text: &str, prompt: Option<&str>) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }

    if is_whisper_prompt_echo(text) {
        return String::new();
    }

    let Some(prompt) = prompt.map(str::trim).filter(|value| !value.is_empty()) else {
        return text.to_string();
    };

    let text_norm = alphanumeric_lower(text);
    let prompt_norm = alphanumeric_lower(prompt);
    if !text_norm.is_empty() && prompt_norm.contains(&text_norm) && is_likely_prompt_echo(text) {
        return String::new();
    }

    if transcription_words_match_prompt(text, prompt) && is_likely_prompt_echo(text) {
        return String::new();
    }

    text.to_string()
}

pub fn clean_raw_transcription_with_dictionary(text: &str, dictionary: &Dictionary) -> String {
    let text = clean_raw_transcription(text);
    apply_corrections(&text, &dictionary.corrections)
}

/// Collapse false restarts from speech-to-text, e.g.
/// "под каждую собиралась под каждую систему" -> "под каждую систему".
pub fn collapse_speech_stutters(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return text.to_string();
    }

    const MAX_PHRASE_WORDS: usize = 5;
    const MAX_GAP_WORDS: usize = 3;

    let mut result = Vec::new();
    let mut index = 0usize;
    let mut collapsed = false;

    while index < words.len() {
        let mut skipped = false;

        'scan: for phrase_len in (1..=MAX_PHRASE_WORDS.min(words.len() - index)).rev() {
            if index + phrase_len >= words.len() {
                continue;
            }

            let phrase = &words[index..index + phrase_len];
            // A lone word is a stutter only when repeated back to back ("мы мы"); with a gap it is
            // usually just a common word used twice ("под каждую … под каждый").
            let max_gap = if phrase_len == 1 { 0 } else { MAX_GAP_WORDS };
            let search_end = (index + phrase_len + max_gap).min(words.len().saturating_sub(phrase_len));

            for repeat_at in (index + phrase_len)..=search_end {
                if repeat_at + phrase_len <= words.len() && words[repeat_at..repeat_at + phrase_len] == *phrase
                {
                    index = repeat_at;
                    skipped = true;
                    break 'scan;
                }
            }
        }

        if skipped {
            collapsed = true;
            continue;
        }

        result.push(words[index]);
        index += 1;
    }

    if !collapsed {
        // Nothing to collapse: keep the caller's spacing (Original mode relies on it).
        return text.to_string();
    }
    result.join(" ")
}

pub fn apply_original(text: &str) -> String {
    text.trim().to_string()
}

/// Maximum sentences per auto-generated paragraph in Basic cleanup mode.
const BASIC_MAX_PARAGRAPH_SENTENCES: usize = 2;
/// Soft character limit per paragraph before forcing a break at the next sentence.
const BASIC_MAX_PARAGRAPH_CHARS: usize = 220;

pub fn apply_basic_cleanup(text: &str) -> String {
    let mut text = clean_raw_transcription(text);
    if text.is_empty() {
        return text;
    }

    text = collapse_whitespace(&text);
    text = ensure_spaces_after_punctuation(&text);
    text = wrap_into_readable_paragraphs(&text);
    capitalize_paragraphs(&text)
}

/// Split long dictation blocks into shorter paragraphs for readability.
pub fn wrap_into_readable_paragraphs(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    text.split("\n\n")
        .map(split_block_into_paragraphs)
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn split_block_into_paragraphs(block: &str) -> String {
    let block = block.trim_start();
    if block.trim().is_empty() {
        return String::new();
    }

    let trailing = trailing_whitespace(block);
    let content = block.trim_end();

    let sentences = split_sentences(content);
    if sentences.len() <= BASIC_MAX_PARAGRAPH_SENTENCES
        && content.chars().count() <= BASIC_MAX_PARAGRAPH_CHARS
    {
        return format!("{content}{trailing}");
    }

    let mut paragraphs = Vec::new();
    let mut current = Vec::new();
    let mut current_chars = 0usize;

    for sentence in sentences {
        let sentence_chars = sentence.chars().count();
        let needs_break = !current.is_empty()
            && (current.len() >= BASIC_MAX_PARAGRAPH_SENTENCES
                || current_chars.saturating_add(sentence_chars) > BASIC_MAX_PARAGRAPH_CHARS);
        if needs_break {
            paragraphs.push(join_sentences(&current));
            current.clear();
            current_chars = 0;
        }
        current_chars = current_chars.saturating_add(sentence_chars);
        if !current.is_empty() {
            current_chars += 1;
        }
        current.push(sentence);
    }

    if !current.is_empty() {
        paragraphs.push(join_sentences(&current));
    }

    let mut result = paragraphs.join("\n\n");
    result.push_str(&trailing);
    result
}

fn trailing_whitespace(text: &str) -> String {
    text.chars()
        .rev()
        .take_while(|ch| ch.is_whitespace())
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn join_sentences(sentences: &[String]) -> String {
    sentences.join(" ")
}

fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut sentences = Vec::new();
    let mut start = 0usize;

    for index in 0..chars.len() {
        let ch = chars[index];
        if !matches!(ch, '.' | '!' | '?' | '…') {
            continue;
        }

        if ch == '.' && is_decimal_point(&chars, index) {
            continue;
        }

        let mut end = index + 1;
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        let sentence: String = chars[start..index + 1].iter().collect();
        if !sentence.trim().is_empty() {
            sentences.push(sentence.trim().to_string());
        }
        start = end;
    }

    if start < chars.len() {
        let tail: String = chars[start..].iter().collect();
        if !tail.trim().is_empty() {
            sentences.push(tail.trim().to_string());
        }
    }

    if sentences.is_empty() {
        sentences.push(text.trim().to_string());
    }

    sentences
}

pub fn apply_spoken_punctuation(text: &str, lang: &str) -> String {
    crate::text::spoken_punctuation::apply_spoken_punctuation(text, lang)
}

/// Backward-compatible wrapper for tests.
pub fn normalize_transcription(
    raw: &str,
    cleanup_enabled: bool,
    spoken_punctuation: bool,
) -> String {
    let mut text = if cleanup_enabled {
        apply_basic_cleanup(raw)
    } else {
        let mut text = clean_raw_transcription(raw);
        text = ensure_spaces_after_punctuation(&text);
        text
    };

    if spoken_punctuation {
        text = apply_spoken_punctuation(&text, "ru");
    }

    text
}

/// Whisper often hallucinates TV subtitle credits on silence or low audio.
pub fn strip_whisper_hallucinations(text: &str) -> String {
    const MARKERS: &[&str] = &[
        "Редактор субтитров",
        "Субтитры создал",
        ". Корректор",
        "Корректор:",
        "Subtitles by",
        "Translated by",
        "Amara.org",
        "Thanks for watching",
        "Thank you for watching",
        "Please subscribe",
        "Продолжение следует",
        "Не забудьте подписаться",
        "Смотрите продолжение",
        "Субтитры:",
        "MBC",
        "www.",
    ];

    let lower = text.to_lowercase();
    let mut cut_at = text.len();
    for marker in MARKERS {
        if let Some(index) = lower.find(&marker.to_lowercase()) {
            cut_at = cut_at.min(index);
        }
    }

    text.get(..cut_at).unwrap_or(text).trim_end().to_string()
}

fn is_whisper_prompt_echo(text: &str) -> bool {
    let lower = text.to_lowercase();
    let trimmed = lower.trim().trim_end_matches('.').trim();

    if trimmed == "термины" || trimmed == "terms" {
        return true;
    }

    let ends_empty_terms = trimmed.ends_with("термины") || trimmed.ends_with("terms");
    let has_lang_prefix = lower.contains("русская")
        && (lower.contains("диктов") || lower.contains("диектов"))
        || lower.contains("english dictation");

    if ends_empty_terms && has_lang_prefix {
        return true;
    }

    if let Some(index) = lower.rfind("термины") {
        let after = lower[index + "термины".len()..]
            .trim()
            .trim_start_matches(':')
            .trim()
            .trim_end_matches('.');
        if after.is_empty() {
            let before = lower[..index].trim().trim_end_matches('.');
            if before.is_empty() || has_lang_prefix || before.contains("диктов") {
                return true;
            }
        }
    }

    false
}

/// Short utterances that repeat the initial prompt are usually silence hallucinations.
fn is_likely_prompt_echo(text: &str) -> bool {
    text.chars().count() <= 40 || text.split_whitespace().count() <= 4
}

fn transcription_words_match_prompt(text: &str, prompt: &str) -> bool {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|word| word.trim_matches(|ch: char| !ch.is_alphanumeric()).to_lowercase())
        .filter(|word| !word.is_empty())
        .collect();

    if words.is_empty() {
        return false;
    }

    let prompt_words: Vec<String> = prompt
        .split_whitespace()
        .map(|word| word.trim_matches(|ch: char| !ch.is_alphanumeric()).to_lowercase())
        .filter(|word| !word.is_empty())
        .collect();

    if prompt_words.is_empty() {
        return false;
    }

    words
        .iter()
        .all(|word| prompt_word_matches(word, &prompt_words))
}

fn prompt_word_matches(word: &str, prompt_words: &[String]) -> bool {
    prompt_words.iter().any(|candidate| {
        if word == candidate {
            return true;
        }
        word.len() >= 6 && candidate.len() >= 6 && words_within_one_edit(word, candidate)
    })
}

fn words_within_one_edit(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }

    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };

    if longer.len() - shorter.len() > 1 {
        return false;
    }

    let mut edits = 0usize;
    let mut short_index = 0usize;
    let mut long_index = 0usize;
    let short_chars: Vec<char> = shorter.chars().collect();
    let long_chars: Vec<char> = longer.chars().collect();

    while short_index < short_chars.len() && long_index < long_chars.len() {
        if short_chars[short_index] == long_chars[long_index] {
            short_index += 1;
            long_index += 1;
            continue;
        }

        edits += 1;
        if edits > 1 {
            return false;
        }

        if short_chars.len() == long_chars.len() {
            short_index += 1;
            long_index += 1;
        } else {
            long_index += 1;
        }
    }

    edits + (long_chars.len() - long_index) + (short_chars.len() - short_index) <= 1
}

fn is_whisper_hallucination_only(text: &str) -> bool {
    let normalized = alphanumeric_lower(text);
    if normalized.is_empty() {
        return true;
    }

    if is_whisper_prompt_echo(text) {
        return true;
    }

    const KNOWN_ONLY: &[&str] = &[
        "редакторсубтитровасинецкаякорректораегорова",
        "редакторсубтитровасинецкаякорректоракуликова",
        "субтитрысоздалdimatorzokov",
        "subtitlesby",
        "translatedby",
        "amaradotorg",
        "thanksforwatching",
        "thankyouforwatching",
        "pleasesubscribe",
        "продолжениеследует",
        "незабудьтеподписаться",
        "смотритепродолжение",
    ];

    KNOWN_ONLY.iter().any(|pattern| normalized == *pattern)
}

fn alphanumeric_lower(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

/// Adds a trailing space so the next injected dictation block does not glue on.
pub fn ensure_trailing_block_separator(text: &str) -> String {
    if text.is_empty() || text.ends_with(char::is_whitespace) {
        return text.to_string();
    }

    let mut result = text.to_string();
    result.push(' ');
    result
}

pub fn ensure_spaces_after_punctuation(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len() + 4);

    for (index, ch) in chars.iter().copied().enumerate() {
        result.push(ch);
        if should_add_space_after(&chars, index) {
            result.push(' ');
        }
    }

    result
}

fn should_add_space_after(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    let next = chars.get(index + 1).copied();

    if next.is_some_and(|next| next.is_whitespace()) {
        return false;
    }

    match ch {
        ',' | '!' | '?' | ';' => true,
        ':' => !is_time_colon(chars, index),
        // Include terminal punctuation so the next recognition block does not glue on.
        '.' => !is_decimal_point(chars, index),
        _ => false,
    }
}

fn is_decimal_point(chars: &[char], index: usize) -> bool {
    chars[index] == '.'
        && chars
            .get(index.wrapping_sub(1))
            .is_some_and(|ch| ch.is_ascii_digit())
        && chars.get(index + 1).is_some_and(|ch| ch.is_ascii_digit())
}

fn is_time_colon(chars: &[char], index: usize) -> bool {
    chars[index] == ':'
        && chars
            .get(index.wrapping_sub(1))
            .is_some_and(|ch| ch.is_ascii_digit())
        && chars.get(index + 1).is_some_and(|ch| ch.is_ascii_digit())
}

fn collapse_whitespace(text: &str) -> String {
    text.split("\n\n")
        .map(|block| block.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn capitalize_paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(|paragraph| capitalize_first_letter(paragraph.trim_start()))
        .filter(|paragraph| !paragraph.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn capitalize_first_letter(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoken_punctuation_is_replaced() {
        let text = normalize_transcription("привет запятая как дела вопросительный знак", false, true);
        assert_eq!(text, "привет, как дела?");
    }

    #[test]
    fn cleanup_collapses_whitespace_and_capitalizes() {
        let text = normalize_transcription("  hello   world  ", true, false);
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn strips_whisper_subtitle_hallucination() {
        let raw = "привет мир Редактор субтитров А.Синецкая Корректор А.Егорова";
        let text = normalize_transcription(raw, false, false);
        assert_eq!(text, "привет мир");
    }

    #[test]
    fn drops_subtitle_hallucination_only() {
        let raw = "Редактор субтитров: А. Синецкая. Корректор: А. Егорова.";
        assert!(clean_raw_transcription(raw).is_empty());
    }

    #[test]
    fn strips_subtitle_hallucination_case_insensitive() {
        let raw = "hello редактор субтитров: test";
        assert_eq!(clean_raw_transcription(raw), "hello");
    }

    #[test]
    fn adds_space_after_punctuation() {
        let text = normalize_transcription("привет,как дела?", false, false);
        assert_eq!(text, "привет, как дела? ");
    }

    #[test]
    fn adds_trailing_space_after_terminal_period() {
        assert_eq!(
            ensure_spaces_after_punctuation("Первое предложение."),
            "Первое предложение. "
        );
    }

    #[test]
    fn basic_cleanup_keeps_trailing_space_after_period() {
        assert_eq!(
            apply_basic_cleanup("первое предложение."),
            "Первое предложение. "
        );
    }

    #[test]
    fn trailing_block_separator_adds_space_without_punctuation() {
        assert_eq!(
            ensure_trailing_block_separator("первый блок"),
            "первый блок "
        );
    }

    #[test]
    fn trailing_block_separator_keeps_existing_whitespace() {
        assert_eq!(ensure_trailing_block_separator("уже есть "), "уже есть ");
    }

    #[test]
    fn keeps_decimal_and_time_without_extra_spaces() {
        let text = normalize_transcription("версия 3.14 в 12:30", false, false);
        assert_eq!(text, "версия 3.14 в 12:30");
    }

    #[test]
    fn original_mode_skips_cleanup() {
        let text = apply_original("  hello   world  ");
        assert_eq!(text, "hello   world");
    }

    #[test]
    fn basic_cleanup_without_spoken_punctuation() {
        let text = apply_basic_cleanup("  hello   world  ");
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn basic_cleanup_splits_long_text_into_paragraphs() {
        let raw = "Первый блок текста короткий. Второй блок уже подлиннее. \
            Третий блок продолжает мысль. Четвёртый блок завершает фразу.";
        let text = apply_basic_cleanup(raw);
        assert!(text.contains("\n\n"), "expected paragraphs, got: {text:?}");
        assert!(!text.starts_with("\n\n"));
    }

    #[test]
    fn wrap_into_readable_paragraphs_splits_by_sentence_count() {
        let text = wrap_into_readable_paragraphs("One. Two. Three. Four.");
        assert_eq!(text, "One. Two.\n\nThree. Four.");
    }

    #[test]
    fn basic_cleanup_keeps_short_text_as_single_paragraph() {
        let text = apply_basic_cleanup("Короткая фраза. И ещё одна.");
        assert!(!text.contains('\n'));
    }

    #[test]
    fn basic_cleanup_preserves_existing_paragraph_breaks() {
        let text = apply_basic_cleanup("Первый абзац.\n\nВторой абзац.");
        assert_eq!(text.matches("\n\n").count(), 1);
    }

    #[test]
    fn split_sentences_keeps_decimal_points() {
        let sentences = split_sentences("версия 3.14 стабильна. релиз завтра.");
        assert_eq!(sentences.len(), 2);
        assert!(sentences[0].contains("3.14"));
    }

    #[test]
    fn collapses_phrase_level_stutter() {
        let raw = "чтобы под каждую собиралась под каждую систему по-своему";
        assert_eq!(
            collapse_speech_stutters(raw),
            "чтобы под каждую систему по-своему"
        );
    }

    #[test]
    fn collapses_single_word_stutter() {
        assert_eq!(collapse_speech_stutters("мы мы пойдём"), "мы пойдём");
    }

    #[test]
    fn leaves_intentional_distinct_phrases() {
        let raw = "сначала под каждую систему потом под каждый сервис";
        assert_eq!(collapse_speech_stutters(raw), raw);
    }

    #[test]
    fn drops_whisper_prompt_echo_with_empty_terms() {
        let raw = "Русская диектовка. Неформальный разговор Термины.";
        assert!(clean_raw_transcription(raw).is_empty());
    }

    #[test]
    fn drops_transcription_that_repeats_initial_prompt() {
        let prompt = "Неформальный разговор. Veyro, React, послать.";
        let raw = "Неформальный разговор Veyro React";
        assert!(strip_prompt_echo(raw, Some(prompt)).is_empty());
    }

    #[test]
    fn keeps_real_speech_that_only_shares_prompt_words() {
        let prompt = "Неформальный разговор. Veyro, React.";
        let raw = "Привет, это неформальный разговор с другом.";
        assert_eq!(
            strip_prompt_echo(raw, Some(prompt)),
            "Привет, это неформальный разговор с другом."
        );
    }

    #[test]
    fn keeps_longer_speech_that_contains_prompt_substring() {
        let prompt = "Деловой стиль. Veyro, React.";
        let raw = "Деловой стиль общения с клиентом сегодня был очень формальным.";
        assert_eq!(strip_prompt_echo(raw, Some(prompt)), raw);
    }

    #[test]
    fn drops_legacy_language_prefix_echo() {
        let raw = "Русская диктовка. Деловой стиль. Термины:";
        assert!(clean_raw_transcription(raw).is_empty());
    }
}
