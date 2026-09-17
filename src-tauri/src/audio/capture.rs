use std::sync::Arc;

use super::segment::AudioSegment;

pub type SegmentCallback = Arc<dyn Fn(AudioSegment) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Continuous,
    PushToTalk,
}
