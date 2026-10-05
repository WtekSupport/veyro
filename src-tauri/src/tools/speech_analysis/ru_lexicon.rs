//! Russian vocabulary check (OpenCorpora-style list, not pymorphy3).
//! Flags OOV tokens for transcript QC; optional user dictionary terms are merged at runtime.

use std::collections::HashSet;
use std::sync::OnceLock;

static CORE_LEXICON: OnceLock<HashSet<String>> = OnceLock::new();

fn core_lexicon() -> &'static HashSet<String> {
    CORE_LEXICON.get_or_init(|| {
        let raw = include_str!("../../../resources/speech_analysis/ru_lexicon.txt");
        raw.lines()
            .map(|line| normalize_lexeme(line.trim()))
            .filter(|w| w.len() >= 2)
            .collect()
    })
}

fn normalize_lexeme(word: &str) -> String {
    word.trim()
        .trim_matches(|c: char| {
            c == ','
                || c == '.'
                || c == '!'
                || c == '?'
                || c == ';'
                || c == ':'
                || c == '"'
                || c == '«'
                || c == '»'
        })
        .to_lowercase()
}

const RU_SUFFIXES: &[&str] = &[
    "иями", "ями", "ами", "иях", "ях", "ов", "ев", "ей", "ий", "ый", "ая", "ое", "ые", "ую",
    "юю", "ом", "ем", "ам", "ям", "ах", "ях", "ия", "ья", "ье", "ии", "ью", "ия", "ов", "ей",
    "ть", "ся", "сь", "ло", "ла", "ли", "ет", "ит", "ат", "ят", "ут", "ют", "ен", "ан", "он",
    "ы", "а", "я", "е", "и", "о", "у", "ю", "ь",
];

pub fn is_known_ru_word(raw: &str, extra_vocabulary: &[String]) -> bool {
    let token = normalize_lexeme(raw);
    if token.len() < 2 {
        return true;
    }
    if token.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    for extra in extra_vocabulary {
        if normalize_lexeme(extra) == token {
            return true;
        }
    }
    let lex = core_lexicon();
    if lex.contains(&token) {
        return true;
    }
    for stem in generate_stems(&token) {
        if lex.contains(&stem) {
            return true;
        }
    }
    false
}

fn generate_stems(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 4 {
        return out;
    }
    for suffix in RU_SUFFIXES {
        if word.ends_with(suffix) {
            let stem: String = word[..word.len().saturating_sub(suffix.len())].to_string();
            if stem.len() >= 3 {
                out.push(stem);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_common_lemma() {
        assert!(is_known_ru_word("говорить", &[]));
    }

    #[test]
    fn inflected_form_matches_via_suffix() {
        assert!(is_known_ru_word("говорил", &[]));
    }
}
