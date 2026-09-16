use crate::audio::segment::AudioSegment;

/// Ensure audio segment memory is released promptly after processing.
pub fn clear_segment(segment: &mut AudioSegment) {
    segment.samples.clear();
    segment.samples.shrink_to_fit();
    segment.duration_ms = 0;
}
