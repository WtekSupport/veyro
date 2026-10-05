use crate::audio::segment::AudioSegment;

use super::types::{AlignError, WordTiming};

/// CTC forced alignment: uniform word timing over speech duration when no coarse segments exist.
pub fn ctc_forced_align(audio: &AudioSegment, full_text: &str) -> Result<Vec<WordTiming>, AlignError> {
    let words: Vec<&str> = full_text.split_whitespace().collect();
    if words.is_empty() {
        return Err(AlignError::EmptyTranscript);
    }
    let duration_ms = audio.duration_ms.max(1);
    let step = duration_ms / words.len() as u64;
    let mut timings = Vec::with_capacity(words.len());
    let mut cursor = 0u64;
    for (index, word) in words.iter().enumerate() {
        let end = if index + 1 == words.len() {
            duration_ms
        } else {
            cursor.saturating_add(step)
        };
        timings.push(WordTiming {
            text: word.to_string(),
            start_ms: cursor,
            end_ms: end,
        });
        cursor = end;
    }
    Ok(timings)
}
