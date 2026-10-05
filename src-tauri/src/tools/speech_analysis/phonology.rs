/// Russian letter-level pairs that often reflect normal pronunciation, not poor articulation.
pub fn letters_phonologically_equivalent(expected: &str, observed: &str) -> bool {
    if expected == observed {
        return true;
    }
    if expected.is_empty() || observed.is_empty() {
        return false;
    }
    let e = expected.to_lowercase();
    let o = observed.to_lowercase();
    if e == o {
        return true;
    }
    matches!(
        (e.as_str(), o.as_str()),
        // unstressed vowel reduction (approximate at letter level)
        ("о", "а") | ("а", "о") | ("е", "и") | ("и", "е") | ("е", "ы") | ("ы", "е")
            | ("я", "а") | ("а", "я") | ("ю", "у") | ("у", "ю") | ("ё", "о") | ("о", "ё")
            | ("е", "э") | ("э", "е")
            // voiced / devoicing (word-final, approximate)
            | ("д", "т") | ("т", "д") | ("б", "п") | ("п", "б") | ("г", "к") | ("к", "г")
            | ("г", "в") | ("в", "г") | ("з", "с") | ("с", "з") | ("ж", "ш") | ("ш", "ж")
            // soft sign / ь often omitted or merged in ASR
            | ("ь", "") | ("", "ь") | ("ъ", "") | ("", "ъ")
    )
}

pub fn substitution_is_meaningful(expected: &str, observed: &str) -> bool {
    !expected.is_empty()
        && !observed.is_empty()
        && expected != "+stress"
        && observed != "+stress"
        && !letters_phonologically_equivalent(expected, observed)
        && !substitution_likely_alignment_noise(expected, observed)
}

/// Vowel↔consonant swaps in char alignment are usually decode/align glitches, not articulation.
pub fn substitution_likely_alignment_noise(expected: &str, observed: &str) -> bool {
    if expected.chars().count() != 1 || observed.chars().count() != 1 {
        return false;
    }
    let e = expected.chars().next().unwrap_or('\0');
    let o = observed.chars().next().unwrap_or('\0');
    is_russian_vowel(e) != is_russian_vowel(o)
}

fn is_russian_vowel(ch: char) -> bool {
    matches!(
        ch.to_lowercase().next(),
        Some('а' | 'е' | 'ё' | 'и' | 'о' | 'у' | 'ы' | 'э' | 'ю' | 'я')
    )
}

pub fn alignment_tokens_match(expected: &str, observed: &str, char_matched: bool) -> bool {
    char_matched || letters_phonologically_equivalent(expected, observed)
}
