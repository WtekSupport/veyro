use crate::word_align::WordTiming;

use super::config::SpeechAnalysisConfig;
use super::types::RepetitionKind;

pub struct AdjacentRepeat {
    pub token: String,
    pub context: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub kind: RepetitionKind,
}

pub fn classify_word_repeat(
    raw_a: &str,
    raw_b: &str,
    gap_ms: u64,
    config: &SpeechAnalysisConfig,
) -> Option<RepetitionKind> {
    let a = normalize_token(raw_a);
    let b = normalize_token(raw_b);
    if a.is_empty() || a != b {
        return None;
    }
    if raw_a.contains('-') || raw_b.contains('-') {
        return Some(RepetitionKind::Emphasis);
    }
    if ends_clause_boundary(raw_a) || gap_ms >= config.long_pause_ms {
        return Some(RepetitionKind::PossibleDeliberate);
    }
    if gap_ms <= 2500 && !ends_sentence_boundary(raw_a) {
        return Some(RepetitionKind::Stutter);
    }
    if ends_sentence_boundary(raw_a) && gap_ms <= 1200 {
        return Some(RepetitionKind::Stutter);
    }
    Some(RepetitionKind::Stutter)
}

pub fn count_scored_repetitions(words: &[WordTiming], config: &SpeechAnalysisConfig) -> usize {
    if words.len() < 2 {
        return 0;
    }
    let mut reps = 0usize;
    for window in words.windows(2) {
        let gap = window[1]
            .start_ms
            .saturating_sub(window[0].end_ms);
        if classify_word_repeat(&window[0].text, &window[1].text, gap, config)
            == Some(RepetitionKind::Stutter)
        {
            reps += 1;
        }
    }
    reps
}

pub fn find_repetition_examples(
    words: &[WordTiming],
    transcript: &str,
    config: &SpeechAnalysisConfig,
    limit: usize,
) -> Vec<AdjacentRepeat> {
    let mut out = Vec::new();
    if words.len() >= 2 {
        for window in words.windows(2) {
            let gap = window[1]
                .start_ms
                .saturating_sub(window[0].end_ms);
            let Some(kind) = classify_word_repeat(&window[0].text, &window[1].text, gap, config)
            else {
                continue;
            };
            if kind == RepetitionKind::PossibleDeliberate {
                continue;
            }
            let token = normalize_token(&window[0].text);
            out.push(AdjacentRepeat {
                token,
                context: format!("{} {}", window[0].text, window[1].text),
                start_ms: window[0].start_ms,
                end_ms: window[1].end_ms,
                kind,
            });
            if out.len() >= limit {
                return out;
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    let tokens: Vec<&str> = transcript.split_whitespace().collect();
    for window in tokens.windows(2) {
        let Some(kind) = classify_word_repeat(window[0], window[1], 0, config) else {
            continue;
        };
        if kind == RepetitionKind::PossibleDeliberate {
            continue;
        }
        out.push(AdjacentRepeat {
            token: normalize_token(window[0]),
            context: format!("{} {}", window[0], window[1]),
            start_ms: 0,
            end_ms: 0,
            kind,
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn ends_sentence_boundary(raw: &str) -> bool {
    raw.trim_end()
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, '.' | '!' | '?' | '…' | ':' | ';'))
}

fn ends_clause_boundary(raw: &str) -> bool {
    if ends_sentence_boundary(raw) {
        return true;
    }
    raw.trim_end()
        .chars()
        .last()
        .is_some_and(|ch| ch == ',')
}

fn normalize_token(raw: &str) -> String {
    raw.trim()
        .trim_matches(|ch: char| !ch.is_alphanumeric())
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::word_align::WordTiming;

    #[test]
    fn sentence_boundary_repeat_not_stutter() {
        let config = SpeechAnalysisConfig::default();
        assert_eq!(
            classify_word_repeat("Есть.", "Есть.", 0, &config),
            Some(RepetitionKind::PossibleDeliberate)
        );
    }

    #[test]
    fn comma_repeat_not_stutter() {
        let config = SpeechAnalysisConfig::default();
        assert_eq!(
            classify_word_repeat("лавировали,", "лавировали", 100, &config),
            Some(RepetitionKind::PossibleDeliberate)
        );
    }

    #[test]
    fn pause_repeat_not_stutter() {
        let config = SpeechAnalysisConfig::default();
        assert_eq!(
            classify_word_repeat("есть", "есть", 400, &config),
            Some(RepetitionKind::PossibleDeliberate)
        );
    }

    #[test]
    fn tight_repeat_is_stutter() {
        let config = SpeechAnalysisConfig::default();
        let words = vec![
            WordTiming {
                text: "есть".into(),
                start_ms: 1000,
                end_ms: 1200,
            },
            WordTiming {
                text: "есть".into(),
                start_ms: 1250,
                end_ms: 1400,
            },
        ];
        assert_eq!(count_scored_repetitions(&words, &config), 1);
    }
}
