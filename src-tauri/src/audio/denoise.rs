use nnnoiseless::DenoiseState;
use rubato::{FftFixedIn, Resampler};

const RNNOISE_RATE: u32 = 48_000;

pub fn denoise_mono_16k(samples: &mut Vec<f32>) {
    if samples.is_empty() {
        return;
    }

    let mut at_48k = upsample_16k_to_48k(samples);
    denoise_48k(&mut at_48k);
    *samples = downsample_48k_to_16k(&at_48k);
}

fn denoise_48k(samples: &mut [f32]) {
    let frame_samples = DenoiseState::FRAME_SIZE;
    let mut state = DenoiseState::new();
    let mut out = [0.0f32; DenoiseState::FRAME_SIZE];

    for chunk in samples.chunks_mut(frame_samples) {
        if chunk.len() < frame_samples {
            let mut padded = [0.0f32; DenoiseState::FRAME_SIZE];
            padded[..chunk.len()].copy_from_slice(chunk);
            state.process_frame(&mut out, &padded);
            chunk.copy_from_slice(&out[..chunk.len()]);
            continue;
        }

        let frame = chunk.to_vec();
        state.process_frame(&mut out, &frame);
        chunk.copy_from_slice(&out);
    }
}

fn upsample_16k_to_48k(samples: &[f32]) -> Vec<f32> {
    resample(samples, 16_000, RNNOISE_RATE)
}

fn downsample_48k_to_16k(samples: &[f32]) -> Vec<f32> {
    resample(samples, RNNOISE_RATE, 16_000)
}

fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let mut resampler = FftFixedIn::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        256,
        2,
        1,
    )
    .expect("resampler init");
    let chunk_size = resampler.input_frames_next();
    let mut pending = samples.to_vec();
    let mut output = Vec::new();

    while pending.len() >= chunk_size {
        let chunk: Vec<f32> = pending.drain(..chunk_size).collect();
        let processed = resampler
            .process(&[chunk], None)
            .expect("resample process");
        output.extend_from_slice(&processed[0]);
    }

    if !pending.is_empty() {
        let tail_len = pending.len();
        let mut padded = pending;
        padded.resize(chunk_size, 0.0);
        let processed = resampler
            .process(&[padded], None)
            .expect("resample tail");
        let produced = processed[0].len();
        let valid = (produced as f64 * tail_len as f64 / chunk_size as f64).round() as usize;
        output.extend_from_slice(&processed[0][..valid.min(produced)]);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denoise_smoke_test() {
        let mut samples = vec![0.01; 16_000];
        denoise_mono_16k(&mut samples);
        assert!(
            (15_900..=16_100).contains(&samples.len()),
            "unexpected resampled length {}",
            samples.len()
        );
        assert!(samples.iter().all(|sample| sample.is_finite()));
    }
}
