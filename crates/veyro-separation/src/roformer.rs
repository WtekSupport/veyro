use std::sync::Arc;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use crate::error::SeparationError;
use crate::model::LoadedModel;
use crate::session::ExecutionProvider;

pub struct InferenceSession {
    inner: Session,
    input_name: String,
}

impl InferenceSession {
    pub fn open(model: &LoadedModel, provider: ExecutionProvider) -> Result<Self, SeparationError> {
        let mut builder = Session::builder().map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        builder = builder
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| SeparationError::ModelLoad(e.to_string()))?;

        for ep in provider.execution_providers() {
            builder = builder
                .with_execution_providers([ep])
                .map_err(|e| SeparationError::ModelLoad(e.to_string()))?;
        }

        let session = builder
            .commit_from_file(model.path())
            .map_err(|e| {
                if e.to_string().to_ascii_lowercase().contains("memory") {
                    SeparationError::OutOfMemory
                } else {
                    SeparationError::ModelLoad(e.to_string())
                }
            })?;

        let input_name = session
            .inputs()
            .first()
            .map(|input| input.name().to_string())
            .ok_or_else(|| SeparationError::ModelLoad("model has no inputs".into()))?;

        if is_host_stft_input_name(&input_name) {
            return Err(SeparationError::IncompatibleModel);
        }

        Ok(Self {
            inner: session,
            input_name,
        })
    }

    pub fn run_vocal_stem(
        &mut self,
        stereo_chunk: &[f32],
    ) -> Result<Vec<f32>, SeparationError> {
        if stereo_chunk.len() % 2 != 0 {
            return Err(SeparationError::InvalidAudio(
                "stereo chunk length must be even".into(),
            ));
        }
        let frames = stereo_chunk.len() / 2;
        let shape = [1_i64, 2, frames as i64];
        let mut planar = Vec::with_capacity(2 * frames);
        for frame in 0..frames {
            planar.push(stereo_chunk[frame * 2]);
        }
        for frame in 0..frames {
            planar.push(stereo_chunk[frame * 2 + 1]);
        }
        let tensor = Tensor::from_array((shape, planar))
            .map_err(|e| SeparationError::Inference(e.to_string()))?;
        let outputs = self
            .inner
            .run(ort::inputs![self.input_name.as_str() => tensor])
            .map_err(|e| {
                if e.to_string().to_ascii_lowercase().contains("memory") {
                    SeparationError::OutOfMemory
                } else {
                    SeparationError::Inference(e.to_string())
                }
            })?;

        let (_, output) = outputs
            .into_iter()
            .next()
            .ok_or_else(|| SeparationError::Inference("empty model output".into()))?;
        let (shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| SeparationError::Inference(e.to_string()))?;
        extract_stereo_from_output(shape, data)
    }
}

fn is_host_stft_input_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("stft")
}

fn extract_stereo_from_output(shape: &[i64], data: &[f32]) -> Result<Vec<f32>, SeparationError> {
    if shape.len() == 3 && shape[0] == 1 && shape[1] == 2 {
        let frames = shape[2] as usize;
        let mut interleaved = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            interleaved.push(data[frame]);
            interleaved.push(data[frames + frame]);
        }
        return Ok(interleaved);
    }
    if shape.len() == 3 && shape[0] == 1 && shape[2] == 2 {
        let frames = shape[1] as usize;
        let mut interleaved = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let base = frame * 2;
            interleaved.push(data[base]);
            interleaved.push(data[base + 1]);
        }
        return Ok(interleaved);
    }
    if data.len() % 2 == 0 {
        return Ok(data.to_vec());
    }
    Err(SeparationError::Inference(format!(
        "unexpected output shape: {shape:?}"
    )))
}

pub fn resample_stereo(
    interleaved: &[f32],
    source_rate: u32,
    target_rate: u32,
) -> Result<Vec<f32>, SeparationError> {
    if source_rate == target_rate {
        return Ok(interleaved.to_vec());
    }
    if interleaved.len() % 2 != 0 {
        return Err(SeparationError::InvalidAudio(
            "stereo buffer length must be even".into(),
        ));
    }
    let frames = interleaved.len() / 2;
    let ratio = target_rate as f64 / source_rate as f64;
    let out_frames = ((frames as f64) * ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for out_frame in 0..out_frames {
        let src_pos = out_frame as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let idx_next = (idx + 1).min(frames.saturating_sub(1));
        for ch in 0..2 {
            let a = interleaved[idx * 2 + ch];
            let b = interleaved[idx_next * 2 + ch];
            out.push(a + (b - a) * frac);
        }
    }
    Ok(out)
}

pub fn separate_with_vocal_model(
    model: &LoadedModel,
    mix: &crate::buffer::AudioBuffer,
    provider: ExecutionProvider,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<(crate::buffer::AudioBuffer, crate::buffer::AudioBuffer), SeparationError> {
    let profile = model.profile();
    let chunk_size = profile.chunk_samples();
    let overlap = profile.overlap_samples().min(chunk_size / 2);
    let hop = chunk_size.saturating_sub(overlap);

    let mut session = InferenceSession::open(model, provider)?;
    let vocals = crate::chunk::process_chunks(
        &mix.samples,
        chunk_size,
        hop,
        &progress,
        |chunk| session.run_vocal_stem(chunk),
    )?;

    let instrumental = subtract_stems(&mix.samples, &vocals);
    Ok((
        crate::buffer::AudioBuffer::new(vocals, mix.sample_rate, 2),
        crate::buffer::AudioBuffer::new(instrumental, mix.sample_rate, 2),
    ))
}

fn subtract_stems(mix: &[f32], vocals: &[f32]) -> Vec<f32> {
    let len = mix.len().min(vocals.len());
    mix.iter()
        .zip(vocals.iter())
        .take(len)
        .map(|(m, v)| m - v)
        .collect()
}

pub fn separate_roformer(
    model: &LoadedModel,
    mix: &crate::buffer::AudioBuffer,
    provider: ExecutionProvider,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<(crate::buffer::AudioBuffer, crate::buffer::AudioBuffer), SeparationError> {
    separate_with_vocal_model(model, mix, provider, progress)
}
