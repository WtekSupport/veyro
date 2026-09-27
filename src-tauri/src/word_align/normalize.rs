use crate::timed_text::TimedTextSegment;

use super::types::{CoarseSegment, UnifiedInput};

/// Stage 0: map arbitrary ASR output to a single shape before alignment.
pub fn unified_from_timed_segments(segments: &[TimedTextSegment]) -> UnifiedInput {
    let coarse: Vec<CoarseSegment> = segments
        .iter()
        .filter_map(|segment| {
            let text = segment.text.trim();
            if text.is_empty() {
                return None;
            }
            Some(CoarseSegment {
                text: text.to_string(),
                start_ms: segment.start_ms,
                end_ms: segment.end_ms.max(segment.start_ms),
            })
        })
        .collect();

    let full_text = coarse
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    UnifiedInput {
        full_text,
        coarse_segments: if coarse.is_empty() {
            None
        } else {
            Some(coarse)
        },
    }
}

/// Words for alignment inside one coarse segment (whitespace split; hyphens/apostrophes stay intact).
pub fn segment_words_for_alignment(segment_text: &str) -> Vec<String> {
    segment_text
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timed_text::TimedTextSegment;

    #[test]
    fn unified_joins_segment_text() {
        let input = unified_from_timed_segments(&[
            TimedTextSegment {
                text: "Hello world".to_string(),
                start_ms: 0,
                end_ms: 1000,
                words: Vec::new(),
            },
            TimedTextSegment {
                text: "again".to_string(),
                start_ms: 1500,
                end_ms: 2000,
                words: Vec::new(),
            },
        ]);
        assert_eq!(input.full_text, "Hello world again");
        assert_eq!(input.coarse_segments.as_ref().unwrap().len(), 2);
    }
}
