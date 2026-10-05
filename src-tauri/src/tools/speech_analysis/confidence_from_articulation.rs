use crate::timed_text::TimedTextSegment;

use super::types::ArticulationReport;

/// Maps phoneme-level GOP proxies onto word timings when available.
pub fn apply_word_confidence_from_articulation(
    timed_segments: &mut [TimedTextSegment],
    articulation: &ArticulationReport,
) {
    if articulation
        .phoneme_segments
        .iter()
        .all(|segment| segment.gop.is_none())
    {
        return;
    }

    for segment in timed_segments {
        for word in &mut segment.words {
            if word.confidence.is_some() {
                continue;
            }
            let mut gops = Vec::new();
            for phoneme in &articulation.phoneme_segments {
                let Some(gop) = phoneme.gop else {
                    continue;
                };
                let overlap_start = word.start_ms.max(phoneme.start_ms);
                let overlap_end = word.end_ms.min(phoneme.end_ms);
                if overlap_end > overlap_start {
                    gops.push(gop);
                }
            }
            if gops.is_empty() {
                continue;
            }
            let mean_gop = gops.iter().sum::<f32>() / gops.len() as f32;
            let normalized = ((mean_gop.clamp(-1.0, 0.0) + 1.0) * 100.0).round() / 100.0;
            word.confidence = Some(normalized);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timed_text::{TimedTextSegment, TimedWord};
    use super::super::types::{ArticulationReport, PhonemeSegment, ReliabilityLevel};

    #[test]
    fn assigns_overlapping_gop_to_words() {
        let mut segments = vec![TimedTextSegment {
            text: "a b".into(),
            start_ms: 0,
            end_ms: 1000,
            words: vec![
                TimedWord {
                    text: "a".into(),
                    start_ms: 0,
                    end_ms: 400,
                    confidence: None,
                },
                TimedWord {
                    text: "b".into(),
                    start_ms: 400,
                    end_ms: 900,
                    confidence: None,
                },
            ],
        }];
        let articulation = ArticulationReport {
            reliability: ReliabilityLevel::Medium,
            unavailable_reason_key: None,
            ctc_variant_used: None,
            missing_ctc_download: None,
            mean_gop: Some(-0.2),
            per: None,
            low_gop_token_count: 0,
            alignment_token_count: 2,
            letter_baseline_error_rate_percent: None,
            weak_symbols: Vec::new(),
            top_substitutions: Vec::new(),
            phoneme_segments: vec![
                PhonemeSegment {
                    symbol: "a".into(),
                    start_ms: 0,
                    end_ms: 400,
                    gop: Some(-0.2),
                },
                PhonemeSegment {
                    symbol: "b".into(),
                    start_ms: 400,
                    end_ms: 900,
                    gop: Some(0.0),
                },
            ],
            word_gop_hits: Vec::new(),
        };
        apply_word_confidence_from_articulation(&mut segments, &articulation);
        assert!(segments[0].words[0].confidence.is_some());
    }
}
