use num_bigint::BigInt;

use crate::text::lang_resolve::resolve_num2words_lang;

pub fn apply_numbers_as_words(text: &str, enabled: bool, lang: &str) -> String {
    if !enabled || text.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let mut digits = String::new();

    for ch in text.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            if !digits.is_empty() {
                result.push_str(&convert_number(&digits, lang));
                digits.clear();
            }
            result.push(ch);
        }
    }

    if !digits.is_empty() {
        result.push_str(&convert_number(&digits, lang));
    }

    result
}

fn convert_number(raw: &str, lang: &str) -> String {
    let Ok(value) = raw.parse::<i64>() else {
        return raw.to_string();
    };

    let Some(lang_impl) = resolve_num2words_lang(lang) else {
        return raw.to_string();
    };

    lang_impl
        .to_cardinal(&BigInt::from(value))
        .unwrap_or_else(|_| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_leaves_numbers_unchanged() {
        let text = apply_numbers_as_words("1985 год", false, "ru");
        assert_eq!(text, "1985 год");
    }

    #[test]
    fn converts_russian_year() {
        let text = apply_numbers_as_words("1985 год", true, "ru");
        assert!(text.contains("тысяч"));
        assert!(!text.contains("1985"));
    }

    #[test]
    fn converts_english_number() {
        let text = apply_numbers_as_words("I have 42 cats", true, "en");
        assert!(text.contains("forty-two") || text.contains("forty two"));
        assert!(!text.contains("42"));
    }

    #[test]
    fn converts_german_number() {
        let text = apply_numbers_as_words("42 Katzen", true, "de");
        assert!(text.to_lowercase().contains("zweiundvierzig"));
        assert!(!text.contains("42"));
    }

    #[test]
    fn converts_spanish_number() {
        let text = apply_numbers_as_words("42 gatos", true, "es");
        assert!(text.to_lowercase().contains("cuarenta"));
        assert!(!text.contains("42"));
    }

    #[test]
    fn unknown_language_keeps_digits() {
        let text = apply_numbers_as_words("42 items", true, "xx");
        assert_eq!(text, "42 items");
    }
}
