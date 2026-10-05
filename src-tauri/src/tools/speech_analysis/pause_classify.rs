use crate::timed_text::TimedWord;

use super::pause_energy::PauseInterval;

const PUNCT_END: &[char] = &['.', '!', '?', '…', ':', ';'];

pub struct ClassifiedPauses {
    pub punctuation: usize,
    pub mid_phrase: usize,
    pub very_long: usize,
}

pub fn classify_pauses(
    pauses: &[PauseInterval],
    words: &[TimedWord],
    very_long_ms: u64,
) -> ClassifiedPauses {
    let mut punctuation = 0usize;
    let mut mid_phrase = 0usize;
    let mut very_long = 0usize;

    for pause in pauses {
        if pause.duration_ms >= very_long_ms {
            very_long += 1;
        }
        if pause_follows_punctuation(pause.start_ms, words) {
            punctuation += 1;
        } else {
            mid_phrase += 1;
        }
    }

    ClassifiedPauses {
        punctuation,
        mid_phrase,
        very_long,
    }
}

fn pause_follows_punctuation(pause_start_ms: u64, words: &[TimedWord]) -> bool {
    let Some(before) = words
        .iter()
        .filter(|w| w.end_ms <= pause_start_ms.saturating_add(80))
        .max_by_key(|w| w.end_ms)
    else {
        return false;
    };
    let trimmed = before.text.trim();
    if trimmed.ends_with(PUNCT_END) {
        return true;
    }
    let after = words.iter().find(|w| w.start_ms >= pause_start_ms.saturating_sub(40));
    if let Some(next) = after {
        let first = next.text.chars().next();
        if first.is_some_and(|c| c.is_uppercase()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timed_text::TimedWord;

    #[test]
    fn pause_after_period_is_punctuation() {
        let words = vec![TimedWord {
            text: "Привет.".to_string(),
            start_ms: 0,
            end_ms: 400,
            confidence: None,
        }];
        let pauses = vec![PauseInterval {
            start_ms: 420,
            duration_ms: 300,
        }];
        let c = classify_pauses(&pauses, &words, 1500);
        assert_eq!(c.punctuation, 1);
        assert_eq!(c.mid_phrase, 0);
    }
}
