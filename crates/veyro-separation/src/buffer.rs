use crate::error::SeparationError;

pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioBuffer {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate,
            channels: channels.max(1),
        }
    }

    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (self.frame_count() as u64 * 1000) / self.sample_rate as u64
    }

    pub fn to_stereo_44100(&self) -> Result<Self, SeparationError> {
        if self.samples.is_empty() {
            return Err(SeparationError::InvalidAudio("empty".into()));
        }
        let channels = self.channels.max(1) as usize;
        let mut stereo = Vec::with_capacity(self.frame_count() * 2);
        for frame in self.samples.chunks(channels) {
            let left = frame.first().copied().unwrap_or(0.0);
            let right = if channels >= 2 {
                frame[1]
            } else {
                left
            };
            stereo.push(left);
            stereo.push(right);
        }

        if self.sample_rate == SAMPLE_RATE {
            return Ok(Self::new(stereo, SAMPLE_RATE, 2));
        }

        let resampled = crate::roformer::resample_stereo(&stereo, self.sample_rate, SAMPLE_RATE)?;
        Ok(Self::new(resampled, SAMPLE_RATE, 2))
    }
}
