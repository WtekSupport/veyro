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
        let channels = channels.max(1);
        let duration_ms = if sample_rate == 0 {
            0
        } else {
            let frames = samples.len() as u64 / channels as u64;
            (frames * 1000) / sample_rate as u64
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

    /// Inclusive start, exclusive end in milliseconds on this segment's timeline.
    pub fn clip_ms(&self, start_ms: u64, end_ms: u64) -> Self {
        if self.sample_rate == 0 || self.samples.is_empty() {
            return Self::new(Vec::new(), self.sample_rate, self.channels);
        }
        let end_ms = end_ms.max(start_ms);
        let channels = self.channels.max(1) as u64;
        let rate = self.sample_rate as u64;
        let frame_samples = channels;
        let total_frames = self.samples.len() as u64 / frame_samples;
        let start_frame = (start_ms.saturating_mul(rate) / 1000).min(total_frames);
        let end_frame = (end_ms.saturating_mul(rate) / 1000).min(total_frames).max(start_frame);
        let start_index = (start_frame * frame_samples) as usize;
        let end_index = (end_frame * frame_samples) as usize;
        Self::new(
            self.samples[start_index..end_index].to_vec(),
            self.sample_rate,
            self.channels,
        )
    }
}
