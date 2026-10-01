mod buffer;
mod chunk;
mod demucs;
mod error;
mod model;
mod normalize;
mod roformer;
mod session;
mod stft_roformer;

pub use buffer::AudioBuffer;
pub use demucs::MULTI_STEM_KEEP;
pub use error::SeparationError;
pub use model::{LoadedModel, ModelKind, SeparationProfile};
pub use session::{ExecutionProvider, SessionOptions};

use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SeparationOptions {
    pub profile: SeparationProfile,
    pub normalize_output: bool,
    pub execution_provider: ExecutionProvider,
}

#[derive(Debug, Clone)]
pub struct SeparatedAudio {
    pub vocals: AudioBuffer,
    pub instrumental: AudioBuffer,
}

#[derive(Debug, Clone)]
pub struct MultiStemAudio {
    /// Non-vocal stems from `htdemucs_6s` (drums/bass/other/guitar/piano).
    pub stems: Vec<(String, AudioBuffer)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeparationWarning {
    ExecutionProviderFallback,
    ProfileFallbackToFast,
}

pub fn load_model(path: &Path, profile: SeparationProfile) -> Result<LoadedModel, SeparationError> {
    LoadedModel::load(path, profile)
}

pub fn init_runtime() {
    ort::init().with_name("veyro").commit();
}

pub fn separate(
    input: &AudioBuffer,
    model: &LoadedModel,
    options: &SeparationOptions,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<(SeparatedAudio, Vec<SeparationWarning>), SeparationError> {
    let mut warnings = Vec::new();
    let mix = input.to_stereo_44100()?;

    let try_run = |model: &LoadedModel, provider: ExecutionProvider| {
        match model.kind() {
            ModelKind::RoFormer => roformer::separate_roformer(model, &mix, provider, progress.clone()),
            ModelKind::StftRoFormer => {
                stft_roformer::separate_stft_roformer(model, &mix, provider, progress.clone())
            }
            ModelKind::Demucs => demucs::separate_demucs(model, &mix, provider, progress.clone()),
            ModelKind::DemucsMultiStem => Err(SeparationError::IncompatibleModel),
        }
    };

    let mut result = try_run(model, options.execution_provider);
    if result.is_err() && options.execution_provider != ExecutionProvider::Cpu {
        warnings.push(SeparationWarning::ExecutionProviderFallback);
        tracing::warn!("separation GPU EP failed, falling back to CPU");
        result = try_run(model, ExecutionProvider::Cpu);
    }

    let (mut vocals, mut instrumental) = result?;

    if options.normalize_output {
        normalize::peak_normalize(&mut vocals);
        normalize::peak_normalize(&mut instrumental);
    }

    Ok((
        SeparatedAudio {
            vocals,
            instrumental,
        },
        warnings,
    ))
}

/// Second independent inference on the **original mix** for instrument stems.
/// Does not consume or modify the base vocals/instrumental pair.
pub fn separate_multi_stem(
    input: &AudioBuffer,
    model: &LoadedModel,
    options: &SeparationOptions,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<(MultiStemAudio, Vec<SeparationWarning>), SeparationError> {
    if model.kind() != ModelKind::DemucsMultiStem {
        return Err(SeparationError::IncompatibleModel);
    }
    let mut warnings = Vec::new();
    let mix = input.to_stereo_44100()?;

    let try_run = |provider: ExecutionProvider| {
        demucs::separate_demucs_multi_stem(model, &mix, provider, progress.clone())
    };

    let mut result = try_run(options.execution_provider);
    if result.is_err() && options.execution_provider != ExecutionProvider::Cpu {
        warnings.push(SeparationWarning::ExecutionProviderFallback);
        tracing::warn!("multi-stem separation GPU EP failed, falling back to CPU");
        result = try_run(ExecutionProvider::Cpu);
    }

    let mut stems = result?;
    if options.normalize_output {
        for (_name, buffer) in &mut stems {
            normalize::peak_normalize(buffer);
        }
    }

    Ok((MultiStemAudio { stems }, warnings))
}
