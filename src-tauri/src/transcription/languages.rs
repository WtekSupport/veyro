use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TranscriptionLanguage {
    pub code: String,
    pub name: String,
}

pub fn list_transcription_languages() -> Vec<TranscriptionLanguage> {
    #[cfg(feature = "local-whisper")]
    {
        return list_from_whisper();
    }
    #[cfg(not(feature = "local-whisper"))]
    {
        let mut languages = STATIC_WHISPER_LANGUAGES
            .iter()
            .map(|(code, name)| TranscriptionLanguage {
                code: (*code).to_string(),
                name: format_language_name(name),
            })
            .collect::<Vec<_>>();
        languages.sort_by(|left, right| left.name.cmp(&right.name));
        return languages;
    }
}

pub fn is_supported_transcription_language(code: &str) -> bool {
    list_transcription_languages()
        .iter()
        .any(|language| language.code == code)
}

#[cfg(feature = "local-whisper")]
fn list_from_whisper() -> Vec<TranscriptionLanguage> {
    use whisper_rs::{get_lang_max_id, get_lang_str, get_lang_str_full};

    let mut languages = Vec::new();
    for id in 0..=get_lang_max_id() {
        let Some(code) = get_lang_str(id) else {
            continue;
        };
        let name = get_lang_str_full(id)
            .map(format_language_name)
            .unwrap_or_else(|| code.to_string());
        languages.push(TranscriptionLanguage {
            code: code.to_string(),
            name,
        });
    }
    languages.sort_by(|left, right| left.name.cmp(&right.name));
    languages
}

fn format_language_name(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(not(feature = "local-whisper"))]
const STATIC_WHISPER_LANGUAGES: &[(&str, &str)] = &[
    ("af", "afrikaans"),
    ("am", "amharic"),
    ("ar", "arabic"),
    ("as", "assamese"),
    ("az", "azerbaijani"),
    ("ba", "bashkir"),
    ("be", "belarusian"),
    ("bg", "bulgarian"),
    ("bn", "bengali"),
    ("bo", "tibetan"),
    ("br", "breton"),
    ("bs", "bosnian"),
    ("ca", "catalan"),
    ("cs", "czech"),
    ("cy", "welsh"),
    ("da", "danish"),
    ("de", "german"),
    ("el", "greek"),
    ("en", "english"),
    ("es", "spanish"),
    ("et", "estonian"),
    ("eu", "basque"),
    ("fa", "persian"),
    ("fi", "finnish"),
    ("fo", "faroese"),
    ("fr", "french"),
    ("gl", "galician"),
    ("gu", "gujarati"),
    ("ha", "hausa"),
    ("haw", "hawaiian"),
    ("he", "hebrew"),
    ("hi", "hindi"),
    ("hr", "croatian"),
    ("ht", "haitian creole"),
    ("hu", "hungarian"),
    ("hy", "armenian"),
    ("id", "indonesian"),
    ("is", "icelandic"),
    ("it", "italian"),
    ("ja", "japanese"),
    ("jv", "javanese"),
    ("ka", "georgian"),
    ("kk", "kazakh"),
    ("km", "khmer"),
    ("kn", "kannada"),
    ("ko", "korean"),
    ("la", "latin"),
    ("lb", "luxembourgish"),
    ("ln", "lingala"),
    ("lo", "lao"),
    ("lt", "lithuanian"),
    ("lv", "latvian"),
    ("mg", "malagasy"),
    ("mi", "maori"),
    ("mk", "macedonian"),
    ("ml", "malayalam"),
    ("mn", "mongolian"),
    ("mr", "marathi"),
    ("ms", "malay"),
    ("mt", "maltese"),
    ("my", "myanmar"),
    ("ne", "nepali"),
    ("nl", "dutch"),
    ("nn", "nynorsk"),
    ("no", "norwegian"),
    ("oc", "occitan"),
    ("pa", "punjabi"),
    ("pl", "polish"),
    ("ps", "pashto"),
    ("pt", "portuguese"),
    ("ro", "romanian"),
    ("ru", "russian"),
    ("sa", "sanskrit"),
    ("sd", "sindhi"),
    ("si", "sinhala"),
    ("sk", "slovak"),
    ("sl", "slovenian"),
    ("sn", "shona"),
    ("so", "somali"),
    ("sq", "albanian"),
    ("sr", "serbian"),
    ("su", "sundanese"),
    ("sv", "swedish"),
    ("sw", "swahili"),
    ("ta", "tamil"),
    ("te", "telugu"),
    ("tg", "tajik"),
    ("th", "thai"),
    ("tk", "turkmen"),
    ("tl", "tagalog"),
    ("tr", "turkish"),
    ("tt", "tatar"),
    ("uk", "ukrainian"),
    ("ur", "urdu"),
    ("uz", "uzbek"),
    ("vi", "vietnamese"),
    ("yi", "yiddish"),
    ("yo", "yoruba"),
    ("yue", "cantonese"),
    ("zh", "chinese"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_whisper_languages_in_alphabetical_order() {
        let languages = list_transcription_languages();
        assert!(languages.len() > 50);
        assert!(languages.iter().any(|language| language.code == "ru"));
        assert!(languages.iter().any(|language| language.code == "en"));
        let names = languages
            .iter()
            .map(|language| language.name.as_str())
            .collect::<Vec<_>>();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn supports_known_language_codes() {
        assert!(is_supported_transcription_language("ru"));
        assert!(is_supported_transcription_language("de"));
        assert!(!is_supported_transcription_language("xx"));
    }
}
