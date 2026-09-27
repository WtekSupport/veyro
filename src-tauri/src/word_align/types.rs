#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordTiming {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoarseSegment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedInput {
    pub full_text: String,
    pub coarse_segments: Option<Vec<CoarseSegment>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignError {
    /// No segment-level timings and CTC alignment is not available yet.
    CtcUnavailable,
    EmptyTranscript,
    NoUsableSegments,
}

impl AlignError {
    pub fn user_message_key(self) -> &'static str {
        match self {
            Self::CtcUnavailable => "tools.audioSrt.alignmentNeedsCtc",
            Self::EmptyTranscript => "tools.audioSrt.emptyResult",
            Self::NoUsableSegments => "tools.audioSrt.noTimestamps",
        }
    }
}
