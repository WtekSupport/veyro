use crate::timed_text::TimedTextSegment;

use super::types::{IntelligibilityReport, LowConfidenceWord, ReliabilityLevel};

const LOW_CONFIDENCE_THRESHOLD: f32 = 0.55;

pub fn analyze_intelligibility(
    timed_segments: &[TimedTextSegment],
    transcript_confidence: Option<f32>,
) -> IntelligibilityReport {
    let mut confidences = Vec::new();
    let mut low_confidence_words = Vec::new();

    for segment in timed_segments {
        for word in &segment.words {
            if let Some(confidence) = word_confidence_proxy(word) {
                confidences.push(confidence);
                if confidence < LOW_CONFIDENCE_THRESHOLD {
                    low_confidence_words.push(LowConfidenceWord {
                        text: word.text.clone(),
                        start_ms: word.start_ms,
                        end_ms: word.end_ms,
                        confidence,
                    });
                }
            }
        }
    }

    let mean_word_confidence = if confidences.is_empty() {
        transcript_confidence
    } else {
        Some(confidences.iter().sum::<f32>() / confidences.len() as f32)
    };

    let reliability = if mean_word_confidence.is_some() {
        ReliabilityLevel::Medium
    } else {
        ReliabilityLevel::Unavailable
    };

    low_confidence_words.truncate(40);

    IntelligibilityReport {
        mean_word_confidence,
        low_confidence_word_count: low_confidence_words.len(),
        low_confidence_words,
        reliability,
    }
}

fn word_confidence_proxy(word: &crate::timed_text::TimedWord) -> Option<f32> {
    word.confidence
}
