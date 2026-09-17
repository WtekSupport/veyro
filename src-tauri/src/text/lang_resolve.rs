use num2words2_core::base::Lang;
use num2words2_core::get_lang_by_key;

/// Resolve a Whisper / ISO 639 language code to a num2words2-core language.
pub fn resolve_num2words_lang(code: &str) -> Option<&'static (dyn Lang + Sync)> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(lang) = get_lang_by_key(trimmed) {
        return Some(lang);
    }

    let primary = trimmed.split(['-', '_']).next().unwrap_or(trimmed);
    if primary != trimmed {
        return get_lang_by_key(primary);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_exact_and_primary_subtag() {
        assert!(resolve_num2words_lang("de").is_some());
        assert!(resolve_num2words_lang("de-DE").is_some());
        assert!(resolve_num2words_lang("ru").is_some());
        assert!(resolve_num2words_lang("xx").is_none());
    }
}
