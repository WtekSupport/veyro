use crate::audio::segment::AudioSegment;
use crate::timed_text::{TimedTextSegment, TimedWord};
use crate::word_align::{align, unified_from_timed_segments, EnergyAlignConfig, WordTiming};

use super::phonemize::phonemize_word;
use super::types::PhonemeSegment;

fn words_from_segment_text(segment_text: &str) -> Vec<String> {
    segment_text
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Sherpa and some other providers return plain text only. Build one coarse segment so
/// metrics use the same word inventory as `report.transcript`.
pub fn ensure_timed_segments_from_transcript(
    segments: &mut Vec<TimedTextSegment>,
    transcript: &str,
    speech_start_ms: u64,
    speech_end_ms: u64,
) {
    if segments.is_empty() {
        let text = transcript.trim();
        if !text.is_empty() {
            let end_ms = speech_end_ms.max(speech_start_ms.saturating_add(1));
            segments.push(TimedTextSegment {
                text: text.to_string(),
                start_ms: speech_start_ms,
                end_ms,
                words: Vec::new(),
            });
        }
    }
    sync_segment_text_from_words(segments);
    ensure_segment_word_timings(segments);
}

fn sync_segment_text_from_words(segments: &mut [TimedTextSegment]) {
    for segment in segments {
        if segment.text.trim().is_empty() && !segment.words.is_empty() {
            segment.text = segment
                .words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
        }
    }
}

/// When STT returns segment text without per-word timings, synthesize coarse word spans
/// so fluency, intelligibility, and articulation see the same word count as the transcript.
pub fn ensure_segment_word_timings(segments: &mut [TimedTextSegment]) {
    for segment in segments {
        if !segment.words.is_empty() {
            continue;
        }
        let tokens = words_from_segment_text(&segment.text);
        if tokens.is_empty() {
            continue;
        }
        let duration = segment
            .end_ms
            .saturating_sub(segment.start_ms)
            .max(1);
        let token_count = tokens.len();
        let step = (duration / token_count as u64).max(1);
        let mut cursor = segment.start_ms;
        for (index, text) in tokens.into_iter().enumerate() {
            let end = if index + 1 == token_count {
                segment.end_ms.max(cursor)
            } else {
                cursor.saturating_add(step).min(segment.end_ms)
            };
            segment.words.push(TimedWord {
                text,
                start_ms: cursor,
                end_ms: end.max(cursor),
                confidence: None,
            });
            cursor = end;
        }
    }
}

/// Replaces word start/end with energy alignment when counts match the coarse transcript.
pub fn apply_energy_refined_word_timings(
    segments: &mut [TimedTextSegment],
    audio: &AudioSegment,
) {
    let refined = refine_word_timings(audio, segments);
    if refined.is_empty() {
        return;
    }
    let mut expected = 0usize;
    for segment in segments.iter() {
        expected += segment.words.len();
    }
    if expected == 0 || refined.len() != expected {
        return;
    }
    let mut iter = refined.iter();
    for segment in segments.iter_mut() {
        for word in &mut segment.words {
            let Some(timing) = iter.next() else {
                return;
            };
            word.start_ms = timing.start_ms;
            word.end_ms = timing.end_ms;
        }
    }
}

pub fn refine_word_timings(
    audio: &AudioSegment,
    timed_segments: &[TimedTextSegment],
) -> Vec<WordTiming> {
    if timed_segments.is_empty() {
        return Vec::new();
    }
    let input = unified_from_timed_segments(timed_segments);
    align(audio, &input, EnergyAlignConfig::default()).unwrap_or_default()
}

pub fn phoneme_segments_from_words(words: &[WordTiming]) -> Vec<PhonemeSegment> {
    let mut segments = Vec::new();
    for word in words {
        let phones = phonemize_word(&word.text);
        if phones.is_empty() {
            continue;
        }
        let duration = word.end_ms.saturating_sub(word.start_ms).max(1);
        let step = duration / phones.len() as u64;
        let mut cursor = word.start_ms;
        for (index, symbol) in phones.iter().enumerate() {
            let end = if index + 1 == phones.len() {
                word.end_ms
            } else {
                cursor.saturating_add(step)
            };
            segments.push(PhonemeSegment {
                symbol: symbol.clone(),
                start_ms: cursor,
                end_ms: end,
                gop: None,
            });
            cursor = end;
        }
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timed_text::TimedTextSegment;

    #[test]
    fn builds_segments_from_transcript_when_stt_has_text_only() {
        let mut segments = Vec::new();
        ensure_timed_segments_from_transcript(
            &mut segments,
            "hello world",
            100,
            5000,
        );
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].words.len(), 2);
    }

    #[test]
    fn fluency_sees_words_after_text_only_stt() {
        use crate::tools::speech_analysis::fluency::analyze_fluency;
        let mut segments = Vec::new();
        ensure_timed_segments_from_transcript(
            &mut segments,
            "alpha beta gamma delta",
            0,
            10_000,
        );
        let fluency = analyze_fluency(
            &segments,
            &[(0, 10_000)],
            10_000,
            Some("ru"),
            &super::super::config::SpeechAnalysisConfig::default(),
            &[],
            16_000,
        );
        assert!(fluency.word_count >= 4);
    }

    #[test]
    fn ensure_word_timings_from_segment_text() {
        let mut segments = vec![TimedTextSegment {
            text: "one two three".to_string(),
            start_ms: 0,
            end_ms: 3000,
            words: Vec::new(),
        }];
        ensure_segment_word_timings(&mut segments);
        assert_eq!(segments[0].words.len(), 3);
        assert_eq!(segments[0].words[0].text, "one");
        assert_eq!(segments[0].words[2].end_ms, 3000);
    }
}
