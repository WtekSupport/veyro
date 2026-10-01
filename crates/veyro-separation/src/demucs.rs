use std::sync::Arc;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use crate::error::SeparationError;
use crate::model::LoadedModel;
use crate::session::ExecutionProvider;

const SAMPLE_RATE: u32 = 44_100;
const N_SAMPLES: usize = 343_980;
/// Classic 4-stem / 6-stem Demucs layout: drums, bass, other, vocals[, guitar, piano].
const VOCALS_INDEX: usize = 3;

/// Instrument stems kept for the “split instrumental further” mode (vocals discarded).
pub const MULTI_STEM_KEEP: &[(&str, usize)] = &[
    ("drums", 0),
    ("bass", 1),
    ("other", 2),
    ("guitar", 4),
    ("piano", 5),
];

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

    fn run_chunk_raw(
        &mut self,
        chunk: &[f32],
    ) -> Result<(Vec<i64>, Vec<f32>, usize), SeparationError> {
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
        let (out_shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| SeparationError::Inference(e.to_string()))?;
        Ok((out_shape.to_vec(), data.to_vec(), frames))
    }
}

fn stem_count_and_layout(shape: &[i64]) -> Result<(usize, usize, usize), SeparationError> {
    // Returns (stem_count, channels, out_frames)
    if shape.len() == 3 && shape[0] == 1 && shape[1] == 2 {
        // Single stereo stem (specialist vocals-only export).
        return Ok((1, 2, shape[2] as usize));
    }
    if shape.len() == 3 && shape[0] >= 4 && shape[1] == 2 {
        return Ok((shape[0] as usize, shape[1] as usize, shape[2] as usize));
    }
    if shape.len() == 4 && shape[0] == 1 && shape[1] >= 4 && shape[2] == 2 {
        return Ok((shape[1] as usize, shape[2] as usize, shape[3] as usize));
    }
    Err(SeparationError::Inference(format!(
        "unexpected demucs output shape: {shape:?}"
    )))
}

fn extract_stem_interleaved(
    shape: &[i64],
    data: &[f32],
    input_frames: usize,
    stem_index: usize,
) -> Result<Vec<f32>, SeparationError> {
    let (stem_count, channels, out_frames) = stem_count_and_layout(shape)?;
    let frames = input_frames.min(out_frames);
    if stem_count == 1 {
        if stem_index != 0 {
            return Err(SeparationError::Inference(
                "single-stem demucs output has no stem index".into(),
            ));
        }
        return interleave_planar_stereo(&data[..out_frames * 2], frames);
    }
    if stem_index >= stem_count {
        return Err(SeparationError::Inference(format!(
            "stem index {stem_index} out of range for {stem_count} stems"
        )));
    }
    let stem_block = channels * out_frames;
    let base = stem_index * stem_block;
    if data.len() < base + channels * frames {
        return Err(SeparationError::Inference(format!(
            "demucs output too short for stem {stem_index} shape {shape:?}"
        )));
    }
    let mut interleaved = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        interleaved.push(data[base + frame]);
        interleaved.push(data[base + out_frames + frame]);
    }
    Ok(interleaved)
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

fn validate_mix(mix: &crate::buffer::AudioBuffer) -> Result<(), SeparationError> {
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
    Ok(())
}

pub fn separate_demucs(
    model: &LoadedModel,
    mix: &crate::buffer::AudioBuffer,
    provider: ExecutionProvider,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<(crate::buffer::AudioBuffer, crate::buffer::AudioBuffer), SeparationError> {
    validate_mix(mix)?;

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

        let (shape, data, frames) = session.run_chunk_raw(&chunk)?;
        // Specialist vocals-only exports have a single stem at index 0; multi-stem uses VOCALS_INDEX.
        let (stem_count, _, _) = stem_count_and_layout(&shape)?;
        let stem_index = if stem_count == 1 { 0 } else { VOCALS_INDEX };
        let vocal_chunk = extract_stem_interleaved(&shape, &data, frames, stem_index)?;
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

/// Run `htdemucs_6s` on the original mix and return non-vocal stems only.
pub fn separate_demucs_multi_stem(
    model: &LoadedModel,
    mix: &crate::buffer::AudioBuffer,
    provider: ExecutionProvider,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<Vec<(String, crate::buffer::AudioBuffer)>, SeparationError> {
    validate_mix(mix)?;

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

    let mut acc: Vec<Vec<f32>> = MULTI_STEM_KEEP
        .iter()
        .map(|_| vec![0.0f32; mix.samples.len()])
        .collect();
    let mut weights = vec![0.0f32; mix.samples.len()];

    for chunk_index in 0..n_chunks {
        let start = chunk_index * stride;
        let end = (start + N_SAMPLES).min(total);
        let mut chunk = mix.samples[start * 2..end * 2].to_vec();
        if chunk.len() / 2 < N_SAMPLES {
            chunk.resize(N_SAMPLES * 2, 0.0);
        }

        let (shape, data, frames) = session.run_chunk_raw(&chunk)?;
        let (stem_count, _, _) = stem_count_and_layout(&shape)?;
        if stem_count < 6 {
            return Err(SeparationError::Inference(format!(
                "multi-stem Demucs expected 6 stems, got {stem_count} (shape {shape:?})"
            )));
        }

        let effective_frames = end - start;
        for (keep_i, &(_name, stem_index)) in MULTI_STEM_KEEP.iter().enumerate() {
            let stem_chunk = extract_stem_interleaved(&shape, &data, frames, stem_index)?;
            for frame in 0..effective_frames {
                let w = window[frame];
                let out_idx = (start + frame) * 2;
                acc[keep_i][out_idx] += stem_chunk[frame * 2] * w;
                acc[keep_i][out_idx + 1] += stem_chunk[frame * 2 + 1] * w;
                if keep_i == 0 {
                    weights[out_idx] += w;
                    weights[out_idx + 1] += w;
                }
            }
        }

        let percent = (((chunk_index + 1) as u64 * 100) / n_chunks as u64).min(100) as u8;
        progress(percent);
    }

    let mut out = Vec::with_capacity(MULTI_STEM_KEEP.len());
    for (keep_i, &(name, _)) in MULTI_STEM_KEEP.iter().enumerate() {
        let mut samples = std::mem::take(&mut acc[keep_i]);
        for (sample, weight) in samples.iter_mut().zip(weights.iter()) {
            if *weight > 1e-6 {
                *sample /= weight;
            }
        }
        samples.truncate(mix.samples.len());
        out.push((
            name.to_string(),
            crate::buffer::AudioBuffer::new(samples, mix.sample_rate, 2),
        ));
    }
    Ok(out)
}
