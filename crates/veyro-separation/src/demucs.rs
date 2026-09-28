use std::sync::Arc;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use crate::error::SeparationError;
use crate::model::LoadedModel;
use crate::session::ExecutionProvider;

const SAMPLE_RATE: u32 = 44_100;
const N_SAMPLES: usize = 343_980;
const VOCALS_INDEX: usize = 3;

struct DemucsSession {
    inner: Session,
}

impl DemucsSession {
    fn open(model: &LoadedModel, provider: ExecutionProvider) -> Result<Self, SeparationError> {
        let mut builder = Session::builder().map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        builder = builder
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        for ep in provider.execution_providers() {
            builder = builder
                .with_execution_providers([ep])
                .map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        }
        let inner = builder
            .commit_from_file(model.path())
            .map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        Ok(Self { inner })
    }

    fn run_chunk(&mut self, chunk: &[f32]) -> Result<Vec<f32>, SeparationError> {
        if chunk.len() % 2 != 0 {
            return Err(SeparationError::InvalidAudio(
                "stereo chunk length must be even".into(),
            ));
        }
        let frames = chunk.len() / 2;
        let mut planar = Vec::with_capacity(2 * frames);
        for frame in 0..frames {
            planar.push(chunk[frame * 2]);
        }
        for frame in 0..frames {
            planar.push(chunk[frame * 2 + 1]);
        }
        let shape = [1_i64, 2, frames as i64];
        let tensor = Tensor::from_array((shape, planar))
            .map_err(|e| SeparationError::Inference(e.to_string()))?;
        let outputs = self
            .inner
            .run(ort::inputs!["mix" => tensor])
            .map_err(|e| SeparationError::Inference(e.to_string()))?;
        let (_, output) = outputs
            .into_iter()
            .next()
            .ok_or_else(|| SeparationError::Inference("empty model output".into()))?;
        let (shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| SeparationError::Inference(e.to_string()))?;
        extract_vocals_interleaved(shape, data, frames)
    }
}

fn extract_vocals_interleaved(
    shape: &[i64],
    data: &[f32],
    input_frames: usize,
) -> Result<Vec<f32>, SeparationError> {
    if shape.len() == 3 && shape[0] == 1 && shape[1] == 2 {
        let out_frames = shape[2] as usize;
        let frames = input_frames.min(out_frames);
        return interleave_planar_stereo(&data[..out_frames * 2], frames);
    }
    if shape.len() == 3 && shape[0] >= 4 && shape[1] == 2 {
        let out_frames = shape[2] as usize;
        let frames = input_frames.min(out_frames);
        let channels = shape[1] as usize;
        let stem_block = channels * out_frames;
        let base = VOCALS_INDEX * stem_block;
        if data.len() < base + channels * frames {
            return Err(SeparationError::Inference(format!(
                "demucs output too short for shape {shape:?}"
            )));
        }
        let mut interleaved = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            interleaved.push(data[base + frame]);
            interleaved.push(data[base + out_frames + frame]);
        }
        return Ok(interleaved);
    }
    if shape.len() == 4 && shape[0] == 1 && shape[1] >= 4 && shape[2] == 2 {
        let out_frames = shape[3] as usize;
        let frames = input_frames.min(out_frames);
        let channels = shape[2] as usize;
        let stem_block = channels * out_frames;
        let base = VOCALS_INDEX * stem_block;
        if data.len() < base + channels * frames {
            return Err(SeparationError::Inference(format!(
                "demucs output too short for shape {shape:?}"
            )));
        }
        let mut interleaved = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            interleaved.push(data[base + frame]);
            interleaved.push(data[base + out_frames + frame]);
        }
        return Ok(interleaved);
    }
    Err(SeparationError::Inference(format!(
        "unexpected demucs output shape: {shape:?}"
    )))
}

fn interleave_planar_stereo(planar: &[f32], frames: usize) -> Result<Vec<f32>, SeparationError> {
    if planar.len() < frames * 2 {
        return Err(SeparationError::Inference(
            "demucs planar output too short".into(),
        ));
    }
    let mut interleaved = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        interleaved.push(planar[frame]);
        interleaved.push(planar[frames + frame]);
    }
    Ok(interleaved)
}

fn chunk_window(n: usize, overlap: usize) -> Vec<f32> {
    let mut window = vec![1.0f32; n];
    if overlap == 0 {
        return window;
    }
    for i in 0..overlap {
        let fade = i as f32 / overlap as f32;
        window[i] = fade;
        window[n - 1 - i] = fade;
    }
    window
}

pub fn separate_demucs(
    model: &LoadedModel,
    mix: &crate::buffer::AudioBuffer,
    provider: ExecutionProvider,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<(crate::buffer::AudioBuffer, crate::buffer::AudioBuffer), SeparationError> {
    if mix.sample_rate != SAMPLE_RATE {
        return Err(SeparationError::InvalidAudio(format!(
            "Demucs requires {SAMPLE_RATE} Hz input"
        )));
    }
    if mix.channels != 2 {
        return Err(SeparationError::InvalidAudio(
            "Demucs requires stereo input".into(),
        ));
    }

    let overlap = N_SAMPLES / 4;
    let stride = N_SAMPLES - overlap;
    let total = mix.samples.len() / 2;
    let n_chunks = if total == 0 {
        1
    } else {
        (total + stride - 1) / stride
    };
    let window = chunk_window(N_SAMPLES, overlap);
    let mut session = DemucsSession::open(model, provider)?;
    let mut acc = vec![0.0f32; mix.samples.len()];
    let mut weights = vec![0.0f32; mix.samples.len()];

    for chunk_index in 0..n_chunks {
        let start = chunk_index * stride;
        let end = (start + N_SAMPLES).min(total);
        let mut chunk = mix.samples[start * 2..end * 2].to_vec();
        if chunk.len() / 2 < N_SAMPLES {
            chunk.resize(N_SAMPLES * 2, 0.0);
        }

        let vocal_chunk = session.run_chunk(&chunk)?;
        let effective_frames = end - start;
        for frame in 0..effective_frames {
            let w = window[frame];
            let out_idx = (start + frame) * 2;
            acc[out_idx] += vocal_chunk[frame * 2] * w;
            acc[out_idx + 1] += vocal_chunk[frame * 2 + 1] * w;
            weights[out_idx] += w;
            weights[out_idx + 1] += w;
        }

        let percent = (((chunk_index + 1) as u64 * 100) / n_chunks as u64).min(100) as u8;
        progress(percent);
    }

    let mut vocals = acc;
    for (sample, weight) in vocals.iter_mut().zip(weights.iter()) {
        if *weight > 1e-6 {
            *sample /= weight;
        }
    }
    vocals.truncate(mix.samples.len());

    let instrumental = mix
        .samples
        .iter()
        .zip(vocals.iter())
        .map(|(m, v)| m - v)
        .collect();

    Ok((
        crate::buffer::AudioBuffer::new(vocals, mix.sample_rate, 2),
        crate::buffer::AudioBuffer::new(instrumental, mix.sample_rate, 2),
    ))
}
