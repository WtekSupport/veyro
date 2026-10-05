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

/// Resample to 16 kHz mono, including any tail buffered in the resampler.
pub fn resample_mono_to_16k(
    samples: &[f32],
    channels: u16,
    source_rate: u32,
) -> Result<Vec<f32>, AudioError> {
    if source_rate == TARGET_SAMPLE_RATE && channels <= 1 {
        return Ok(samples.to_vec());
    }
    let mut resampler = MonoResampler::new(source_rate, TARGET_SAMPLE_RATE)?;
    let mut out = resampler.push(samples, channels)?;
    out.extend(resampler.flush()?);
    Ok(out)
}

pub fn normalize_segment(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<(Vec<f32>, u32), AudioError> {
    let resampled = resample_mono_to_16k(samples, channels, sample_rate)?;
    Ok((resampled, TARGET_SAMPLE_RATE))
}

/// Target sample rate for source-separation models (Mel-Band RoFormer family).
pub const SEPARATION_SAMPLE_RATE: u32 = 44_100;

pub struct ChannelPreservingResampler {
    resampler: Option<FftFixedIn<f32>>,
    pending: Vec<f32>,
    chunk_size: usize,
    channels: u16,
}

impl ChannelPreservingResampler {
    pub fn new(source_rate: u32, target_rate: u32, channels: u16) -> Result<Self, AudioError> {
        let channels = channels.max(1);
        if source_rate == target_rate {
            return Ok(Self {
                resampler: None,
                pending: Vec::new(),
                chunk_size: 0,
                channels,
            });
        }

        let resampler = FftFixedIn::<f32>::new(
            source_rate as usize,
            target_rate as usize,
            RESAMPLE_CHUNK,
            2,
            channels as usize,
        )
        .map_err(|error| AudioError::Processing(error.to_string()))?;
        let chunk_size = resampler.input_frames_next();

        Ok(Self {
            resampler: Some(resampler),
            pending: Vec::new(),
            chunk_size,
            channels,
        })
    }

    pub fn push(&mut self, samples: &[f32], channels: u16) -> Result<Vec<f32>, AudioError> {
        let channels = channels.max(1);
        if channels != self.channels {
            return Err(AudioError::Processing(format!(
                "channel count changed from {} to {channels}",
                self.channels
            )));
        }
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let frame_bytes = self.channels as usize;
        let chunk_size = self.chunk_size;

        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(samples.to_vec());
        };

        self.pending.extend_from_slice(samples);
        let mut output = Vec::new();

        while self.pending.len() / frame_bytes >= chunk_size {
            let take_samples = chunk_size * frame_bytes;
            let chunk: Vec<f32> = self.pending.drain(..take_samples).collect();
            let mut planes: Vec<Vec<f32>> = (0..frame_bytes)
                .map(|_| Vec::with_capacity(chunk_size))
                .collect();
            for frame in chunk.chunks(frame_bytes) {
                for (ch, value) in frame.iter().enumerate() {
                    planes[ch].push(*value);
                }
            }
            let processed = resampler
                .process(&planes, None)
                .map_err(|error| AudioError::Processing(error.to_string()))?;
            for frame_idx in 0..processed[0].len() {
                for plane in &processed {
                    output.push(plane[frame_idx]);
                }
            }
        }

        Ok(output)
    }

    pub fn flush(&mut self) -> Result<Vec<f32>, AudioError> {
        if self.pending.is_empty() {
            return Ok(Vec::new());
        }

        let frame_bytes = self.channels as usize;
        let chunk_size = self.chunk_size;
        let frames = self.pending.len() / frame_bytes;

        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(std::mem::take(&mut self.pending));
        };

        let mut padded = std::mem::take(&mut self.pending);
        padded.resize(chunk_size * frame_bytes, 0.0);
        let mut planes: Vec<Vec<f32>> = (0..frame_bytes)
            .map(|_| Vec::with_capacity(chunk_size))
            .collect();
        for frame in padded.chunks(frame_bytes) {
            for (ch, value) in frame.iter().enumerate() {
                if planes[ch].len() < chunk_size {
                    planes[ch].push(*value);
                }
            }
        }
        for plane in &mut planes {
            plane.resize(chunk_size, 0.0);
        }
        let processed = resampler
            .process(&planes, None)
            .map_err(|error| AudioError::Processing(error.to_string()))?;
        let out_frames = processed[0].len();
        let mut output = Vec::with_capacity(out_frames * frame_bytes);
        for frame_idx in 0..out_frames {
            for plane in &processed {
                output.push(plane[frame_idx]);
            }
        }
        let keep_frames = frames.min(out_frames);
        Ok(output[..keep_frames * frame_bytes].to_vec())
    }
}

pub fn resample_preserve_channels(
    samples: &[f32],
    source_rate: u32,
    target_rate: u32,
    channels: u16,
) -> Result<Vec<f32>, AudioError> {
    let mut resampler = ChannelPreservingResampler::new(source_rate, target_rate, channels)?;
    let mut out = resampler.push(samples, channels)?;
    out.extend(resampler.flush()?);
    Ok(out)
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
