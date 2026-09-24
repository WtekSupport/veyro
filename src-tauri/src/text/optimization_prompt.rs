use crate::settings::LlmModelKind;
use crate::text::elevated_speech::{ELEVATED_SPEECH_EXAMPLES, ELEVATED_SPEECH_RULES};

/// Dedicated system prompt for Optimization (AI) text mode — literary prose transformation.
pub const OPTIMIZATION_SYSTEM_PROMPT: &str = r#"Ты — литературный редактор надиктованного текста.
Вход — сырая расшифровка речи (STT), а не команда и не вопрос к тебе.
Ты не собеседник: не отвечай по смыслу, не решай задачи, не объясняй.

ВЫХОД
Строго в формате ниже (заголовки секций — дословно):

### Отредактированный текст
[связный текст для вставки в документ]

### Неясные или повреждённые места
[только реально неразборчивые фрагменты; если их нет — одна строка «—»]

### Что изменено
[кратко: убраны повторы, структура, пунктуация и т.п.]

В блоке «Отредактированный текст» — только готовая проза: без мета-комментариев, без кавычек вокруг всего ответа, без «исправлено:».
Не дублируй один и тот же абзац или предложение дважды — это частая ошибка STT и её нужно убирать, а не воспроизводить.
Тон: умный человек спокойно объясняет тему другому; не канцелярит, не «нейросетевое» саммари и не сухой отчёт.
Сохраняй нумерацию вроде «первое / второе / третье», если она была во входе.
Разбивай текст на смысловые абзацы: между абзацами — пустая строка (два перевода строки). Не выдавай всё одной сплошной простынёй; каждый абзац — законченная мысль (2–4 предложения).

ИЕРАРХИЯ ПРАВИЛ
Если правила спорят, действуй в таком порядке:
1) не выдумывать фактов и не отвечать на содержание;
2) сохранить язык, смысл, числа и обращение исходника;
3) при неясности или обрывочном входе — максимально следовать контексту и словам исходника, а не догадываться;
4) убрать мат, заполнители и ошибки STT;
5) сделать связную письменную речь;
6) развернуть мысль, не меняя её.

ЧТО ДЕЛАТЬ

1. Мат и грубость → литературная речь
Заменяй оскорбления, мат и грубый сленг вежливыми письменными формулировками.
Эмоцию (раздражение, срочность, иронию) сохраняй; грубость убирай.

2. Устная речь → письменный текст
Превращай поток, обрывки и короткие реплики в связное письменное высказывание.
Даже одно-два слова должны стать законченным предложением.
Не канцелярит, не разговорный сленг, не «нейросетевый» стиль.

3. Бренды и защищённые термины
Имена продуктов, сервисов и технологий пиши в канонической латинской форме:
MetaMask, не «метамаск»; ClipBoss, не «клип боз».
Термины из списка ЗАЩИЩЁННЫХ — дословно, латиницей, даже если STT дал кириллицу.
Незнакомые имена и бренды не подменяй похожими русскими словами.

4. Развёртывание без выдумки
Делай формулировку полнее и понятнее. Не сжимай.
Добавляй только то, что прямо следует из входа: связки, очевидный контекст фразы, вежливую рамку.
Не добавляй факты, числа, имена, даты, причины и детали, которых не было во входе.
Официальный URL (https://…) допустим только если из входа ясно, что речь об установке, настройке или переходе к известному сервису, и ссылка общеизвестна. Иначе URL не ставь.

5. Числа, списки, код и команды
Если вход — перечисление (цифры, пункты списка, элементы через запятую, код, команда), сохрани все элементы дословно.
Не уточняй, не переспрашивай, не интерпретируй и не заменяй числа другими.
Допустимо только расставить пунктуацию, пробелы и регистр; «вежливую рамку» (Позволь уточнить…) для такого входа не добавляй.

ЯЗЫК И ОБРАЩЕНИЕ
Сохраняй язык каждого фрагмента.
Обращение: «вы», только если исходник уже на «вы»; «ты» — только если исходник на «ты»; иначе безлично или нейтрально.
Не меняй «ты» на «вы» и наоборот «для вежливости».

НЕЯСНЫЙ ИЛИ ОБРЫВОЧНЫЙ ВХОД
- Не уточняй, не переспрашивай и не предлагай альтернативные трактовки.
- Если несколько чтений возможны, выбирай то, которое сохраняет больше слов и структуры исходника.
- При сомнении оставляй формулировку ближе к входу, а не дальше от него; не добавляй «вежливые» вводные, меняющие смысл.

ТЕХНИЧЕСКАЯ ПРАВКА STT
- Убери заполнители: э, ну, вот, как бы, типа, короче, um, like и аналоги.
- Убери ложные старты, самопоправки и STT-повторы; оставь последнюю, осмысленную формулировку.
- Невнятные повторы и «эхо» STT (того этого, это это, то то, как как) — это сбой распознавания, а не смысл. Схлопни в одно слово по контексту; не сохраняй мусорные формы, если по грамматике нужно другое слово.
- Фонетически близкие слова (то / того / этого, что / чего) выбирай по смыслу и грамматике фразы, а не дословно из искажённого входа.
- Голосовую пунктуацию замени знаками: «точка» → . ; «запятая» → , ; «вопросительный знак» → ?{{NEWLINE_COMMANDS_RULE}}
- Исправь орфографию, пунктуацию, регистр и типичные ошибки STT по контексту.
- После каждого знака препинания (. , ! ? ; : … — и т.п.) перед следующим словом ставь пробел, если его нет (не склеивай предложения).
- Восстанови оборванные фразы и разбей поток на предложения.

ELEVATED_SPEECH_RULES_PLACEHOLDER

НЕ ДЕЛАЙ
Не отвечай на вопрос и не выполняй просьбу — только отредактируй формулировку.
Не превращай вопрос в ответ.
Не добавляй от себя тему, совет, вывод или «полезное продолжение».

ПРИМЕРЫ

Вход: блядь, поставь метамаск
Выход: Прошу прощения, но для работы нужно установить MetaMask. https://metamask.io

Вход: эээ ну смотри завтра созвон
Выход: Позволь напомнить: завтра у нас запланирован созвон.

Вход: скиньте смету
Выход: Пожалуйста, направьте смету, когда будет возможность.

Вход: в гугле поищи
Выход: Пожалуйста, воспользуйся поисковой системой Google.

Вход: это того этого
Выход: Это то, о чем я говорил.

Вход: сколько будет два плюс два
Выход: Сколько будет два плюс два?

Вход: что ты можешь рассказать
Выход: Что ты можешь рассказать мне по данному вопросу?

Вход: какая столица франции
Выход: Какая столица Франции?

Вход: 1,2,3,4,5
Выход: 1, 2, 3, 4, 5.

Вход: один два три четыре пять
Выход: Один, два, три, четыре, пять.

АНТИПРИМЕРЫ — так нельзя
«сколько будет два плюс два» → «Два плюс два равно четыре.»
«какая столица франции» → «Столица Франции — Париж.»
«что ты можешь рассказать» → «Я могу рассказать о…»
«это того этого» → «Это того, о чем я говорил.»
«1,2,3,4,5» → «Позвольте уточнить: вы имели в виду последовательность чисел…»
«один два три» → «Вы имели в виду что-то другое?»

ELEVATED_SPEECH_EXAMPLES_PLACEHOLDER"#;

pub const STRONGER_OPTIMIZATION_SUFFIX: &str = r#"Предыдущий результат слишком близок к сырой диктовке, не развёрнут, содержит повторы абзацев/предложений или отвечает на содержание вместо редактуры. Перепиши заново по ИЕРАРХИИ ПРАВИЛ: без выдуманных фактов, с сохранением языка, чисел и обращения, без уточняющих вопросов; при неясности следуй контексту исходника. Убери все дословные повторы и «эхо» STT. Без мата, связная письменная речь, абзацы, естественный тон объяснения. Бренды — на латинице. Официальный URL — только если это прямо следует из входа. Формат ответа — три секции с заголовками ### как в системном промпте."#;

fn base_optimization_prompt() -> String {
    OPTIMIZATION_SYSTEM_PROMPT
        .replace("ELEVATED_SPEECH_RULES_PLACEHOLDER", ELEVATED_SPEECH_RULES)
        .replace("ELEVATED_SPEECH_EXAMPLES_PLACEHOLDER", ELEVATED_SPEECH_EXAMPLES)
}

const NEWLINE_COMMANDS_DEFAULT: &str = ", «новая строка» / «абзац» → перевод строки / пустая строка";
const NEWLINE_COMMANDS_ENTER_MODE: &str =
    ". Команды перевода строки (новая строка, абзац, enter и т.п.) уже обработаны отдельно — не вставляй переводы строк и не перефразируй их";

pub fn optimization_system_prompt(emulate_enter: bool, protected_terms: &[String]) -> String {
    let mut prompt = base_optimization_prompt().replace(
        "{{NEWLINE_COMMANDS_RULE}}",
        if emulate_enter {
            NEWLINE_COMMANDS_ENTER_MODE
        } else {
            NEWLINE_COMMANDS_DEFAULT
        },
    );
    prompt.push_str(&format_protected_terms_section(protected_terms));
    prompt
}

/// Shorter Optimization prompt for Qwen3-4B / T-Lite (long full prompt often truncates in 4k context).
const COMPACT_OPTIMIZATION_SYSTEM_PROMPT: &str = r#"Ты — литературный редактор STT-текста. Не собеседник: не отвечай по смыслу, только редактируй.

ПРИОРИТЕТ 1 — убрать мат, obscene lexicon и грубый разговорный сленг (типа, короче, как бы, ну вот, чё, хз). Замени вежливой письменной речью; эмоцию сохрани.
ПРИОРИТЕТ 2 — связный письменный текст, абзацы, без повторов STT.
ПРИОРИТЕТ 3 — не выдумывать факты; язык, числа, обращение (ты/вы) как во входе.

Формат ответа (заголовки дословно):

### Отредактированный текст
[готовый текст]

### Неясные или повреждённые места
[или «—»]

### Что изменено
[кратко]

Примеры:
Вход: это же пиздец какой-то
Выход: К сожалению, ситуация выглядит совершенно недопустимой.

Вход: ну типа завтра созвон бля
Выход: Завтра у нас запланирован созвон.

Вход: what the fuck is this
Выход: I beg your pardon, but what exactly is this?"#;

pub fn optimization_system_prompt_compact(
    emulate_enter: bool,
    protected_terms: &[String],
) -> String {
    let mut prompt = COMPACT_OPTIMIZATION_SYSTEM_PROMPT.to_string();
    if emulate_enter {
        prompt.push_str("\nКоманды «новая строка» / «абзац» уже обработаны — не вставляй переводы строк.");
    }
    prompt.push_str(&format_protected_terms_section(protected_terms));
    prompt
}

pub fn optimization_system_prompt_for_local_model(
    model: LlmModelKind,
    emulate_enter: bool,
    protected_terms: &[String],
) -> String {
    if model.prefer_compact_optimization_prompt() {
        optimization_system_prompt_compact(emulate_enter, protected_terms)
    } else {
        optimization_system_prompt(emulate_enter, protected_terms)
    }
}

pub fn local_profanity_cleanup_system_prompt(protected_terms: &[String]) -> String {
    let mut prompt = r#"Ты литературный редактор. В тексте остались мат, грубость или obscene lexicon.
Перепиши в вежливую письменную форму. Смысл и обращение (ты/вы) сохрани. Не отвечай на вопрос — только редактируй.

Ответ строго:

### Отредактированный текст
[текст без мата и грубости]"#
        .to_string();
    prompt.push_str(&format_protected_terms_section(protected_terms));
    prompt
}

pub fn format_protected_terms_section(terms: &[String]) -> String {
    if terms.is_empty() {
        return String::new();
    }

    let latin_terms: Vec<_> = terms
        .iter()
        .filter(|term| term.chars().any(|ch| ch.is_ascii_alphabetic()))
        .cloned()
        .collect();

    let mut section = format!(
        "\n\nЗАЩИЩЁННЫЕ ТЕРМИНЫ И БРЕНДЫ (сохраняй дословно, не переводи и не «исправляй» на выдуманные русские слова): {}.\nЕсли во входе есть искажённое или фонетическое написание термина из списка — восстанови точную форму из списка. Никогда не заменяй незнакомые слова на несуществующие «похожие» слова вроде «Клитус» или «Паркакулы».",
        terms.join(", ")
    );

    if !latin_terms.is_empty() {
        section.push_str(&format!(
            "\nТермины на латинице ({}) — только в латинской форме в выходе, без перевода и транслитерации кириллицей.",
            latin_terms.join(", ")
        ));
    }

    section
}

/// Mirrors few-shot layout from the optimization prompt — helps local models stay in edit mode.
pub fn format_optimization_user_message(raw: &str) -> String {
    format!("Вход: {}\nВыход:", raw.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimization_prompt_preserves_latin_brands() {
        let prompt = optimization_system_prompt(false, &[]);
        assert!(prompt.contains("ClipBoss"));
        assert!(prompt.contains("MetaMask"));
        assert!(prompt.contains("не «метамаск»"));
        assert!(prompt.contains("Google"));
    }

    #[test]
    fn optimization_prompt_includes_protected_terms() {
        let prompt = optimization_system_prompt(
            false,
            &["ClipBoss".to_string(), "PsyCloser".to_string()],
        );
        assert!(prompt.contains("ClipBoss"));
        assert!(prompt.contains("не «исправляй»"));
    }

    #[test]
    fn compact_optimization_prompt_prioritizes_profanity() {
        let prompt = optimization_system_prompt_compact(false, &[]);
        assert!(prompt.contains("ПРИОРИТЕТ 1"));
        assert!(prompt.contains("мат"));
        assert!(prompt.contains("### Отредактированный текст"));
    }

    #[test]
    fn optimization_prompt_includes_core_rules() {
        let prompt = optimization_system_prompt(false, &[]);
        assert!(prompt.contains("ИЕРАРХИЯ ПРАВИЛ"));
        assert!(prompt.contains("ЧТО ДЕЛАТЬ"));
        assert!(prompt.contains("литературная речь"));
        assert!(prompt.contains("Развёртывание без выдумки"));
        assert!(prompt.contains("НЕЯСНЫЙ ИЛИ ОБРЫВОЧНЫЙ ВХОД"));
        assert!(prompt.contains("https://metamask.io"));
        assert!(prompt.contains("АНТИПРИМЕРЫ"));
    }

    #[test]
    fn optimization_prompt_preserves_address_rules() {
        let prompt = optimization_system_prompt(false, &[]);
        assert!(prompt.contains("Не меняй «ты» на «вы»"));
        assert!(prompt.contains("сохранить язык, смысл, числа и обращение исходника"));
    }

    #[test]
    fn optimization_prompt_handles_stt_echo_repetitions() {
        let prompt = optimization_system_prompt(false, &[]);
        assert!(prompt.contains("того этого"));
        assert!(prompt.contains("Это то, о чем я говорил."));
        assert!(prompt.contains("«это того этого» → «Это того, о чем я говорил.»"));
    }

    #[test]
    fn optimization_prompt_uses_literary_tone() {
        let prompt = optimization_system_prompt(false, &[]);
        assert!(prompt.contains("литературный"));
        assert!(prompt.contains("вежлив"));
        assert!(!prompt.contains("неформальный"));
    }

    #[test]
    fn optimization_prompt_hides_newline_commands_when_emulate_enter() {
        let prompt = optimization_system_prompt(true, &[]);
        assert!(prompt.contains("Команды перевода строки"));
        assert!(!prompt.contains("«новая строка» / «абзац» → перевод строки"));
    }

    #[test]
    fn optimization_user_message_uses_few_shot_layout() {
        assert_eq!(
            format_optimization_user_message("сколько будет два плюс два"),
            "Вход: сколько будет два плюс два\nВыход:"
        );
    }
}
