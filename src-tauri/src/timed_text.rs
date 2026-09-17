use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimedTextSegment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}
