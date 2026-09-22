use serde::{Deserialize, Serialize};

use super::local_stt::{LocalSttEngine, LocalSttModelKind};
use super::WhisperModelKind;

const HF_WHISPER: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LocalSttFamily {
    #[default]
    WhisperBase,
    WhisperSmall,
    WhisperMedium,
    WhisperLargeV3Turbo,
    WhisperLargeV3,
    #[serde(
        rename = "parakeet_tdt_0_6b_v3",
        alias = "parakeet_tdt06b_v3"
    )]
    ParakeetTdt06bV3,
    #[serde(rename = "qwen3_asr_0_6b", alias = "qwen3_asr06b")]
    Qwen3Asr06b,
    #[serde(rename = "qwen3_asr_1_7b", alias = "qwen3_asr17b")]
    Qwen3Asr17b,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LocalSttQuant {
    /// Pre-quantization `ggml-{size}.bin` (legacy installs).
    #[default]
    Legacy,
    Q4_0,
    Q5,
    Q8_0,
    Int8,
    Fp16,
    Fp32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSttVariant {
    pub family: LocalSttFamily,
    pub quant: LocalSttQuant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SherpaOnnxLayout {
    NemoInt8,
    Qwen3Int8,
    NemoFpOnnx,
}

#[derive(Debug, Clone, Copy)]
pub struct SttVariantSpec {
    pub variant: LocalSttVariant,
    pub engine: LocalSttEngine,
    pub available: bool,
    pub download_size_mb: u32,
    pub ram_mb: u32,
    pub vram_mb: Option<u32>,
    pub speed_tier: u8,
    pub accuracy_tier: u8,
    pub features_i18n_key: &'static str,
    pub whisper_file_name: Option<&'static str>,
    pub sherpa_archive_url: Option<&'static str>,
    pub sherpa_archive_name: Option<&'static str>,
    pub sherpa_layout: Option<SherpaOnnxLayout>,
}

impl LocalSttFamily {
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

    pub fn whisper_slug(self) -> Option<&'static str> {
        Some(match self {
            Self::WhisperBase => "base",
            Self::WhisperSmall => "small",
            Self::WhisperMedium => "medium",
            Self::WhisperLargeV3Turbo => "large-v3-turbo",
            Self::WhisperLargeV3 => "large-v3",
            _ => return None,
        })
    }

    pub fn sherpa_bundle_key(self) -> Option<&'static str> {
        Some(match self {
            Self::ParakeetTdt06bV3 => "parakeet-tdt-0.6b-v3",
            Self::Qwen3Asr06b => "qwen3-asr-0.6b",
            Self::Qwen3Asr17b => "qwen3-asr-1.7b",
            _ => return None,
        })
    }

    pub fn name_i18n_key(self) -> &'static str {
        match self {
            Self::WhisperBase => "settings.sttFamilyWhisperBase",
            Self::WhisperSmall => "settings.sttFamilyWhisperSmall",
            Self::WhisperMedium => "settings.sttFamilyWhisperMedium",
            Self::WhisperLargeV3Turbo => "settings.sttFamilyWhisperLargeV3Turbo",
            Self::WhisperLargeV3 => "settings.sttFamilyWhisperLargeV3",
            Self::ParakeetTdt06bV3 => "settings.sttFamilyParakeet",
            Self::Qwen3Asr06b => "settings.sttFamilyQwen3Asr06b",
            Self::Qwen3Asr17b => "settings.sttFamilyQwen3Asr17b",
        }
    }

    pub fn from_legacy_kind(kind: LocalSttModelKind) -> Self {
        match kind {
            LocalSttModelKind::WhisperBase => Self::WhisperBase,
            LocalSttModelKind::WhisperSmall => Self::WhisperSmall,
            LocalSttModelKind::WhisperMedium => Self::WhisperMedium,
            LocalSttModelKind::WhisperLargeV3Turbo => Self::WhisperLargeV3Turbo,
            LocalSttModelKind::WhisperLargeV3 => Self::WhisperLargeV3,
            LocalSttModelKind::ParakeetTdt06bV3 => Self::ParakeetTdt06bV3,
            LocalSttModelKind::Qwen3Asr06b => Self::Qwen3Asr06b,
            LocalSttModelKind::Qwen3Asr17b => Self::Qwen3Asr17b,
        }
    }

    pub fn to_legacy_kind(self, _quant: LocalSttQuant) -> Option<LocalSttModelKind> {
        Some(match self {
            Self::WhisperBase => LocalSttModelKind::WhisperBase,
            Self::WhisperSmall => LocalSttModelKind::WhisperSmall,
            Self::WhisperMedium => LocalSttModelKind::WhisperMedium,
            Self::WhisperLargeV3Turbo => LocalSttModelKind::WhisperLargeV3Turbo,
            Self::WhisperLargeV3 => LocalSttModelKind::WhisperLargeV3,
            Self::ParakeetTdt06bV3 => LocalSttModelKind::ParakeetTdt06bV3,
            Self::Qwen3Asr06b => LocalSttModelKind::Qwen3Asr06b,
            Self::Qwen3Asr17b => LocalSttModelKind::Qwen3Asr17b,
        })
    }
}

impl LocalSttQuant {
    pub fn label_i18n_key(self) -> &'static str {
        match self {
            Self::Legacy => "settings.sttQuantLegacy",
            Self::Q4_0 => "settings.sttQuantQ4",
            Self::Q5 => "settings.sttQuantQ5",
            Self::Q8_0 => "settings.sttQuantQ8",
            Self::Int8 => "settings.sttQuantInt8",
            Self::Fp16 => "settings.sttQuantFp16",
            Self::Fp32 => "settings.sttQuantFp32",
        }
    }

    pub fn is_default_for_family(self, family: LocalSttFamily) -> bool {
        if family.is_whisper() {
            return self == Self::Legacy;
        }
        self == Self::Int8
    }
}

impl LocalSttVariant {
    pub fn new(family: LocalSttFamily, quant: LocalSttQuant) -> Self {
        Self { family, quant }
    }

    pub fn from_legacy_kind(kind: LocalSttModelKind) -> Self {
        Self {
            family: LocalSttFamily::from_legacy_kind(kind),
            quant: if kind.is_whisper() {
                LocalSttQuant::Legacy
            } else {
                LocalSttQuant::Int8
            },
        }
    }

    pub fn as_api_id(self) -> String {
        format!(
            "{}_{}",
            family_api_str(self.family),
            quant_api_str(self.quant)
        )
    }

    pub fn engine(self) -> LocalSttEngine {
        self.family.engine()
    }
}

fn family_api_str(family: LocalSttFamily) -> &'static str {
    match family {
        LocalSttFamily::WhisperBase => "whisper_base",
        LocalSttFamily::WhisperSmall => "whisper_small",
        LocalSttFamily::WhisperMedium => "whisper_medium",
        LocalSttFamily::WhisperLargeV3Turbo => "whisper_large_v3_turbo",
        LocalSttFamily::WhisperLargeV3 => "whisper_large_v3",
        LocalSttFamily::ParakeetTdt06bV3 => "parakeet_tdt_0_6b_v3",
        LocalSttFamily::Qwen3Asr06b => "qwen3_asr_0_6b",
        LocalSttFamily::Qwen3Asr17b => "qwen3_asr_1_7b",
    }
}

fn quant_api_str(quant: LocalSttQuant) -> &'static str {
    match quant {
        LocalSttQuant::Legacy => "legacy",
        LocalSttQuant::Q4_0 => "q4_0",
        LocalSttQuant::Q5 => "q5",
        LocalSttQuant::Q8_0 => "q8_0",
        LocalSttQuant::Int8 => "int8",
        LocalSttQuant::Fp16 => "fp16",
        LocalSttQuant::Fp32 => "fp32",
    }
}

pub fn parse_variant_id(id: &str) -> Option<LocalSttVariant> {
    for family in LocalSttFamily::all() {
        let prefix = format!("{}_", family_api_str(family));
        let Some(rest) = id.strip_prefix(&prefix) else {
            continue;
        };
        let quant = match rest {
            "legacy" => LocalSttQuant::Legacy,
            "q4_0" => LocalSttQuant::Q4_0,
            "q5" => LocalSttQuant::Q5,
            "q8_0" => LocalSttQuant::Q8_0,
            "int8" => LocalSttQuant::Int8,
            "fp16" => LocalSttQuant::Fp16,
            "fp32" => LocalSttQuant::Fp32,
            _ => continue,
        };
        return Some(LocalSttVariant { family, quant });
    }
    None
}

pub fn quants_for_family(family: LocalSttFamily) -> &'static [LocalSttQuant] {
    match family {
        LocalSttFamily::WhisperBase => &[
            LocalSttQuant::Legacy,
            LocalSttQuant::Q4_0,
            LocalSttQuant::Q5,
            LocalSttQuant::Q8_0,
        ],
        LocalSttFamily::WhisperSmall => &[
            LocalSttQuant::Legacy,
            LocalSttQuant::Q4_0,
            LocalSttQuant::Q5,
            LocalSttQuant::Q8_0,
        ],
        LocalSttFamily::WhisperMedium => &[
            LocalSttQuant::Legacy,
            LocalSttQuant::Q4_0,
            LocalSttQuant::Q5,
            LocalSttQuant::Q8_0,
        ],
        LocalSttFamily::WhisperLargeV3Turbo => &[
            LocalSttQuant::Legacy,
            LocalSttQuant::Q5,
            LocalSttQuant::Q8_0,
        ],
        LocalSttFamily::WhisperLargeV3 => &[
            LocalSttQuant::Legacy,
            LocalSttQuant::Q5,
        ],
        LocalSttFamily::ParakeetTdt06bV3 => {
            &[LocalSttQuant::Int8, LocalSttQuant::Fp16, LocalSttQuant::Fp32]
        }
        LocalSttFamily::Qwen3Asr06b | LocalSttFamily::Qwen3Asr17b => &[LocalSttQuant::Int8],
    }
}

fn whisper_quant_suffix(family: LocalSttFamily, quant: LocalSttQuant) -> Option<&'static str> {
    let slug = family.whisper_slug()?;
    match quant {
        LocalSttQuant::Legacy => Some(match slug {
            "base" => "ggml-base.bin",
            "small" => "ggml-small.bin",
            "medium" => "ggml-medium.bin",
            "large-v3-turbo" => "ggml-large-v3-turbo.bin",
            "large-v3" => "ggml-large-v3.bin",
            _ => return None,
        }),
        LocalSttQuant::Q4_0 => Some(match slug {
            "base" => "ggml-base-q4_0.bin",
            "small" => "ggml-small-q4_0.bin",
            "medium" => "ggml-medium-q4_0.bin",
            _ => return None,
        }),
        LocalSttQuant::Q5 => Some(match slug {
            "base" | "small" => match slug {
                "base" => "ggml-base-q5_1.bin",
                "small" => "ggml-small-q5_1.bin",
                _ => unreachable!(),
            },
            "medium" | "large-v3-turbo" | "large-v3" => match slug {
                "medium" => "ggml-medium-q5_0.bin",
                "large-v3-turbo" => "ggml-large-v3-turbo-q5_0.bin",
                "large-v3" => "ggml-large-v3-q5_0.bin",
                _ => unreachable!(),
            },
            _ => return None,
        }),
        LocalSttQuant::Q8_0 => Some(match slug {
            "base" => "ggml-base-q8_0.bin",
            "small" => "ggml-small-q8_0.bin",
            "medium" => "ggml-medium-q8_0.bin",
            "large-v3-turbo" => "ggml-large-v3-turbo-q8_0.bin",
            _ => return None,
        }),
        _ => None,
    }
}

fn whisper_download_url(file_name: &str) -> String {
    format!("{HF_WHISPER}/{file_name}")
}

fn base_whisper_tiers(family: LocalSttFamily) -> (u32, u32, u8, u8, &'static str) {
    match family {
        LocalSttFamily::WhisperBase => (141, 2_048, 5, 2, "settings.sttModelBaseFeatures"),
        LocalSttFamily::WhisperSmall => (466, 4_096, 4, 3, "settings.sttModelSmallFeatures"),
        LocalSttFamily::WhisperMedium => (1_500, 8_192, 3, 4, "settings.sttModelMediumFeatures"),
        LocalSttFamily::WhisperLargeV3Turbo => {
            (1_500, 8_192, 3, 4, "settings.sttModelLargeV3TurboFeatures")
        }
        LocalSttFamily::WhisperLargeV3 => (3_100, 8_192, 2, 5, "settings.sttModelLargeV3Features"),
        _ => (0, 0, 3, 3, "settings.sttModelBaseFeatures"),
    }
}

fn apply_quant_heuristics(
    base_size_mb: u32,
    base_ram_mb: u32,
    base_vram: Option<u32>,
    base_speed: u8,
    base_accuracy: u8,
    quant: LocalSttQuant,
) -> (u32, u32, Option<u32>, u8, u8) {
    match quant {
        LocalSttQuant::Legacy => (base_size_mb, base_ram_mb, base_vram, base_speed, base_accuracy),
        LocalSttQuant::Q4_0 => (
            (base_size_mb * 35) / 100,
            (base_ram_mb * 75) / 100,
            base_vram.map(|v| (v * 75) / 100),
            (base_speed + 1).min(5),
            base_accuracy.saturating_sub(1).max(1),
        ),
        LocalSttQuant::Q5 => (
            (base_size_mb * 45) / 100,
            (base_ram_mb * 85) / 100,
            base_vram.map(|v| (v * 85) / 100),
            (base_speed + 1).min(5),
            base_accuracy,
        ),
        LocalSttQuant::Q8_0 => (
            (base_size_mb * 55) / 100,
            base_ram_mb,
            base_vram,
            base_speed,
            (base_accuracy + 1).min(5),
        ),
        LocalSttQuant::Int8 => (
            base_size_mb,
            base_ram_mb,
            base_vram,
            base_speed,
            base_accuracy,
        ),
        LocalSttQuant::Fp16 => (
            (base_size_mb * 180) / 100,
            (base_ram_mb * 130) / 100,
            base_vram.map(|v| (v * 130) / 100),
            base_speed.saturating_sub(1).max(1),
            (base_accuracy + 1).min(5),
        ),
        LocalSttQuant::Fp32 => (
            (base_size_mb * 250) / 100,
            (base_ram_mb * 160) / 100,
            base_vram.map(|v| (v * 160) / 100),
            base_speed.saturating_sub(2).max(1),
            (base_accuracy + 1).min(5),
        ),
    }
}

pub fn variant_spec(variant: LocalSttVariant) -> SttVariantSpec {
    if variant.family.is_whisper() {
        return whisper_spec(variant);
    }
    sherpa_spec(variant)
}

fn whisper_spec(variant: LocalSttVariant) -> SttVariantSpec {
    let (base_size, base_ram, base_speed, base_accuracy, features) =
        base_whisper_tiers(variant.family);
    let base_vram = match variant.family {
        LocalSttFamily::WhisperLargeV3 | LocalSttFamily::WhisperLargeV3Turbo => Some(4_096),
        LocalSttFamily::WhisperMedium => Some(2_048),
        _ => None,
    };
    let file_name = whisper_quant_suffix(variant.family, variant.quant);
    let available = file_name.is_some();
    let (download_size_mb, ram_mb, vram_mb, speed_tier, accuracy_tier) = if available {
        apply_quant_heuristics(
            base_size,
            base_ram,
            base_vram,
            base_speed,
            base_accuracy,
            variant.quant,
        )
    } else {
        (0, base_ram, base_vram, base_speed, base_accuracy)
    };
    SttVariantSpec {
        variant,
        engine: LocalSttEngine::Whisper,
        available,
        download_size_mb,
        ram_mb,
        vram_mb,
        speed_tier,
        accuracy_tier,
        features_i18n_key: features,
        whisper_file_name: file_name,
        sherpa_archive_url: None,
        sherpa_archive_name: None,
        sherpa_layout: None,
    }
}

fn sherpa_spec(variant: LocalSttVariant) -> SttVariantSpec {
    let (base_size, base_ram, base_speed, base_accuracy, features, layout, url, archive) =
        match (variant.family, variant.quant) {
            (LocalSttFamily::ParakeetTdt06bV3, LocalSttQuant::Int8) => (
                650,
                4_096,
                4,
                4,
                "settings.sttModelParakeetFeatures",
                SherpaOnnxLayout::NemoInt8,
                Some("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2"),
                Some("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2"),
            ),
            (LocalSttFamily::ParakeetTdt06bV3, LocalSttQuant::Fp16) => (
                900,
                6_144,
                3,
                4,
                "settings.sttModelParakeetFeatures",
                SherpaOnnxLayout::NemoFpOnnx,
                Some("https://huggingface.co/Yiivgeny/parakeet-tdt-0.6b-v3-sherpa-onnx-fp16/resolve/main/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-fp16.tar.bz2"),
                Some("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-fp16.tar.bz2"),
            ),
            (LocalSttFamily::ParakeetTdt06bV3, LocalSttQuant::Fp32) => (
                1_800,
                8_192,
                2,
                5,
                "settings.sttModelParakeetFeatures",
                SherpaOnnxLayout::NemoFpOnnx,
                Some("https://huggingface.co/Yiivgeny/parakeet-tdt-0.6b-v3-sherpa-onnx-fp32/resolve/main/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-fp32.tar.bz2"),
                Some("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-fp32.tar.bz2"),
            ),
            (LocalSttFamily::Qwen3Asr06b, LocalSttQuant::Int8) => (
                950,
                8_192,
                3,
                4,
                "settings.sttModelQwen06Features",
                SherpaOnnxLayout::Qwen3Int8,
                Some("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2"),
                Some("sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2"),
            ),
            (LocalSttFamily::Qwen3Asr17b, LocalSttQuant::Int8) => (
                2_200,
                16_384,
                2,
                5,
                "settings.sttModelQwen17Features",
                SherpaOnnxLayout::Qwen3Int8,
                Some("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-qwen3-asr-1.7B-int8-2026-03-25.tar.bz2"),
                Some("sherpa-onnx-qwen3-asr-1.7B-int8-2026-03-25.tar.bz2"),
            ),
            _ => (
                0,
                0,
                3,
                3,
                "settings.sttModelBaseFeatures",
                SherpaOnnxLayout::NemoInt8,
                None,
                None,
            ),
        };

    let available = url.is_some();
    let (download_size_mb, ram_mb, vram_mb, speed_tier, accuracy_tier) = apply_quant_heuristics(
        base_size,
        base_ram,
        Some(2_048),
        base_speed,
        base_accuracy,
        variant.quant,
    );

    SttVariantSpec {
        variant,
        engine: LocalSttEngine::Sherpa,
        available,
        download_size_mb,
        ram_mb,
        vram_mb,
        speed_tier,
        accuracy_tier,
        features_i18n_key: features,
        whisper_file_name: None,
        sherpa_archive_url: url,
        sherpa_archive_name: archive,
        sherpa_layout: Some(layout),
    }
}

pub fn whisper_download_url_for(variant: LocalSttVariant) -> Option<String> {
    let spec = variant_spec(variant);
    spec.whisper_file_name
        .map(|name| whisper_download_url(name))
}

pub fn normalize_variant(variant: LocalSttVariant) -> LocalSttVariant {
    let quants = quants_for_family(variant.family);
    if quants.contains(&variant.quant) && variant_spec(variant).available {
        return variant;
    }
    let quant = quants
        .iter()
        .copied()
        .find(|q| variant_spec(LocalSttVariant::new(variant.family, *q)).available)
        .unwrap_or(quants[0]);
    LocalSttVariant::new(variant.family, quant)
}

pub fn legacy_whisper_kind_for_family(family: LocalSttFamily) -> Option<WhisperModelKind> {
    match family {
        LocalSttFamily::WhisperBase => Some(WhisperModelKind::Base),
        LocalSttFamily::WhisperSmall => Some(WhisperModelKind::Small),
        LocalSttFamily::WhisperMedium => Some(WhisperModelKind::Medium),
        LocalSttFamily::WhisperLargeV3Turbo => Some(WhisperModelKind::LargeV3Turbo),
        LocalSttFamily::WhisperLargeV3 => Some(WhisperModelKind::LargeV3),
        _ => None,
    }
}

pub fn migrate_from_legacy_stt_model(kind: LocalSttModelKind) -> LocalSttVariant {
    LocalSttVariant::from_legacy_kind(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_base_q8_has_url() {
        let v = LocalSttVariant::new(LocalSttFamily::WhisperBase, LocalSttQuant::Q8_0);
        let spec = variant_spec(v);
        assert!(spec.available);
        assert_eq!(spec.whisper_file_name, Some("ggml-base-q8_0.bin"));
    }

    #[test]
    fn large_v3_no_q4() {
        let v = LocalSttVariant::new(LocalSttFamily::WhisperLargeV3, LocalSttQuant::Q4_0);
        assert!(!variant_spec(v).available);
    }

    #[test]
    fn variant_id_roundtrip() {
        let v = LocalSttVariant::new(LocalSttFamily::ParakeetTdt06bV3, LocalSttQuant::Int8);
        let id = v.as_api_id();
        let parsed = parse_variant_id(&id).expect("parse");
        assert_eq!(parsed, v);
    }
}
