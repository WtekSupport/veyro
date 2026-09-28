use std::path::Path;

use crate::error::SeparationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeparationProfile {
    Quality,
    Fast,
    Legacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    RoFormer,
    StftRoFormer,
    Demucs,
}

impl SeparationProfile {
    pub fn model_kind(self) -> ModelKind {
        match self {
            Self::Quality | Self::Fast => ModelKind::StftRoFormer,
            Self::Legacy => ModelKind::Demucs,
        }
    }

    pub fn stft_track_step_samples(self) -> usize {
        match self {
            Self::Quality => 44_100 * 8,
            Self::Fast => 44_100 * 10,
            Self::Legacy => 0,
        }
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Quality => "quality",
            Self::Fast => "fast",
            Self::Legacy => "legacy",
        }
    }

    pub fn chunk_samples(self) -> usize {
        match self {
            Self::Quality => 484_659, // hop * (1100 - 1)
            Self::Fast => 262_144,
            Self::Legacy => 343_980,
        }
    }

    pub fn overlap_samples(self) -> usize {
        match self {
            Self::Quality => 44_100,
            Self::Fast => 22_050,
            Self::Legacy => 34_398,
        }
    }
}

pub struct LoadedModel {
    path: std::path::PathBuf,
    kind: ModelKind,
    profile: SeparationProfile,
}

impl LoadedModel {
    pub fn load(path: &Path, profile: SeparationProfile) -> Result<Self, SeparationError> {
        if !path.is_file() {
            return Err(SeparationError::ModelLoad(format!(
                "missing model file: {}",
                path.display()
            )));
        }
        Ok(Self {
            path: path.to_path_buf(),
            kind: profile.model_kind(),
            profile,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> ModelKind {
        self.kind
    }

    pub fn profile(&self) -> SeparationProfile {
        self.profile
    }
}
