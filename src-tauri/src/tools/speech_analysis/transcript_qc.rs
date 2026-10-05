//! Transcript quality checks (chunk-merge artifacts, duplicate n-grams, suspicious tokens).

const MIN_DUPLICATE_NGRAM_WORDS: usize = 4;

#[derive(Debug, Clone, Default)]
pub struct TranscriptQualityReport {
    pub duplicate_ngram_count: usize,
    pub duplicate_ngram_examples: Vec<String>,
    pub suspicious_word_count: usize,
    pub degraded: bool,
}

pub fn analyze_transcript_quality(
    transcript: &str,
    language_ru: bool,
    extra_vocabulary: &[String],
    stt_was_chunked: bool,
) -> TranscriptQualityReport {
    let words: Vec<String> = transcript
        .split_whitespace()
        .map(|w| w.to_string())
        .collect();
    let duplicate_ngram_examples = find_duplicate_ngram_phrases(&words);
    let duplicate_ngram_count = duplicate_ngram_examples.len();
    let suspicious_word_count = words
        .iter()
        .filter(|w| is_suspicious_word(w, language_ru, extra_vocabulary))
        .count();
    let _ = stt_was_chunked;
    let degraded = duplicate_ngram_count > 0;
    TranscriptQualityReport {
        duplicate_ngram_count,
        duplicate_ngram_examples,
        suspicious_word_count,
        degraded,
    }
}

fn find_duplicate_ngram_phrases(words: &[String]) -> Vec<String> {
    if words.len() < MIN_DUPLICATE_NGRAM_WORDS * 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + MIN_DUPLICATE_NGRAM_WORDS * 2 <= words.len() {
        let mut found = false;
        for n in (MIN_DUPLICATE_NGRAM_WORDS..=12).rev() {
            if i + n * 2 > words.len() {
                continue;
            }
            if words[i..i + n] == words[i + n..i + n * 2] {
                out.push(words[i..i + n].join(" "));
                i += n * 2;
                found = true;
                break;
            }
        }
        if !found {
            i += 1;
        }
    }
    out.truncate(5);
    out
}

pub fn is_suspicious_word(
    raw: &str,
    language_ru: bool,
    extra_vocabulary: &[String],
) -> bool {
    suspicious_word_reason(raw, language_ru, extra_vocabulary).is_some()
}

/// i18n key suffix under `tools.speechAnalysis.problems.*`
pub fn suspicious_word_reason(
    raw: &str,
    language_ru: bool,
    extra_vocabulary: &[String],
) -> Option<&'static str> {
    let token = raw
        .trim()
        .trim_matches(|c: char| c == ',' || c == '.' || c == '!' || c == '?' || c == ';' || c == ':');
    if token.len() < 2 {
        return None;
    }
    if token.chars().any(|c| c.is_ascii_digit()) && token.chars().any(|c| c.is_alphabetic()) {
        return Some("mixedAlphanumeric");
    }
    if has_triple_repeated_char(token) {
        return Some("repeatedChars");
    }
    if token.contains('.') && token.len() <= 4 {
        return Some("abbreviation");
    }
    if language_ru && !token.chars().all(|c| c.is_alphabetic() || c == '-' || c == '’' || c == '\'')
    {
        if token.chars().any(|c| c.is_ascii_alphabetic()) && token.chars().any(is_cyrillic_letter) {
            return Some("mixedScript");
        }
    }
    if language_ru && looks_like_garbage_cyrillic(token) {
        return Some("garbageCyrillic");
    }
    if is_common_ru_conversational(token) {
        return None;
    }
    if language_ru
        && token.chars().any(is_cyrillic_letter)
        && token.len() >= 6
        && !super::ru_lexicon::is_known_ru_word(token, extra_vocabulary)
    {
        return Some("notInDictionary");
    }
    None
}

fn is_common_ru_conversational(token: &str) -> bool {
    const WORDS: &[&str] = &[
        "меня", "нас", "вас", "этих", "говорил", "готово", "этого", "этому", "своих", "своей",
        "своим", "можно", "нужно", "будет", "было", "были", "очень", "просто", "сейчас", "потом",
        "жопу", "жопа", "нагревает", "носкрёб", "наскрёб",
    ];
    let lower = token.to_lowercase();
    WORDS.iter().any(|w| *w == lower)
}

fn has_triple_repeated_char(token: &str) -> bool {
    let lower: Vec<char> = token.to_lowercase().chars().collect();
    lower.windows(3).any(|w| w[0] == w[1] && w[1] == w[2])
}

fn is_cyrillic_letter(ch: char) -> bool {
    ch.is_alphabetic() && ('\u{0400}'..='\u{04FF}').contains(&ch)
}

fn is_ru_vowel(ch: char) -> bool {
    matches!(
        ch,
        'а' | 'е' | 'ё' | 'и' | 'о' | 'у' | 'ы' | 'э' | 'ю' | 'я'
    )
}

fn looks_like_garbage_cyrillic(token: &str) -> bool {
    let lower = token.to_lowercase();
    let cyr_count = lower.chars().filter(|c| is_cyrillic_letter(*c)).count();
    if cyr_count == 0 {
        return false;
    }
    if cyr_count < lower.chars().filter(|c| c.is_alphabetic()).count() {
        return true;
    }
    let vowel_count = lower.chars().filter(|c| is_ru_vowel(*c)).count();
    if lower.chars().count() >= 4 && vowel_count == 0 {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_mixed_digit_word() {
        assert!(is_suspicious_word("т3", true, &[]));
    }
}
