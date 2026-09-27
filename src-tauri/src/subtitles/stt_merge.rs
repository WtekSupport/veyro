use crate::timed_text::TimedTextSegment;

/// Drop duplicate STT segments produced when adjacent VAD chunks re-transcribe the same speech.
pub fn dedupe_vad_merged_segments(mut segments: Vec<TimedTextSegment>) -> Vec<TimedTextSegment> {
    segments.sort_by_key(|segment| segment.start_ms);
    let mut out: Vec<TimedTextSegment> = Vec::with_capacity(segments.len());

    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        let seg_start = segment.start_ms;
        let seg_end = segment.end_ms.max(seg_start);
        let seg_duration = seg_end.saturating_sub(seg_start).max(1);

        if let Some(last) = out.last() {
            let last_end = last.end_ms.max(last.start_ms);
            if seg_start + 150 < last_end {
                let overlap_start = seg_start.max(last.start_ms);
                let overlap_end = seg_end.min(last_end);
                let overlap = overlap_end.saturating_sub(overlap_start);
                if overlap * 100 / seg_duration >= 35 {
                    continue;
                }
            }
        }

        out.push(TimedTextSegment {
            text: text.to_string(),
            start_ms: seg_start,
            end_ms: seg_end,
            words: segment.words,
        });
    }

    out
}

/// When Whisper language is pinned (auto-detect file), drop segments in the wrong script.
pub fn filter_segments_by_sticky_language(
    segments: Vec<TimedTextSegment>,
    sticky_language: Option<&str>,
) -> Vec<TimedTextSegment> {
    let Some(lang) = sticky_language else {
        return segments;
    };
    let lang = lang.split('-').next().unwrap_or(lang).to_ascii_lowercase();
    let filtered: Vec<_> = segments
        .into_iter()
        .filter(|segment| text_matches_stt_language(&lang, &segment.text))
        .collect();
    filtered
}

pub fn text_matches_stt_language(lang: &str, text: &str) -> bool {
    let lang = lang.split('-').next().unwrap_or(lang).to_ascii_lowercase();
    segment_matches_sticky_language(&lang, text)
}

fn segment_matches_sticky_language(lang: &str, text: &str) -> bool {
    let cyrillic = cyrillic_letter_ratio(text);
    match lang {
        "ru" | "uk" | "be" | "kk" | "bg" | "sr" => cyrillic >= 0.08,
        "en" | "de" | "fr" | "es" | "it" | "pt" | "nl" | "pl" | "cs" | "sv" | "da" | "no"
        | "fi" | "tr" | "id" | "vi" | "ms" | "tl" | "hi" | "ja" | "ko" | "zh" => cyrillic < 0.25,
        _ => true,
    }
}

fn cyrillic_letter_ratio(text: &str) -> f64 {
    let mut letters = 0u32;
    let mut cyrillic = 0u32;
    for ch in text.chars() {
        if ch.is_alphabetic() {
            letters += 1;
            if ('\u{0400}'..='\u{04FF}').contains(&ch) {
                cyrillic += 1;
            }
        }
    }
    if letters == 0 {
        0.0
    } else {
        cyrillic as f64 / letters as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(text: &str, start: u64, end: u64) -> TimedTextSegment {
        TimedTextSegment {
            text: text.to_string(),
            start_ms: start,
            end_ms: end,
            words: Vec::new(),
        }
    }

    #[test]
    fn dedupe_drops_mostly_overlapping_segment() {
        let merged = dedupe_vad_merged_segments(vec![
            seg("Hello world", 0, 5_000),
            seg("Привет мир", 4_000, 9_000),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "Hello world");
    }

    #[test]
    fn filter_drops_cyrillic_when_sticky_en() {
        let merged = filter_segments_by_sticky_language(
            vec![
                seg("Hello", 0, 1_000),
                seg("Максим, спасибо большое", 2_000, 3_000),
            ],
            Some("en"),
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "Hello");
    }
}
