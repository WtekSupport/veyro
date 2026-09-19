use serde::{Deserialize, Serialize};

use super::WhisperModelKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSttEngine {
    Whisper,
    Sherpa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LocalSttModelKind {
    #[default]
    #[serde(rename = "base", alias = "whisper_base")]
    WhisperBase,
    #[serde(rename = "small", alias = "whisper_small")]
    WhisperSmall,
    #[serde(rename = "medium", alias = "whisper_medium")]
    WhisperMedium,
    #[serde(rename = "large_v3_turbo", alias = "whisper_large_v3_turbo")]
    WhisperLargeV3Turbo,
    #[serde(rename = "large_v3", alias = "whisper_large_v3")]
    WhisperLargeV3,
    #[serde(rename = "parakeet_tdt_0_6b_v3", alias = "parakeet_tdt06b_v3")]
    ParakeetTdt06bV3,
    #[serde(rename = "qwen3_asr_0_6b", alias = "qwen3_asr06b")]
    Qwen3Asr06b,
    #[serde(rename = "qwen3_asr_1_7b", alias = "qwen3_asr17b")]
    Qwen3Asr17b,
}

impl LocalSttModelKind {
    pub fn all() -> [Self; 8] {
        [
            Self::WhisperBase,
            Self::WhisperSmall,
            Self::WhisperMedium,
            Self::WhisperLargeV3Turbo,
            Self::WhisperLargeV3,
            Self::ParakeetTdt06bV3,
            Self::Qwen3Asr06b,
            Self::Qwen3Asr17b,
        ]
    }

    pub fn engine(self) -> LocalSttEngine {
        if self.is_whisper() {
            LocalSttEngine::Whisper
        } else {
            LocalSttEngine::Sherpa
        }
    }

    pub fn is_whisper(self) -> bool {
        matches!(
            self,
            Self::WhisperBase
                | Self::WhisperSmall
                | Self::WhisperMedium
                | Self::WhisperLargeV3Turbo
                | Self::WhisperLargeV3
        )
    }

    pub fn whisper_kind(self) -> Option<WhisperModelKind> {
        Some(match self {
            Self::WhisperBase => WhisperModelKind::Base,
            Self::WhisperSmall => WhisperModelKind::Small,
            Self::WhisperMedium => WhisperModelKind::Medium,
            Self::WhisperLargeV3Turbo => WhisperModelKind::LargeV3Turbo,
            Self::WhisperLargeV3 => WhisperModelKind::LargeV3,
            _ => return None,
        })
    }

    pub fn from_whisper(kind: WhisperModelKind) -> Self {
        match kind {
            WhisperModelKind::Base => Self::WhisperBase,
            WhisperModelKind::Small => Self::WhisperSmall,
            WhisperModelKind::Medium => Self::WhisperMedium,
            WhisperModelKind::LargeV3Turbo => Self::WhisperLargeV3Turbo,
            WhisperModelKind::LargeV3 => Self::WhisperLargeV3,
        }
    }

    /// Directory name under `{models_dir}/sherpa/`.
    pub fn sherpa_bundle_dir(self) -> &'static str {
        match self {
            Self::ParakeetTdt06bV3 => "parakeet-tdt-0.6b-v3",
            Self::Qwen3Asr06b => "qwen3-asr-0.6b",
            Self::Qwen3Asr17b => "qwen3-asr-1.7b",
            _ => "",
        }
    }

    pub fn download_archive_url(self) -> Option<&'static str> {
        Some(match self {
            Self::ParakeetTdt06bV3 => "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2",
            Self::Qwen3Asr06b => "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2",
            Self::Qwen3Asr17b => "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-qwen3-asr-1.7B-int8-2026-03-25.tar.bz2",
            _ => return None,
        })
    }

    pub fn download_archive_file_name(self) -> Option<&'static str> {
        Some(match self {
            Self::ParakeetTdt06bV3 => "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2",
            Self::Qwen3Asr06b => "sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2",
            Self::Qwen3Asr17b => "sherpa-onnx-qwen3-asr-1.7B-int8-2026-03-25.tar.bz2",
            _ => return None,
        })
    }

    pub fn approx_size_mb(self) -> u32 {
        if let Some(whisper) = self.whisper_kind() {
            return whisper.approx_size_mb();
        }
        match self {
            Self::ParakeetTdt06bV3 => 650,
            Self::Qwen3Asr06b => 950,
            Self::Qwen3Asr17b => 2_200,
            _ => 0,
        }
    }

    pub fn as_api_str(self) -> &'static str {
        match self {
            Self::WhisperBase => "base",
            Self::WhisperSmall => "small",
            Self::WhisperMedium => "medium",
            Self::WhisperLargeV3Turbo => "large_v3_turbo",
            Self::WhisperLargeV3 => "large_v3",
            Self::ParakeetTdt06bV3 => "parakeet_tdt_0_6b_v3",
            Self::Qwen3Asr06b => "qwen3_asr_0_6b",
            Self::Qwen3Asr17b => "qwen3_asr_1_7b",
        }
    }
}

pub fn sherpa_stt_compiled() -> bool {
    cfg!(feature = "local-sherpa-stt")
}

pub fn sherpa_gpu_compiled() -> bool {
    cfg!(any(feature = "local-sherpa-cuda", feature = "local-sherpa-directml"))
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn sherpa_accelerator_available() -> bool {
    sherpa_stt_compiled()
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub fn sherpa_accelerator_available() -> bool {
    sherpa_gpu_compiled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_legacy_whisper_values() {
        for (value, expected) in [
            ("base", LocalSttModelKind::WhisperBase),
            ("large_v3_turbo", LocalSttModelKind::WhisperLargeV3Turbo),
            ("parakeet_tdt_0_6b_v3", LocalSttModelKind::ParakeetTdt06bV3),
            ("qwen3_asr_0_6b", LocalSttModelKind::Qwen3Asr06b),
        ] {
            let parsed: LocalSttModelKind = serde_json::from_str(&format!("\"{value}\"")).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn serializes_api_model_ids() {
        for (kind, expected) in [
            (LocalSttModelKind::WhisperBase, "base"),
            (LocalSttModelKind::WhisperLargeV3Turbo, "large_v3_turbo"),
            (LocalSttModelKind::ParakeetTdt06bV3, "parakeet_tdt_0_6b_v3"),
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
        }
    }
}
