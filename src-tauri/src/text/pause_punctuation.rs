use crate::timed_text::TimedTextSegment;

/// Gap between Whisper segments above this → sentence boundary (`.`).
const SENTENCE_GAP_MS: u64 = 900;
/// Gap above this → clause boundary (`,`).
const COMMA_GAP_MS: u64 = 450;

const QUESTION_TAIL_WORDS: &[&str] = &["ли", "разве", "неужели", "нет"];

/// Join Whisper segments with punctuation inferred from inter-segment pauses.
pub fn apply_pause_punctuation(segments: &[TimedTextSegment]) -> String {
    let segments: Vec<_> = segments
        .iter()
        .map(|segment| TimedTextSegment {
            text: segment.text.trim().to_string(),
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
        })
        .filter(|segment| !segment.text.is_empty())
        .collect();

    if segments.is_empty() {
        return String::new();
    }
    if segments.len() == 1 {
        return maybe_question_suffix(&segments[0].text);
    }

    let mut out = String::new();
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            let prev = &segments[index - 1];
            let gap = segment.start_ms.saturating_sub(prev.end_ms);
            let separator = if gap >= SENTENCE_GAP_MS {
                ". "
            } else if gap >= COMMA_GAP_MS {
                ", "
            } else {
                " "
            };
            if separator == ". " && out.ends_with(',') {
                out.pop();
                if out.ends_with(' ') {
                    out.pop();
                }
            }
            out.push_str(separator);
        }
        out.push_str(&segment.text);
    }

    let trimmed = out.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let with_terminal = if trimmed.ends_with(['.', '!', '?', ',', '…']) {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    };

    maybe_question_suffix(&with_terminal)
}

fn maybe_question_suffix(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let Some(last_word) = words.last() else {
        return text.to_string();
    };
    let normalized = last_word
        .trim_matches(|ch: char| !ch.is_alphabetic())
        .to_ascii_lowercase();
    if QUESTION_TAIL_WORDS.contains(&normalized.as_str()) && !text.ends_with('?') {
        let base = text.trim_end_matches('.');
        return format!("{base}?");
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(text: &str, start: u64, end: u64) -> TimedTextSegment {
        TimedTextSegment {
            text: text.to_string(),
            start_ms: start,
            end_ms: end,
        }
    }

    #[test]
    fn long_gap_inserts_sentence_boundary() {
        let text = apply_pause_punctuation(&[
            seg("первое", 0, 500),
            seg("второе", 1500, 2000),
        ]);
        assert_eq!(text, "первое. второе.");
    }

    #[test]
    fn medium_gap_inserts_comma() {
        let text = apply_pause_punctuation(&[
            seg("один", 0, 400),
            seg("два", 900, 1200),
        ]);
        assert_eq!(text, "один, два.");
    }

    #[test]
    fn short_gap_keeps_space_only() {
        let text = apply_pause_punctuation(&[
            seg("раз", 0, 300),
            seg("два", 350, 600),
        ]);
        assert_eq!(text, "раз два.");
    }

    #[test]
    fn question_hint_for_li() {
        let text = apply_pause_punctuation(&[seg("ты придёшь ли", 0, 800)]);
        assert_eq!(text, "ты придёшь ли?");
    }
}
