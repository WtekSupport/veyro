//! Absolute timeline helpers for the Audio → SRT pipeline (`todo/TZ_audio_to_srt.md` §4.2).
//!
//! **Regression:** `cargo test --no-default-features audio_to_srt_timing`

use crate::audio::segment::AudioSegment;
use crate::timed_text::TimedTextSegment;

/// Whisper stays reliable when each `full()` call stays within VAD-sized windows (~30s).
pub fn expand_stt_chunks(
    regions: Vec<(u64, AudioSegment)>,
    max_chunk_ms: u64,
) -> Vec<(u64, AudioSegment)> {
    let max_chunk_ms = max_chunk_ms.max(1_000);
    let mut out = Vec::new();
    for (region_start_ms, audio) in regions {
        if audio.duration_ms <= max_chunk_ms {
            out.push((region_start_ms, audio));
            continue;
        }
        let mut offset_ms = 0u64;
        while offset_ms < audio.duration_ms {
            let chunk_end = (offset_ms + max_chunk_ms).min(audio.duration_ms);
            let chunk = audio.clip_ms(offset_ms, chunk_end);
            if !chunk.is_empty() {
                out.push((region_start_ms.saturating_add(offset_ms), chunk));
            }
            if chunk_end <= offset_ms {
                break;
            }
            offset_ms = chunk_end;
        }
    }
    out
}

/// Shift chunk-local STT timings onto the file timeline (after VAD region offset).
pub fn offset_timed_segments(segments: &mut [TimedTextSegment], offset_ms: u64) {
    for segment in segments {
        segment.start_ms = segment.start_ms.saturating_add(offset_ms);
        segment.end_ms = segment.end_ms.saturating_add(offset_ms);
        for word in &mut segment.words {
            word.start_ms = word.start_ms.saturating_add(offset_ms);
            word.end_ms = word.end_ms.saturating_add(offset_ms);
        }
    }
}

pub fn merge_region_timed_segments(
    merged: &mut Vec<TimedTextSegment>,
    mut segments: Vec<TimedTextSegment>,
    region_start_ms: u64,
) {
    offset_timed_segments(&mut segments, region_start_ms);
    merged.extend(segments);
}

pub fn sort_timed_segments_by_start(segments: &mut [TimedTextSegment]) {
    segments.sort_by_key(|segment| segment.start_ms);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtitles::{build_subtitle_cues, SubtitleOptions};
    use crate::timed_text::TimedWord;

    fn seg(text: &str, start: u64, end: u64) -> TimedTextSegment {
        TimedTextSegment {
            text: text.to_string(),
            start_ms: start,
            end_ms: end,
            words: Vec::new(),
        }
    }

    #[test]
    fn expand_splits_long_region_on_absolute_timeline() {
        let rate = 16_000u32;
        let samples = vec![0.01f32; rate as usize * 47];
        let long = AudioSegment::new(samples, rate, 1);
        assert!(long.duration_ms >= 46_000);

        let chunks = expand_stt_chunks(vec![(0, long)], 30_000);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].0, 0);
        assert!(chunks[0].1.duration_ms <= 30_000);
        assert_eq!(chunks[1].0, chunks[0].1.duration_ms);
        assert!(chunks[1].1.duration_ms >= 16_000);
    }

    #[test]
    fn offset_shifts_segment_and_words() {
        let mut segments = vec![TimedTextSegment {
            text: "hi".to_string(),
            start_ms: 100,
            end_ms: 400,
            words: vec![TimedWord {
                text: "hi".to_string(),
                start_ms: 100,
                end_ms: 400,
                confidence: None,
            }],
        }];
        offset_timed_segments(&mut segments, 6_000);
        assert_eq!(segments[0].start_ms, 6_100);
        assert_eq!(segments[0].end_ms, 6_400);
        assert_eq!(segments[0].words[0].start_ms, 6_100);
    }

    /// Two VAD islands: STT returns local 0-based timings per chunk; merged file must keep the silence gap.
    #[test]
    fn regression_vad_chunks_preserve_silence_on_merged_timeline() {
        let mut merged = vec![seg("Replica A", 0, 2_000)];
        merge_region_timed_segments(&mut merged, vec![seg("Replica B", 0, 1_500)], 3_500);
        sort_timed_segments_by_start(&mut merged);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].end_ms, 2_000);
        assert_eq!(merged[1].start_ms, 3_500);
        assert!(merged[1].start_ms.saturating_sub(merged[0].end_ms) >= 1_500);

        let options = SubtitleOptions {
            pause_split_ms: 150,
            reading_tail_ms: 200,
            smart_split: true,
            ..Default::default()
        };
        let cues = build_subtitle_cues(&merged, &options);
        assert_eq!(cues.len(), 2);
        let visible_gap = cues[1].start_ms.saturating_sub(cues[0].end_ms);
        assert!(
            visible_gap >= 150,
            "SRT must not glue cues across a 1.5s STT gap (visible_gap={visible_gap}ms)"
        );
    }

    /// Whisper-like segment bounds must flow into cues without collapsing the inter-segment gap.
    #[test]
    fn regression_whisper_segment_bounds_not_replaced_by_next_start() {
        let segments = vec![
            seg("Андрей,", 720, 990),
            seg("привет как дела", 6_200, 8_100),
        ];
        let options = SubtitleOptions {
            pause_split_ms: 400,
            reading_tail_ms: 200,
            smart_split: true,
            ..Default::default()
        };
        let cues = build_subtitle_cues(&segments, &options);
        assert!(cues.len() >= 2);
        let first_end = cues[0].end_ms;
        let second_start = cues[1].start_ms;
        assert!(
            second_start.saturating_sub(first_end) >= 400,
            "first cue end {first_end} must stay before second start {second_start}"
        );
        assert!(first_end <= 990 + 200, "first cue should not stretch far past STT end");
    }
}
