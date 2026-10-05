use std::collections::HashMap;

use super::phonology::{letters_phonologically_equivalent, substitution_is_meaningful};
use super::types::SubstitutionPair;

pub fn phoneme_error_rate(expected: &[String], observed: &[String]) -> f32 {
    if expected.is_empty() {
        return 0.0;
    }
    let distance = levenshtein(expected, observed);
    distance as f32 / expected.len() as f32
}

pub fn top_substitutions(
    expected: &[String],
    observed: &[String],
    top_n: usize,
    min_count: usize,
) -> Vec<SubstitutionPair> {
    let ops = alignment_ops(expected, observed);
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for AlignOp::Substitute { from, to } in ops {
        if substitution_is_meaningful(&from, &to) {
            *counts.entry((from, to)).or_default() += 1;
        }
    }
    let mut pairs: Vec<SubstitutionPair> = counts
        .into_iter()
        .filter(|(_, count)| *count >= min_count)
        .map(|((expected, observed), count)| SubstitutionPair {
            expected,
            observed,
            count,
        })
        .collect();
    pairs.sort_by(|a, b| b.count.cmp(&a.count));
    pairs.truncate(top_n);
    pairs
}

enum AlignOp {
    Substitute { from: String, to: String },
}

fn alignment_ops(expected: &[String], observed: &[String]) -> Vec<AlignOp> {
    let mut ops = Vec::new();
    let matrix = levenshtein_backtrace(expected, observed);
    for (left, right) in matrix {
        ops.push(AlignOp::Substitute {
            from: left,
            to: right,
        });
    }
    ops
}

fn levenshtein(expected: &[String], observed: &[String]) -> usize {
    let n = expected.len();
    let m = observed.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        dp[i][0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = if letters_phonologically_equivalent(&expected[i - 1], &observed[j - 1]) {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[n][m]
}

fn levenshtein_backtrace(expected: &[String], observed: &[String]) -> Vec<(String, String)> {
    let n = expected.len();
    let m = observed.len();
    if n == 0 {
        return observed
            .iter()
            .map(|token| (String::new(), token.clone()))
            .collect();
    }
    if m == 0 {
        return expected
            .iter()
            .map(|token| (token.clone(), String::new()))
            .collect();
    }

    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        dp[i][0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = if letters_phonologically_equivalent(&expected[i - 1], &observed[j - 1]) {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    let mut i = n;
    let mut j = m;
    let mut pairs = Vec::new();
    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && dp[i][j]
                == dp[i - 1][j - 1]
                    + if letters_phonologically_equivalent(&expected[i - 1], &observed[j - 1]) {
                        0
                    } else {
                        1
                    }
        {
            pairs.push((expected[i - 1].clone(), observed[j - 1].clone()));
            i -= 1;
            j -= 1;
        } else if i > 0 && dp[i][j] == dp[i - 1][j] + 1 {
            pairs.push((expected[i - 1].clone(), String::new()));
            i -= 1;
        } else if j > 0 && dp[i][j] == dp[i][j - 1] + 1 {
            pairs.push((String::new(), observed[j - 1].clone()));
            j -= 1;
        } else {
            break;
        }
    }
    pairs.reverse();
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_zero_when_equal() {
        let tokens = vec!["a".into(), "b".into()];
        assert!((phoneme_error_rate(&tokens, &tokens) - 0.0).abs() < 0.001);
    }
}
