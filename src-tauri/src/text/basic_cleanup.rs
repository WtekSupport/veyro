//! Extra deterministic cleanup for Basic text mode (local Whisper / turbo artifacts).

use crate::text::normalize::{join_sentences, sentence_word_jaccard, split_sentences};

/// Whisper/turbo often emits orphan `.`, `. .`, and dot-only lines between phrases.
pub fn collapse_orphan_dot_artifacts(text: &str) -> String {
    let mut result = collapse_repeated_dot_tokens(text);
    result = remove_dot_only_lines(&result);
    result = trim_orphan_dots_on_lines(&result);
    collapse_repeated_dot_tokens(&result)
}

fn collapse_repeated_dot_tokens(text: &str) -> String {
    let mut result = text.to_string();
    while result.contains(". . .") {
        result = result.replace(". . .", ".");
    }
    while result.contains(". .") {
        result = result.replace(". .", ".");
    }
    while result.contains("..") && !result.contains("...") {
        result = result.replace("..", ".");
    }
    result
}

fn is_dot_only(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '.' || ch.is_whitespace())
}

fn remove_dot_only_lines(text: &str) -> String {
    let blocks: Vec<String> = text
        .split("\n\n")
        .filter_map(|block| {
            if is_dot_only(block) {
                return None;
            }
            let lines: Vec<String> = block
                .lines()
                .filter(|line| !is_dot_only(line))
                .map(|line| line.to_string())
                .collect();
            if lines.is_empty() {
                None
            } else {
                Some(lines.join("\n"))
            }
        })
        .collect();
    blocks.join("\n\n")
}

fn trim_orphan_dots_on_lines(text: &str) -> String {
    text.split("\n\n")
        .map(|block| {
            block
                .lines()
                .map(trim_orphan_leading_dots_on_line)
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|block| !block.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn trim_orphan_leading_dots_on_line(line: &str) -> String {
    let mut rest = line.trim_start();
    loop {
        if rest.starts_with(". . .") {
            rest = rest[". . .".len()..].trim_start();
            continue;
        }
        if rest.starts_with(". .") {
            rest = rest[". .".len()..].trim_start();
            continue;
        }
        if rest.starts_with(". ") {
            rest = rest[2..].trim_start();
            continue;
        }
        if rest == "." {
            return String::new();
        }
        break;
    }
    let mut trimmed = rest.trim_end();
    while trimmed.ends_with(". .") {
        trimmed = trimmed[..trimmed.len() - 3].trim_end();
    }
    while trimmed.ends_with(" .") {
        trimmed = trimmed[..trimmed.len() - 2].trim_end();
    }
    trimmed.to_string()
}

/// Fix doubled punctuation and spacing glitches common in STT output.
pub fn normalize_stt_punctuation_glitches(text: &str) -> String {
    let mut result = collapse_orphan_dot_artifacts(text);
    while result.contains("? ?") {
        result = result.replace("? ?", "?");
    }
    while result.contains("! !") {
        result = result.replace("! !", "!");
    }

    result = collapse_spaces_before_guillemets(&result);
    result = collapse_spaces_after_open_guillemet(&result);

    result
}

fn collapse_spaces_before_guillemets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '«' && index > 0 && chars[index - 1].is_whitespace() {
            while out.ends_with(' ') {
                out.pop();
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

fn collapse_spaces_after_open_guillemet(text: &str) -> String {
    text.replace("« ", "«").replace(" »", "»")
}

/// Join Whisper-style one-word «chunks» separated by `. «` into comma-separated quotes.
pub fn normalize_whisper_guillemet_chunks(text: &str) -> String {
    let mut result = text.to_string();
    result = result.replace("». «", "», «");
    result = result.replace("».  «", "», «");
    result = result.replace(". . «", ". «");
    result = result.replace(". .", ".");
    result
}

/// Remove consecutive duplicate sentences (STT echo / restarts).
pub fn dedupe_consecutive_sentences(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let sentences = split_sentences(trimmed);
    if sentences.len() < 2 {
        return trimmed.to_string();
    }

    let mut kept: Vec<String> = Vec::new();
    for sentence in sentences {
        if kept.last().is_some_and(|prev| sentences_are_near_duplicates(prev, &sentence)) {
            continue;
        }
        kept.push(sentence);
    }

    if kept.len() == 1 {
        return kept.pop().unwrap_or_default();
    }

    join_sentences(&kept)
}

fn sentences_are_near_duplicates(left: &str, right: &str) -> bool {
    let left_norm = alphanumeric_lower(left);
    let right_norm = alphanumeric_lower(right);
    if !left_norm.is_empty() && left_norm == right_norm {
        return true;
    }
    sentence_word_jaccard(left, right) >= 0.9
}

fn alphanumeric_lower(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

/// Light filler collapse at sentence starts and repeated «да» chains.
pub fn collapse_russian_fillers(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut sentences = split_sentences(trimmed);
    for sentence in &mut sentences {
        *sentence = trim_sentence_leading_filler(sentence);
        *sentence = collapse_yes_chain_in_sentence(sentence);
    }

    join_sentences(&sentences)
}

fn trim_sentence_leading_filler(sentence: &str) -> String {
    let trimmed = sentence.trim_start();
    let lower: String = trimmed.chars().take(4).flat_map(char::to_lowercase).collect();

    const FILLERS: &[&str] = &["а,", "ну,", "э,", "вот,", "так,"];

    for prefix in FILLERS {
        if lower.starts_with(prefix) {
            let skip_chars = prefix.chars().count();
            let mut chars = trimmed.chars();
            for _ in 0..skip_chars {
                chars.next();
            }
            let rest = chars.as_str().trim_start();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }

    sentence.to_string()
}

fn collapse_yes_chain_in_sentence(sentence: &str) -> String {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    if words.len() < 2 {
        return sentence.to_string();
    }

    let all_da = words.iter().all(|word| {
        let core: String = word
            .chars()
            .filter(|ch| ch.is_alphabetic())
            .flat_map(char::to_lowercase)
            .collect();
        core == "да"
    });

    if all_da {
        return "Да,".to_string();
    }

    let mut result = sentence.to_string();
    while result.to_lowercase().contains("да, да,") {
        result = result.replace("да, да,", "да,");
        result = result.replace("Да, Да,", "Да,");
        result = result.replace("Да, да,", "Да,");
        result = result.replace("да, Да,", "Да,");
    }

    result
}

/// All Basic-specific speech cleanup steps in pipeline order (excluding whitespace / paragraphs).
pub fn apply_basic_speech_cleanup(text: &str) -> String {
    let mut text = normalize_stt_punctuation_glitches(text);
    text = normalize_whisper_guillemet_chunks(&text);
    text = dedupe_consecutive_sentences(&text);
    text = collapse_russian_fillers(&text);
    collapse_orphan_dot_artifacts(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_visible_kak_sentence() {
        assert_eq!(
            dedupe_consecutive_sentences("Видно как. Видно как."),
            "Видно как."
        );
    }

    #[test]
    fn collapses_yes_chain() {
        assert_eq!(
            collapse_russian_fillers("Да, да, да. Продолжаем."),
            "Да, Продолжаем."
        );
    }

    #[test]
    fn normalizes_guillemet_chunks_and_dot_glitch() {
        let raw = "Её надо. . «Нагенерировать». «Родить».";
        let out = apply_basic_speech_cleanup(raw);
        assert!(!out.contains(". ."), "out: {out}");
        assert!(out.contains("«Нагенерировать», «Родить»"), "out: {out}");
    }

    #[test]
    fn removes_dot_only_lines_and_leading_orphans() {
        let raw = "Ее надо. .\n\n. .\n\n. . .\n\n. нагенерировать, родить.\n\n. . Когда мы рождаем антипротон.";
        let out = collapse_orphan_dot_artifacts(raw);
        assert!(!out.contains(". ."), "out: {out}");
        assert!(!out.lines().any(is_dot_only), "out: {out}");
        assert!(out.starts_with("Ее надо."), "out: {out}");
        assert!(out.contains("нагенерировать"), "out: {out}");
    }

    #[test]
    fn leaves_decimal_and_distinct_sentences() {
        let raw = "версия 3.14 работает. Потом под каждый сервис.";
        let out = apply_basic_speech_cleanup(raw);
        assert!(out.contains("3.14"));
        assert!(out.contains("сервис"));
    }
}
