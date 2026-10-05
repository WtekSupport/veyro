mod config;
mod ctc;
mod energy;
mod normalize;
mod types;

pub use config::EnergyAlignConfig;
pub use normalize::{segment_words_for_alignment, unified_from_timed_segments};
pub use types::{AlignError, CoarseSegment, UnifiedInput, WordTiming};

use crate::audio::segment::AudioSegment;

use ctc::ctc_forced_align;
use energy::align_segments_via_energy;

/// Choose algorithm A (energy refinement) or B (CTC) per `todo/word-alignment-spec.md`.
pub fn align(
    audio: &AudioSegment,
    input: &UnifiedInput,
    config: EnergyAlignConfig,
) -> Result<Vec<WordTiming>, AlignError> {
    if input.full_text.trim().is_empty() {
        return Err(AlignError::EmptyTranscript);
    }

    if let Some(segments) = input.coarse_segments.as_ref() {
        if !segments.is_empty() {
            return Ok(align_segments_via_energy(audio, segments, config));
        }
    }

    ctc_forced_align(audio, &input.full_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timed_text::TimedTextSegment;

    #[test]
    fn align_uses_energy_when_coarse_segments_present() {
        let input = unified_from_timed_segments(&[TimedTextSegment {
            text: "hello".to_string(),
            start_ms: 0,
            end_ms: 500,
            words: Vec::new(),
        }]);
        let audio = AudioSegment::new(vec![0.2; 8_000], 16_000, 1);
        let words = align(&audio, &input, EnergyAlignConfig::default()).expect("align");
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "hello");
    }

    #[test]
    fn align_without_segments_uses_ctc_uniform() {
        let input = UnifiedInput {
            full_text: "hello world".to_string(),
            coarse_segments: None,
        };
        let audio = AudioSegment::new(vec![0.2; 8_000], 16_000, 1);
        let words = align(&audio, &input, EnergyAlignConfig::default()).expect("align");
        assert_eq!(words.len(), 2);
    }
}
