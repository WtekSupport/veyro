use rubato::{FftFixedIn, Resampler};

use crate::error::AudioError;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;
const RESAMPLE_CHUNK: usize = 256;

pub fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }

    let channels = channels as usize;
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

pub struct MonoResampler {
    resampler: Option<FftFixedIn<f32>>,
    pending: Vec<f32>,
    chunk_size: usize,
}

impl MonoResampler {
    pub fn new(source_rate: u32, target_rate: u32) -> Result<Self, AudioError> {
        if source_rate == target_rate {
            return Ok(Self {
                resampler: None,
                pending: Vec::new(),
                chunk_size: 0,
            });
        }

        let resampler = FftFixedIn::<f32>::new(
            source_rate as usize,
            target_rate as usize,
            RESAMPLE_CHUNK,
            2,
            1,
        )
        .map_err(|error| AudioError::Processing(error.to_string()))?;
        let chunk_size = resampler.input_frames_next();

        Ok(Self {
            resampler: Some(resampler),
            pending: Vec::new(),
            chunk_size,
        })
    }

    pub fn push(&mut self, samples: &[f32], channels: u16) -> Result<Vec<f32>, AudioError> {
        let mono = to_mono(samples, channels);
        if mono.is_empty() {
            return Ok(Vec::new());
        }

        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(mono);
        };

        self.pending.extend_from_slice(&mono);
        let mut output = Vec::new();

        while self.pending.len() >= self.chunk_size {
            let chunk: Vec<f32> = self.pending.drain(..self.chunk_size).collect();
            let processed = resampler
                .process(&[chunk], None)
                .map_err(|error| AudioError::Processing(error.to_string()))?;
            output.extend(processed.into_iter().flatten());
        }

        Ok(output)
    }

    pub fn flush(&mut self) -> Result<Vec<f32>, AudioError> {
        let Some(resampler) = self.resampler.as_mut() else {
            let tail = std::mem::take(&mut self.pending);
            return Ok(tail);
        };

        if self.pending.is_empty() {
            return Ok(Vec::new());
        }

        let mut padded = std::mem::take(&mut self.pending);
        padded.resize(self.chunk_size, 0.0);
        let processed = resampler
            .process(&[padded], None)
            .map_err(|error| AudioError::Processing(error.to_string()))?;
        Ok(processed.into_iter().flatten().collect())
    }

    pub fn reset(&mut self) {
        self.pending.clear();
    }
}

pub fn normalize_segment(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<(Vec<f32>, u32), AudioError> {
    let mut resampler = MonoResampler::new(sample_rate, TARGET_SAMPLE_RATE)?;
    let resampled = resampler.push(samples, channels)?;
    Ok((resampled, TARGET_SAMPLE_RATE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_to_mono_averages_channels() {
        let stereo = vec![1.0, 3.0, 2.0, 4.0];
        assert_eq!(to_mono(&stereo, 2), vec![2.0, 3.0]);
    }

    #[test]
    fn resampler_accepts_small_cpal_buffers() {
        let mut resampler = MonoResampler::new(48_000, TARGET_SAMPLE_RATE).expect("resampler");
        let mut total = 0;
        for _ in 0..20 {
            let chunk = vec![0.1f32; 240];
            let out = resampler.push(&chunk, 2).expect("push");
            total += out.len();
        }
        assert!(total > 0);
    }
}
