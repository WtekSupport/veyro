use crate::audio::segment::AudioSegment;

use super::types::{AlignError, WordTiming};

/// Algorithm B — CTC forced alignment (model-agnostic acoustic emissions).
///
/// Not wired to an acoustic checkpoint yet; callers must handle [`AlignError::CtcUnavailable`]
/// and fall back to energy refinement when coarse segments exist.
pub fn ctc_forced_align(_audio: &AudioSegment, _full_text: &str) -> Result<Vec<WordTiming>, AlignError> {
    Err(AlignError::CtcUnavailable)
}
