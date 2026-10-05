use super::types::{ReferenceEvalReport, ReferenceWordAlignment, SubstitutionPair};

pub fn evaluate_reference(
    reference: &str,
    hypothesis: &str,
    language_ru: bool,
) -> ReferenceEvalReport {
    let ref_words = tokenize_for_eval(reference, language_ru);
    let hyp_words = tokenize_for_eval(hypothesis, language_ru);
    let (alignment, edits) = align_words(&ref_words, &hyp_words);
    let ref_len = ref_words.len().max(1);
    let wer = edits as f32 / ref_len as f32 * 100.0;
    let ref_chars: String = ref_words.join("");
    let hyp_chars: String = hyp_words.join("");
    let char_edits = char_levenshtein(&ref_chars, &hyp_chars);
    let cer = char_edits as f32 / ref_chars.chars().count().max(1) as f32 * 100.0;

    let substitutions = collect_substitutions(&alignment);

    ReferenceEvalReport {
        reference_word_count: ref_words.len(),
        hypothesis_word_count: hyp_words.len(),
        wer_percent: wer,
        cer_percent: cer,
        substitutions,
        alignment,
    }
}

fn tokenize_for_eval(text: &str, language_ru: bool) -> Vec<String> {
    text.split_whitespace()
        .map(|raw| normalize_eval_token(raw, language_ru))
        .filter(|t| !t.is_empty())
        .collect()
}

fn normalize_eval_token(raw: &str, language_ru: bool) -> String {
    let trimmed = raw
        .trim()
        .trim_matches(|c: char| {
            c == ','
                || c == '.'
                || c == '!'
                || c == '?'
                || c == ';'
                || c == ':'
                || c == '"'
                || c == '«'
                || c == '»'
        });
    if language_ru {
        expand_ru_digits(trimmed)
    } else {
        trimmed.to_lowercase()
    }
}

fn expand_ru_digits(token: &str) -> String {
    if token.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(n) = token.parse::<u32>() {
            return simple_ru_cardinal(n);
        }
    }
    token.to_lowercase()
}

fn simple_ru_cardinal(n: u32) -> String {
    match n {
        0 => "ноль".to_string(),
        1..=10 => [
            "один", "два", "три", "четыре", "пять", "шесть", "семь", "восемь", "девять", "десять",
        ][(n - 1) as usize]
        .to_string(),
        11..=19 => format!("{}", n),
        20..=99 => {
            let tens = n / 10;
            let ones = n % 10;
            let tens_word = match tens {
                2 => "двадцать",
                3 => "тридцать",
                4 => "сорок",
                5 => "пятьдесят",
                6 => "шестьдесят",
                7 => "семьдесят",
                8 => "восемьдесят",
                9 => "девяносто",
                _ => return n.to_string(),
            };
            if ones == 0 {
                tens_word.to_string()
            } else {
                format!("{tens_word} {}", simple_ru_cardinal(ones))
            }
        }
        _ => n.to_string(),
    }
}

fn align_words(ref_words: &[String], hyp_words: &[String]) -> (Vec<ReferenceWordAlignment>, usize) {
    let n = ref_words.len();
    let m = hyp_words.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        dp[i][0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = if ref_words[i - 1] == hyp_words[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    let mut alignment = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 {
            let cost = if ref_words[i - 1] == hyp_words[j - 1] {
                0
            } else {
                1
            };
            if dp[i][j] == dp[i - 1][j - 1] + cost {
                alignment.push(ReferenceWordAlignment {
                    reference: Some(ref_words[i - 1].clone()),
                    hypothesis: Some(hyp_words[j - 1].clone()),
                    matched: cost == 0,
                });
                i -= 1;
                j -= 1;
                continue;
            }
        }
        if i > 0 && dp[i][j] == dp[i - 1][j] + 1 {
            alignment.push(ReferenceWordAlignment {
                reference: Some(ref_words[i - 1].clone()),
                hypothesis: None,
                matched: false,
            });
            i -= 1;
        } else if j > 0 {
            alignment.push(ReferenceWordAlignment {
                reference: None,
                hypothesis: Some(hyp_words[j - 1].clone()),
                matched: false,
            });
            j -= 1;
        }
    }
    alignment.reverse();
    (alignment, dp[n][m])
}

fn collect_substitutions(alignment: &[ReferenceWordAlignment]) -> Vec<SubstitutionPair> {
    let mut counts: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for row in alignment {
        if row.matched {
            continue;
        }
        let expected = row.reference.clone().unwrap_or_default();
        let observed = row.hypothesis.clone().unwrap_or_default();
        if expected.is_empty() || observed.is_empty() {
            continue;
        }
        *counts.entry((expected, observed)).or_default() += 1;
    }
    let mut out: Vec<SubstitutionPair> = counts
        .into_iter()
        .map(|((expected, observed), count)| SubstitutionPair {
            expected,
            observed,
            count,
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count));
    out.truncate(12);
    out
}

fn char_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        dp[i][0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[n][m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_liguria_substitution() {
        let ev = evaluate_reference(
            "в Лигурии регулировщик",
            "в Лигории регулировщик",
            true,
        );
        assert!(
            ev.substitutions.iter().any(|s| {
                s.expected.contains("лигурии") && s.observed.contains("лигории")
            }) || ev.wer_percent > 0.0,
            "{:?}",
            ev.substitutions
        );
    }
}
