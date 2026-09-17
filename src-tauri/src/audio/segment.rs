use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSegment {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_ms: u64,
}

impl AudioSegment {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        let duration_ms = if sample_rate == 0 {
            0
        } else {
            (samples.len() as u64 * 1000) / sample_rate as u64
        };

        Self {
            samples,
            sample_rate,
            channels,
            duration_ms,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}
