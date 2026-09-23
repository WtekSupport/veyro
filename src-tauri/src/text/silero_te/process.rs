use std::collections::HashSet;

pub fn build_uni_symbols(uni_vocab: &[String]) -> HashSet<char> {
    let mut symbols = HashSet::new();
    for unitoken in uni_vocab {
        for ch in unitoken_into_token(unitoken).chars() {
            symbols.insert(ch);
        }
    }
    symbols
}

pub fn process_unicode(text: &str, uni_symbols: &HashSet<char>) -> String {
    let mut processed = String::with_capacity(text.len());
    for c in text.chars() {
        if (c as u32) < 127 {
            processed.push(c);
        } else if !uni_symbols.contains(&c) {
            processed.push('&');
        } else {
            processed.push('{');
            processed.push_str(&format!("{}", c as u32));
            processed.push('}');
        }
    }
    processed
}

pub fn unitoken_into_token(unitoken: &str) -> String {
    split_into_chars(unitoken)
        .into_iter()
        .map(|part| {
            if is_transformed_char(&part) && part.len() >= 2 {
                char::from_u32(part[1..part.len() - 1].parse().unwrap_or(0)).unwrap_or('?')
            } else {
                part.chars().next().unwrap_or(' ')
            }
        })
        .collect()
}

fn is_transformed_char(part: &str) -> bool {
    part.starts_with('{')
        && part.ends_with('}')
        && part[1..part.len() - 1].chars().all(|c| c.is_ascii_digit())
}

fn split_into_chars(text: &str) -> Vec<String> {
    let mut splitted = Vec::new();
    let mut char_buf = String::new();
    let mut uni_start = false;

    for c in text.chars() {
        if !uni_start {
            if c != '{' {
                splitted.push(c.to_string());
            } else {
                char_buf.push(c);
                uni_start = true;
            }
        } else if c.is_ascii_digit() {
            char_buf.push(c);
        } else if c == '}' {
            char_buf.push(c);
            splitted.push(char_buf.clone());
            char_buf.clear();
            uni_start = false;
        } else {
            char_buf.push(c);
        }
    }

    splitted
}

pub fn enhance_tokens(
    tokens: &[String],
    punct: &[i64],
    capital: &[i64],
    index2punct: &[(i64, char)],
) -> Vec<String> {
    let mut output = Vec::new();
    let mut sentence_end = false;

    for (index, token) in tokens.iter().enumerate() {
        let mut c = capital.get(index).copied().unwrap_or(0);
        let p = punct.get(index).copied().unwrap_or(0);
        if sentence_end {
            if c == 0 {
                c = 1;
            }
            sentence_end = false;
        }

        let mut token = token.clone();
        if c == 1 {
            if token
                .chars()
                .next()
                .is_some_and(|ch| ch.is_alphanumeric())
            {
                let mut chars = token.chars();
                if let Some(first) = chars.next() {
                    token = first.to_uppercase().collect::<String>() + chars.as_str();
                }
            } else if token.starts_with("##") && token.len() > 2 {
                let rest: String = token.chars().skip(2).collect();
                let mut chars = rest.chars();
                if let Some(first) = chars.next() {
                    token = format!("##{}{}", first.to_uppercase(), chars.as_str());
                }
            }
        } else if c == 2 {
            token = token.to_uppercase();
        }

        output.push(token);

        if p > 0 {
            if let Some((_, symbol)) = index2punct.iter().find(|(idx, _)| *idx == p) {
                output.push(symbol.to_string());
                if matches!(symbol, '.' | '!' | '?') {
                    sentence_end = true;
                }
            }
        }
    }

    output
}

pub fn language_code(language: &str) -> &'static str {
    let lang = language.split('-').next().unwrap_or(language).to_ascii_lowercase();
    match lang.as_str() {
        "de" => "de",
        "es" => "es",
        "ru" => "ru",
        _ => "en",
    }
}

pub fn language_index(code: &str) -> i64 {
    match code {
        "de" => 1,
        "es" => 2,
        "ru" => 3,
        _ => 0,
    }
}
