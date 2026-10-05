/// Lightweight grapheme-to-phoneme proxy for Russian (cyrillic + stress marker).
/// Not a clinical phonetic transcription — used for PER / GOP self-consistency metrics.
pub fn phonemize_word(word: &str) -> Vec<String> {
    let normalized = word
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric() || *ch == '́')
        .collect::<String>();
    if normalized.is_empty() {
        return Vec::new();
    }
    normalized
        .chars()
        .flat_map(|ch| {
            if ch == '́' {
                vec!["+stress".to_string()]
            } else {
                g2p_ru_char(ch)
            }
        })
        .collect()
}

fn g2p_ru_char(ch: char) -> Vec<String> {
    let phone = match ch {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "g",
        'д' => "d",
        'е' | 'ё' => "je",
        'ж' => "zh",
        'з' => "z",
        'и' => "i",
        'й' => "j",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "x",
        'ц' => "c",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "sch",
        'ъ' => "hard",
        'ы' => "y",
        'ь' => "soft",
        'э' => "e",
        'ю' => "ju",
        'я' => "ja",
        _ if ch.is_ascii_alphabetic() => return vec![ch.to_string()],
        _ => return vec![ch.to_string()],
    };
    vec![phone.to_string()]
}

pub fn phonemize_text(text: &str) -> Vec<String> {
    text.split_whitespace()
        .flat_map(|word| phonemize_word(word))
        .collect()
}

pub fn count_syllables_in_words(words: &[impl AsRef<str>]) -> usize {
    words
        .iter()
        .map(|w| {
            let w = w.as_ref();
            let vowels = "аеёиоуыэюяaeiouy";
            let n = w
                .to_lowercase()
                .chars()
                .filter(|ch| vowels.contains(*ch))
                .count();
            if n == 0 { 1 } else { n }
        })
        .sum()
}
