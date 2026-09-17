//! Russian cardinal numerals with case inflection for «Числа прописью» (rusnum-style, MIT).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumeralCase {
    Nom,
    Gen,
    Dat,
    Acc,
    Ins,
    Prep,
}

pub fn int_to_words_ru(n: i64, case: NumeralCase) -> String {
    match case {
        NumeralCase::Nom | NumeralCase::Acc => chislo::int_to_words(n),
        NumeralCase::Gen | NumeralCase::Prep => int_to_genitive_cardinal(n),
        NumeralCase::Dat => int_to_dative_cardinal(n),
        NumeralCase::Ins => int_to_instrumental_cardinal(n),
    }
}

pub fn resolve_numeral_case(text_before: &str) -> NumeralCase {
    let lower = text_before.to_lowercase();
    let trimmed = lower.trim_end();
    let mut triggers: Vec<&ContextTrigger> = CONTEXT_TRIGGERS.iter().collect();
    triggers.sort_by_key(|t| std::cmp::Reverse(t.phrase.len()));
    for trigger in triggers {
        if trimmed.ends_with(trigger.phrase) {
            let before = trimmed
                .strip_suffix(trigger.phrase)
                .unwrap_or(trimmed)
                .trim_end();
            if before.ends_with(|c: char| c.is_alphabetic()) {
                continue;
            }
            return trigger.case;
        }
    }
    NumeralCase::Nom
}

struct ContextTrigger {
    phrase: &'static str,
    case: NumeralCase,
}

const CONTEXT_TRIGGERS: &[ContextTrigger] = &[
    ContextTrigger {
        phrase: "в районе",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "из-за",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "из-под",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "благодаря",
        case: NumeralCase::Dat,
    },
    ContextTrigger {
        phrase: "согласно",
        case: NumeralCase::Dat,
    },
    ContextTrigger {
        phrase: "вопреки",
        case: NumeralCase::Dat,
    },
    ContextTrigger {
        phrase: "навстречу",
        case: NumeralCase::Dat,
    },
    ContextTrigger {
        phrase: "вместе с",
        case: NumeralCase::Ins,
    },
    ContextTrigger {
        phrase: "рядом с",
        case: NumeralCase::Ins,
    },
    ContextTrigger {
        phrase: "перед",
        case: NumeralCase::Ins,
    },
    ContextTrigger {
        phrase: "между",
        case: NumeralCase::Ins,
    },
    ContextTrigger {
        phrase: "под",
        case: NumeralCase::Ins,
    },
    ContextTrigger {
        phrase: "над",
        case: NumeralCase::Ins,
    },
    ContextTrigger {
        phrase: "через",
        case: NumeralCase::Acc,
    },
    ContextTrigger {
        phrase: "про",
        case: NumeralCase::Acc,
    },
    ContextTrigger {
        phrase: "об",
        case: NumeralCase::Prep,
    },
    ContextTrigger {
        phrase: "при",
        case: NumeralCase::Prep,
    },
    ContextTrigger {
        phrase: "до",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "от",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "из",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "без",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "около",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "возле",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "вокруг",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "более",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "менее",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "свыше",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "против",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "после",
        case: NumeralCase::Gen,
    },
    ContextTrigger {
        phrase: "ко",
        case: NumeralCase::Dat,
    },
    ContextTrigger {
        phrase: "к",
        case: NumeralCase::Dat,
    },
    ContextTrigger {
        phrase: "со",
        case: NumeralCase::Ins,
    },
    ContextTrigger {
        phrase: "с",
        case: NumeralCase::Ins,
    },
    ContextTrigger {
        phrase: "о",
        case: NumeralCase::Prep,
    },
    ContextTrigger {
        phrase: "в",
        case: NumeralCase::Prep,
    },
    ContextTrigger {
        phrase: "на",
        case: NumeralCase::Prep,
    },
    ContextTrigger {
        phrase: "за",
        case: NumeralCase::Acc,
    },
    ContextTrigger {
        phrase: "по",
        case: NumeralCase::Prep,
    },
];

const ONES_GEN_M: [&str; 9] = [
    "одного",
    "двух",
    "трёх",
    "четырёх",
    "пяти",
    "шести",
    "семи",
    "восьми",
    "девяти",
];

const ONES_GEN_F: [&str; 9] = [
    "одной",
    "двух",
    "трёх",
    "четырёх",
    "пяти",
    "шести",
    "семи",
    "восьми",
    "девяти",
];

const TEENS_GEN: [&str; 10] = [
    "десяти",
    "одиннадцати",
    "двенадцати",
    "тринадцати",
    "четырнадцати",
    "пятнадцати",
    "шестнадцати",
    "семнадцати",
    "восемнадцати",
    "девятнадцати",
];

const TENS_GEN: [&str; 10] = [
    "",
    "десяти",
    "двадцати",
    "тридцати",
    "сорока",
    "пятидесяти",
    "шестидесяти",
    "семидесяти",
    "восьмидесяти",
    "девяноста",
];

const HUNDREDS_GEN: [&str; 9] = [
    "ста",
    "двухсот",
    "трёхсот",
    "четырёхсот",
    "пятисот",
    "шестисот",
    "семисот",
    "восьмисот",
    "девятисот",
];

const ORDERS_GEN: [[&str; 3]; 14] = [
    ["", "", ""],
    ["тысячи", "тысяч", "тысяч"],
    ["миллиона", "миллионов", "миллионов"],
    ["миллиарда", "миллиардов", "миллиардов"],
    ["триллиона", "триллионов", "триллионов"],
    ["квадриллиона", "квадриллионов", "квадриллионов"],
    ["квинтиллиона", "квинтиллионов", "квинтиллионов"],
    ["секстиллиона", "секстиллионов", "секстиллионов"],
    ["септиллиона", "септиллионов", "септиллионов"],
    ["октиллиона", "октиллионов", "октиллионов"],
    ["нониллиона", "нониллионов", "нониллионов"],
    ["дециллиона", "дециллионов", "дециллионов"],
    ["ундециллиона", "ундециллионов", "ундециллионов"],
    ["дуодециллиона", "дуодециллионов", "дуодециллионов"],
];

fn order_genitive_form(triad: u32, order: usize) -> &'static str {
    if order == 0 || order >= ORDERS_GEN.len() {
        return "";
    }
    let n = (triad % 100) as u64;
    if (11..=19).contains(&n) {
        return ORDERS_GEN[order][2];
    }
    match triad % 10 {
        1 => ORDERS_GEN[order][0],
        2..=4 => ORDERS_GEN[order][1],
        _ => ORDERS_GEN[order][2],
    }
}

fn triad_to_genitive_words(n: u32, order: usize) -> String {
    let h = (n / 100) as usize;
    let t = ((n % 100) / 10) as usize;
    let o = (n % 10) as usize;
    let mut parts: Vec<&str> = Vec::new();

    if h > 0 {
        parts.push(HUNDREDS_GEN[h - 1]);
    }

    if t == 1 {
        parts.push(TEENS_GEN[o]);
    } else {
        if t > 0 {
            parts.push(TENS_GEN[t]);
        }
        if o > 0 {
            let ones = if order == 1 {
                ONES_GEN_F[o - 1]
            } else {
                ONES_GEN_M[o - 1]
            };
            parts.push(ones);
        }
    }

    parts.join(" ")
}

pub fn int_to_genitive_cardinal(n: i64) -> String {
    if n == 0 {
        return "нуля".to_string();
    }

    let mut result = String::new();
    if n < 0 {
        result.push_str("минус ");
    }

    let mut abs_n = (n as i128).unsigned_abs() as u64;
    let mut parts: Vec<String> = Vec::new();
    let mut order: usize = 0;

    while abs_n > 0 {
        let triad = (abs_n % 1000) as u32;
        abs_n /= 1000;

        if triad != 0 {
            let mut part = triad_to_genitive_words(triad, order);
            if order > 0 {
                let order_word = order_genitive_form(triad, order);
                if !order_word.is_empty() {
                    if !part.is_empty() {
                        part.push(' ');
                    }
                    part.push_str(order_word);
                }
            }
            parts.push(part);
        }
        order += 1;
    }

    parts.reverse();
    result.push_str(&parts.join(" "));
    result
}

const ONES_DAT_M: [&str; 9] = [
    "одному",
    "двум",
    "трём",
    "четырём",
    "пяти",
    "шести",
    "семи",
    "восьми",
    "девяти",
];

const ONES_DAT_F: [&str; 9] = [
    "одной",
    "двум",
    "трём",
    "четырём",
    "пяти",
    "шести",
    "семи",
    "восьми",
    "девяти",
];

const TEENS_DAT: [&str; 10] = TEENS_GEN;
const TENS_DAT: [&str; 10] = [
    "",
    "десяти",
    "двадцати",
    "тридцати",
    "сорока",
    "пятидесяти",
    "шестидесяти",
    "семидесяти",
    "восьмидесяти",
    "девяноста",
];

const HUNDREDS_DAT: [&str; 9] = [
    "ста",
    "двумстам",
    "трёмстам",
    "четырёмстам",
    "пятистам",
    "шестистам",
    "семистам",
    "восьмистам",
    "девятистам",
];

const ORDERS_DAT: [[&str; 3]; 14] = [
    ["", "", ""],
    ["тысяче", "тысячам", "тысячам"],
    ["миллиону", "миллионам", "миллионам"],
    ["миллиарду", "миллиардам", "миллиардам"],
    ["триллиону", "триллионам", "триллионам"],
    ["квадриллиону", "квадриллионам", "квадриллионам"],
    ["квинтиллиону", "квинтиллионам", "квинтиллионам"],
    ["секстиллиону", "секстиллионам", "секстиллионам"],
    ["септиллиону", "септиллионам", "септиллионам"],
    ["октиллиону", "октиллионам", "октиллионам"],
    ["нониллиону", "нониллионам", "нониллионам"],
    ["дециллиону", "дециллионам", "дециллионам"],
    ["ундециллиону", "ундециллионам", "ундециллионам"],
    ["дуодециллиону", "дуодециллионам", "дуодециллионам"],
];

fn order_dative_form(triad: u32, order: usize) -> &'static str {
    if order == 0 || order >= ORDERS_DAT.len() {
        return "";
    }
    let n = (triad % 100) as u64;
    if (11..=19).contains(&n) {
        return ORDERS_DAT[order][2];
    }
    match triad % 10 {
        1 => ORDERS_DAT[order][0],
        2..=4 => ORDERS_DAT[order][1],
        _ => ORDERS_DAT[order][2],
    }
}

fn triad_to_dative_words(n: u32, order: usize) -> String {
    let h = (n / 100) as usize;
    let t = ((n % 100) / 10) as usize;
    let o = (n % 10) as usize;
    let mut parts: Vec<&str> = Vec::new();
    if h > 0 {
        parts.push(HUNDREDS_DAT[h - 1]);
    }
    if t == 1 {
        parts.push(TEENS_DAT[o]);
    } else {
        if t > 0 {
            parts.push(TENS_DAT[t]);
        }
        if o > 0 {
            let ones = if order == 1 {
                ONES_DAT_F[o - 1]
            } else {
                ONES_DAT_M[o - 1]
            };
            parts.push(ones);
        }
    }
    parts.join(" ")
}

pub fn int_to_dative_cardinal(n: i64) -> String {
    int_to_cardinal_with_triad(n, triad_to_dative_words, order_dative_form, "нулю")
}

const ONES_INS_M: [&str; 9] = [
    "одним",
    "двумя",
    "тремя",
    "четырьмя",
    "пятью",
    "шестью",
    "семью",
    "восемью",
    "девятью",
];

const ONES_INS_F: [&str; 9] = ONES_INS_M;
const TEENS_INS: [&str; 10] = [
    "десятью",
    "одиннадцатью",
    "двенадцатью",
    "тринадцатью",
    "четырнадцатью",
    "пятнадцатью",
    "шестнадцатью",
    "семнадцатью",
    "восемнадцатью",
    "девятнадцатью",
];

const TENS_INS: [&str; 10] = [
    "",
    "десятью",
    "двадцатью",
    "тридцатью",
    "сорока",
    "пятьюдесятью",
    "шестьюдесятью",
    "семьюдесятью",
    "восьмьюдесятью",
    "девяноста",
];

const HUNDREDS_INS: [&str; 9] = [
    "ста",
    "двумястами",
    "тремястами",
    "четырьмястами",
    "пятьюстами",
    "шестьюстами",
    "семьюстами",
    "восемьюстами",
    "девятьюстами",
];

const ORDERS_INS: [[&str; 3]; 14] = [
    ["", "", ""],
    ["тысячей", "тысячами", "тысячами"],
    ["миллионом", "миллионами", "миллионами"],
    ["миллиардом", "миллиардами", "миллиардами"],
    ["триллионом", "триллионами", "триллионами"],
    ["квадриллионом", "квадриллионами", "квадриллионами"],
    ["квинтиллионом", "квинтиллионами", "квинтиллионами"],
    ["секстиллионом", "секстиллионами", "секстиллионами"],
    ["септиллионом", "септиллионами", "септиллионами"],
    ["октиллионом", "октиллионами", "октиллионами"],
    ["нониллионом", "нониллионами", "нониллионами"],
    ["дециллионом", "дециллионами", "дециллионами"],
    ["ундециллионом", "ундециллионами", "ундециллионами"],
    ["дуодециллионом", "дуодециллионами", "дуодециллионами"],
];

fn order_instrumental_form(triad: u32, order: usize) -> &'static str {
    if order == 0 || order >= ORDERS_INS.len() {
        return "";
    }
    let n = (triad % 100) as u64;
    if (11..=19).contains(&n) {
        return ORDERS_INS[order][2];
    }
    match triad % 10 {
        1 => ORDERS_INS[order][0],
        2..=4 => ORDERS_INS[order][1],
        _ => ORDERS_INS[order][2],
    }
}

fn triad_to_instrumental_words(n: u32, order: usize) -> String {
    let h = (n / 100) as usize;
    let t = ((n % 100) / 10) as usize;
    let o = (n % 10) as usize;
    let mut parts: Vec<&str> = Vec::new();
    if h > 0 {
        parts.push(HUNDREDS_INS[h - 1]);
    }
    if t == 1 {
        parts.push(TEENS_INS[o]);
    } else {
        if t > 0 {
            parts.push(TENS_INS[t]);
        }
        if o > 0 {
            let ones = if order == 1 {
                ONES_INS_F[o - 1]
            } else {
                ONES_INS_M[o - 1]
            };
            parts.push(ones);
        }
    }
    parts.join(" ")
}

pub fn int_to_instrumental_cardinal(n: i64) -> String {
    int_to_cardinal_with_triad(n, triad_to_instrumental_words, order_instrumental_form, "нолём")
}

fn int_to_cardinal_with_triad(
    n: i64,
    triad_fn: fn(u32, usize) -> String,
    order_fn: fn(u32, usize) -> &'static str,
    zero_word: &str,
) -> String {
    if n == 0 {
        return zero_word.to_string();
    }
    let mut result = String::new();
    if n < 0 {
        result.push_str("минус ");
    }
    let mut abs_n = (n as i128).unsigned_abs() as u64;
    let mut parts: Vec<String> = Vec::new();
    let mut order: usize = 0;
    while abs_n > 0 {
        let triad = (abs_n % 1000) as u32;
        abs_n /= 1000;
        if triad != 0 {
            let mut part = triad_fn(triad, order);
            if order > 0 {
                let order_word = order_fn(triad, order);
                if !order_word.is_empty() {
                    if !part.is_empty() {
                        part.push(' ');
                    }
                    part.push_str(order_word);
                }
            }
            parts.push(part);
        }
        order += 1;
    }
    parts.reverse();
    result.push_str(&parts.join(" "));
    result
}

pub fn wants_genitive_numeral_context(text_before: &str) -> bool {
    matches!(resolve_numeral_case(text_before), NumeralCase::Gen)
}

/// Adjust numerals (digits or words) to the case implied by nearby prepositions.
pub fn apply_russian_numeral_inflection(text: &str) -> String {
    apply_russian_genitive_numerals(text)
}

/// Back-compat alias.
pub fn apply_russian_genitive_numerals(text: &str) -> String {
    let mut triggers: Vec<&ContextTrigger> = CONTEXT_TRIGGERS.iter().collect();
    triggers.sort_by_key(|t| std::cmp::Reverse(t.phrase.len()));

    let mut out = String::with_capacity(text.len());
    let mut byte_idx = 0;
    let bytes = text.as_bytes();

    while byte_idx < bytes.len() {
        let mut matched: Option<(usize, usize)> = None;
        let mut matched_case = NumeralCase::Nom;
        for trigger in &triggers {
            let rest = &text[byte_idx..];
            if !rest.to_lowercase().starts_with(trigger.phrase) {
                continue;
            }
            if !trigger_has_word_boundary(text, byte_idx) {
                continue;
            }
            let mut num_start = byte_idx + trigger.phrase.len();
            while num_start < bytes.len() && bytes[num_start].is_ascii_whitespace() {
                num_start += 1;
            }
            let chars: Vec<char> = text.chars().collect();
            let char_idx = text[..num_start].chars().count();
            if let Some((num_end_char, value)) = parse_number_at(&chars, char_idx) {
                if value >= 0 {
                    let num_end_byte = text
                        .char_indices()
                        .nth(num_end_char)
                        .map(|(i, _)| i)
                        .unwrap_or(text.len());
                    matched = Some((num_start, num_end_byte));
                    matched_case = trigger.case;
                    break;
                }
            }
        }

        if let Some((num_start, num_end)) = matched {
            out.push_str(&text[byte_idx..num_start]);
            let chars: Vec<char> = text.chars().collect();
            let char_idx = text[..num_start].chars().count();
            if let Some((_, value)) = parse_number_at(&chars, char_idx) {
                out.push_str(&int_to_words_ru(value, matched_case));
            }
            byte_idx = num_end;
            continue;
        }

        let ch = text[byte_idx..].chars().next().unwrap();
        out.push(ch);
        byte_idx += ch.len_utf8();
    }

    out
}

fn trigger_has_word_boundary(text: &str, trigger_start: usize) -> bool {
    if trigger_start == 0 {
        return true;
    }
    let before = text[..trigger_start].chars().rev().next();
    match before {
        None => true,
        Some(ch) => !ch.is_alphabetic(),
    }
}

fn is_cyrillic(ch: char) -> bool {
    matches!(ch, 'а'..='я' | 'А'..='Я' | 'ё' | 'Ё')
}

fn parse_number_at(chars: &[char], start: usize) -> Option<(usize, i64)> {
    if start >= chars.len() {
        return None;
    }

    if chars[start].is_ascii_digit() {
        let mut end = start;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
        }
        let digits: String = chars[start..end].iter().collect();
        let value = digits.parse::<i64>().ok()?;
        return Some((end, value));
    }

    if !is_cyrillic(chars[start]) {
        return None;
    }

    let mut end = start;
    let mut words: Vec<String> = Vec::new();
    while end < chars.len() {
        if chars[end].is_whitespace() {
            end += 1;
            continue;
        }
        if !is_cyrillic(chars[end]) {
            break;
        }
        let word_start = end;
        while end < chars.len() && is_cyrillic(chars[end]) {
            end += 1;
        }
        words.push(normalize_num_word(
            &chars[word_start..end].iter().collect::<String>(),
        ));
    }

    let value = parse_russian_cardinal_words(&words)?;
    Some((end, value))
}

fn normalize_num_word(word: &str) -> String {
    word.to_lowercase().replace('ё', "е")
}

fn parse_russian_cardinal_words(words: &[String]) -> Option<i64> {
    if words.is_empty() {
        return None;
    }

    static UNITS: &[(&str, i64)] = &[
        ("ноль", 0),
        ("один", 1),
        ("одна", 1),
        ("одно", 1),
        ("два", 2),
        ("две", 2),
        ("три", 3),
        ("четыре", 4),
        ("пять", 5),
        ("шесть", 6),
        ("семь", 7),
        ("восемь", 8),
        ("девять", 9),
        ("десять", 10),
        ("одиннадцать", 11),
        ("двенадцать", 12),
        ("тринадцать", 13),
        ("четырнадцать", 14),
        ("пятнадцать", 15),
        ("шестнадцать", 16),
        ("семнадцать", 17),
        ("восемнадцать", 18),
        ("девятнадцать", 19),
        ("двадцать", 20),
        ("тридцать", 30),
        ("сорок", 40),
        ("пятьдесят", 50),
        ("шестьдесят", 60),
        ("семьдесят", 70),
        ("восемьдесят", 80),
        ("девяносто", 90),
        ("сто", 100),
        ("двести", 200),
        ("триста", 300),
        ("четыреста", 400),
        ("пятьсот", 500),
        ("шестьсот", 600),
        ("семьсот", 700),
        ("восемьсот", 800),
        ("девятьсот", 900),
    ];

    static MULTS: &[(&str, i64)] = &[
        ("тысяча", 1_000),
        ("тысячи", 1_000),
        ("тысяч", 1_000),
        ("миллион", 1_000_000),
        ("миллиона", 1_000_000),
        ("миллионов", 1_000_000),
        ("миллиард", 1_000_000_000),
        ("миллиарда", 1_000_000_000),
        ("миллиардов", 1_000_000_000),
    ];

    let mut total: i64 = 0;
    let mut current: i64 = 0;

    for word in words {
        if let Some(&value) = UNITS.iter().find(|(w, _)| *w == word.as_str()).map(|(_, v)| v) {
            current += value;
            continue;
        }
        if let Some(&mult) = MULTS.iter().find(|(w, _)| *w == word.as_str()).map(|(_, v)| v) {
            if current == 0 {
                current = 1;
            }
            total += current.saturating_mul(mult);
            current = 0;
            continue;
        }
        return None;
    }

    Some(total + current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genitive_cardinal_round_tens() {
        assert_eq!(int_to_genitive_cardinal(20), "двадцати");
        assert_eq!(int_to_genitive_cardinal(21), "двадцати одного");
        assert_eq!(int_to_genitive_cardinal(5), "пяти");
    }

    #[test]
    fn fix_after_do_and_region() {
        let text = "до двадцать долларов сейчас в районе двадцать долларов";
        let fixed = apply_russian_genitive_numerals(text);
        assert!(fixed.contains("до двадцати долларов"));
        assert!(fixed.contains("в районе двадцати долларов"));
    }

    #[test]
    fn fix_digits_after_do() {
        let text = "до 20 долларов";
        let fixed = apply_russian_genitive_numerals(text);
        assert_eq!(fixed, "до двадцати долларов");
    }

    #[test]
    fn wants_genitive_context() {
        assert!(wants_genitive_numeral_context("оплата до "));
        assert!(wants_genitive_numeral_context("стоит в районе "));
        assert!(!wants_genitive_numeral_context("сейчас "));
    }

    #[test]
    fn dative_and_instrumental_two() {
        assert_eq!(int_to_dative_cardinal(2), "двум");
        assert_eq!(int_to_instrumental_cardinal(2), "двумя");
    }

    #[test]
    fn resolve_case_triggers() {
        assert_eq!(resolve_numeral_case("иду к "), NumeralCase::Dat);
        assert_eq!(resolve_numeral_case("работаю с "), NumeralCase::Ins);
        assert_eq!(resolve_numeral_case("думаю о "), NumeralCase::Prep);
    }

    #[test]
    fn dictation_dollars_example() {
        let text = "Как я уже говорил, до двадцать долларов сейчас считается оплата вполне нормальной. Сейчас нейросеть любая стоит в районе двадцать долларов в месяц.";
        let fixed = apply_russian_genitive_numerals(text);
        assert!(fixed.contains("до двадцати долларов"));
        assert!(fixed.contains("в районе двадцати долларов"));
    }
}
