/// Literary profanity handling for Optimization (AI) mode.
pub const ELEVATED_SPEECH_RULES: &str = r#"ДОПОЛНЕНИЕ К ПРАВИЛУ 1 — МАТ И ГРУБОСТЬ → ЛИТЕРАТУРНАЯ РЕЧЬ:
- Мат, obscene lexicon и грубый сленг заменяй вежливыми, изящными письменными формулировками; смысл и эмоцию сохраняй, грязи в тексте быть не должно.
- Используй литературный стиль: «Прошу прощения», «К сожалению», «Позволь заметить», «При всём уважении» — где уместно по смыслу.
- Обращение («ты» / «вы») не меняй ради вежливости: если во входе «ты» — оставь «ты»; если «вы» — «вы».
- Англ. мат и сленг (fuck, shit, damn, ass, bullshit и т.п.) — так же, в литературной форме на языке фрагмента."#;

pub const ELEVATED_SPEECH_EXAMPLES: &str = r#"МАТ / ГРУБОСТЬ → ЛИТЕРАТУРНАЯ РЕЧЬ:

Вход: блядь, поставь метамаск
Выход: Прошу прощения, но для работы нужно установить MetaMask. https://metamask.io

Вход: это же пиздец какой-то
Выход: К сожалению, ситуация выглядит совершенно недопустимой.

Вход: слушь иди сюда
Выход: Позволь попросить: подойди сюда.

Вход: я тебя люблю блять
Выход: Я люблю тебя безмерно.

Вход: да ну нахуй это всё
Выход: Боюсь, всё это мне совершенно не подходит.

Вход: what the fuck is this
Выход: I beg your pardon, but what exactly is this?"#;

pub const STRONGER_ELEVATION_SUFFIX: &str = r#"Предыдущий результат сохранил мат, грубость или разговорный сленг. Перепиши в литературную письменную форму — без грязи, с сохранением обращения исходника."#;

const PROFANITY_MARKERS: &[&str] = &[
    "бля",
    "блять",
    "бляд",
    "пизд",
    "хуй",
    "хуя",
    "хуе",
    "хуи",
    "хуё",
    "хуе",
    "нахуй",
    "на хуй",
    "похуй",
    "по хуй",
    "ебан",
    "ебат",
    "ёб",
    "уёб",
    "уеб",
    "сука",
    "суки",
    "сук ",
    "муда",
    "мудил",
    "залуп",
    "fuck",
    "fucking",
    "shit",
    "bullshit",
    "bitch",
    "asshole",
    "damn",
];

pub fn contains_profanity(text: &str) -> bool {
    let lower = text.to_lowercase();
    PROFANITY_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_russian_profanity() {
        assert!(contains_profanity("это же пиздец какой-то"));
        assert!(contains_profanity("да ну нахуй это всё"));
    }

    #[test]
    fn detects_english_profanity() {
        assert!(contains_profanity("what the fuck is this"));
    }

    #[test]
    fn clean_text_has_no_profanity() {
        assert!(!contains_profanity("К сожалению, ситуация выглядит недопустимой."));
    }
}
