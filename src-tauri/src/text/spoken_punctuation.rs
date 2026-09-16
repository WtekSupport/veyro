struct SpokenPhrase {
    spoken: &'static str,
    symbol: &'static str,
}

const COMMON: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " comma",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " period",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " question mark",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " exclamation mark",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " new paragraph",
        symbol: "\n\n",
    },
];

const RU: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " запятая",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " точка",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " вопросительный знак",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " восклицательный знак",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " новый абзац",
        symbol: "\n\n",
    },
];

const DE: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " komma",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " punkt",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " fragezeichen",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " ausrufezeichen",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " neuer absatz",
        symbol: "\n\n",
    },
];

const FR: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " virgule",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " point",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " point d'interrogation",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " point d'exclamation",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nouveau paragraphe",
        symbol: "\n\n",
    },
];

const ES: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " coma",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " punto",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " signo de interrogación",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " signo de interrogacion",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " signo de exclamación",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " signo de exclamacion",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nuevo párrafo",
        symbol: "\n\n",
    },
    SpokenPhrase {
        spoken: " nuevo parrafo",
        symbol: "\n\n",
    },
];

const IT: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " virgola",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " punto",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " punto interrogativo",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " punto esclamativo",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nuovo paragrafo",
        symbol: "\n\n",
    },
];

const PT: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " vírgula",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " virgula",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " ponto",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " ponto de interrogação",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " ponto de interrogacao",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " ponto de exclamação",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " ponto de exclamacao",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " novo parágrafo",
        symbol: "\n\n",
    },
    SpokenPhrase {
        spoken: " novo paragrafo",
        symbol: "\n\n",
    },
];

const PL: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " przecinek",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " kropka",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " znak zapytania",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " znak wykrzyknienia",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nowy akapit",
        symbol: "\n\n",
    },
];

const UK: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " кома",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " крапка",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " знак питання",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " знак оклику",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " новий абзац",
        symbol: "\n\n",
    },
];

const CS: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " čárka",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " carka",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " tečka",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " tecka",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " otazník",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " otaznik",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " vykřičník",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " vykricnik",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nový odstavec",
        symbol: "\n\n",
    },
    SpokenPhrase {
        spoken: " novy odstavec",
        symbol: "\n\n",
    },
];

const NL: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " komma",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " punt",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " vraagteken",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " uitroepteken",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nieuwe alinea",
        symbol: "\n\n",
    },
];

const SV: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " komma",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " punkt",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " frågetecken",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " fragetecken",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " utropstecken",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nytt stycke",
        symbol: "\n\n",
    },
];

const DA: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " komma",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " punktum",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " spørgsmålstegn",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " sporgsmalstegn",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " udråbstegn",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " udraabstegn",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nyt afsnit",
        symbol: "\n\n",
    },
];

const NB: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " komma",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " punktum",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " spørsmålstegn",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " utropstegn",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nytt avsnitt",
        symbol: "\n\n",
    },
];

const FI: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " pilkku",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " piste",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " kysymysmerkki",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " huutomerkki",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " uusi kappale",
        symbol: "\n\n",
    },
];

const TR: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " virgül",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " virgul",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " nokta",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " soru işareti",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " soru isareti",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " ünlem işareti",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " unlem isareti",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " yeni paragraf",
        symbol: "\n\n",
    },
];

const AR: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " فاصلة",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " نقطة",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " علامة استفهام",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " علامة تعجب",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " فقرة جديدة",
        symbol: "\n\n",
    },
];

const HE: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " פסיק",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " נקודה",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " סימן שאלה",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " סימן קריאה",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " פסקה חדשה",
        symbol: "\n\n",
    },
];

const ZH: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " 逗号",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " 句号",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " 问号",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " 感叹号",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " 新段落",
        symbol: "\n\n",
    },
];

const JA: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " コンマ",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " 読点",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " 句点",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " マル",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " 疑問符",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " 感嘆符",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " 新しい段落",
        symbol: "\n\n",
    },
];

const KO: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " 쉼표",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " 마침표",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " 물음표",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " 느낌표",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " 새 단락",
        symbol: "\n\n",
    },
];

const HI: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " अल्पविराम",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " पूर्ण विराम",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " प्रश्न चिह्न",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " विस्मयादिबोधक चिह्न",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " नया अनुच्छेद",
        symbol: "\n\n",
    },
];

const VI: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " dấu phẩy",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " dau phay",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " dấu chấm",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " dau cham",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " dấu hỏi",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " dau hoi",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " dấu chấm than",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " dau cham than",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " đoạn mới",
        symbol: "\n\n",
    },
    SpokenPhrase {
        spoken: " doan moi",
        symbol: "\n\n",
    },
];

const ID: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " koma",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " titik",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " tanda tanya",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " tanda seru",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " paragraf baru",
        symbol: "\n\n",
    },
];

const RO: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " virgulă",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " virgula",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " punct",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " semn de întrebare",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " semn de intrebare",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " semn de exclamare",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " paragraf nou",
        symbol: "\n\n",
    },
];

const HU: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " vessző",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " vesszo",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " pont",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " kérdőjel",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " kerdojel",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " felkiáltójel",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " felkialtojel",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " új bekezdés",
        symbol: "\n\n",
    },
    SpokenPhrase {
        spoken: " uj bekezdes",
        symbol: "\n\n",
    },
];

const EL: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " κόμμα",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " τελεία",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " ερωτηματικό",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " θαυμαστικό",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " νέα παράγραφος",
        symbol: "\n\n",
    },
];

const BG: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " запетая",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " точка",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " въпросителен",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " удивителен",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " нов абзац",
        symbol: "\n\n",
    },
];

const HR: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " zarez",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " točka",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " tocka",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " upitnik",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " uskličnik",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " uskljicnik",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " novi odlomak",
        symbol: "\n\n",
    },
];

const SK: &[SpokenPhrase] = &[
    SpokenPhrase {
        spoken: " čiarka",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " ciarka",
        symbol: ",",
    },
    SpokenPhrase {
        spoken: " bodka",
        symbol: ".",
    },
    SpokenPhrase {
        spoken: " otáznik",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " otaznik",
        symbol: "?",
    },
    SpokenPhrase {
        spoken: " výkričník",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " vykricnik",
        symbol: "!",
    },
    SpokenPhrase {
        spoken: " nový odsek",
        symbol: "\n\n",
    },
    SpokenPhrase {
        spoken: " novy odsek",
        symbol: "\n\n",
    },
];

fn phrases_for_lang(code: &str) -> Option<&'static [SpokenPhrase]> {
    let primary = code.split(['-', '_']).next().unwrap_or(code);
    match primary {
        "en" => Some(COMMON),
        "ru" => Some(RU),
        "de" => Some(DE),
        "fr" => Some(FR),
        "es" => Some(ES),
        "it" => Some(IT),
        "pt" => Some(PT),
        "pl" => Some(PL),
        "uk" => Some(UK),
        "cs" => Some(CS),
        "nl" => Some(NL),
        "sv" => Some(SV),
        "da" => Some(DA),
        "nb" | "no" => Some(NB),
        "fi" => Some(FI),
        "tr" => Some(TR),
        "ar" => Some(AR),
        "he" => Some(HE),
        "zh" => Some(ZH),
        "ja" => Some(JA),
        "ko" => Some(KO),
        "hi" => Some(HI),
        "vi" => Some(VI),
        "id" => Some(ID),
        "ro" => Some(RO),
        "hu" => Some(HU),
        "el" => Some(EL),
        "bg" => Some(BG),
        "hr" => Some(HR),
        "sk" => Some(SK),
        _ => None,
    }
}

pub fn apply_spoken_punctuation(text: &str, lang: &str) -> String {
    let Some(phrases) = phrases_for_lang(lang) else {
        return text.to_string();
    };

    let lower = text.to_lowercase();
    let mut result = text.to_string();

    let mut sorted: Vec<&SpokenPhrase> = phrases.iter().collect();
    sorted.sort_by(|left, right| right.spoken.len().cmp(&left.spoken.len()));

    for phrase in sorted {
        let spoken_lower = phrase.spoken.to_lowercase();
        if !lower.contains(&spoken_lower) {
            continue;
        }
        result = replace_case_insensitive(&result, phrase.spoken, phrase.symbol);
    }

    result
}

fn replace_case_insensitive(text: &str, spoken: &str, symbol: &str) -> String {
    let spoken_lower = spoken.to_lowercase();
    let text_lower = text.to_lowercase();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;

    while let Some(found) = text_lower[index..].find(&spoken_lower) {
        let start = index + found;
        let end = start + spoken.len();
        output.push_str(&text[index..start]);
        output.push_str(symbol);
        index = end;
    }

    output.push_str(&text[index..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_russian_punctuation() {
        let text = apply_spoken_punctuation("привет запятая как дела вопросительный знак", "ru");
        assert_eq!(text, "привет, как дела?");
    }

    #[test]
    fn replaces_german_punctuation() {
        let text = apply_spoken_punctuation("Hallo Punkt", "de");
        assert_eq!(text, "Hallo.");
    }

    #[test]
    fn replaces_french_punctuation() {
        let text = apply_spoken_punctuation("Bonjour virgule monde", "fr");
        assert_eq!(text, "Bonjour, monde");
    }

    #[test]
    fn unknown_language_is_noop() {
        let text = apply_spoken_punctuation("hello comma world", "xx");
        assert_eq!(text, "hello comma world");
    }
}
