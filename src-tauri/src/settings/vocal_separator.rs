use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VocalSeparatorProfile {
    #[default]
    Quality,
    Fast,
    Legacy,
    /// Separate downloadable `htdemucs_6s` bundle for instrument split (not a base profile).
    #[serde(rename = "multi-stem")]
    MultiStem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VocalSeparatorOutputFormat {
    #[default]
    Wav,
    MatchSource,
    Flac,
}

impl VocalSeparatorProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quality => "quality",
            Self::Fast => "fast",
            Self::Legacy => "legacy",
            Self::MultiStem => "multi-stem",
        }
    }

    pub fn is_base_profile(self) -> bool {
        matches!(self, Self::Quality | Self::Fast | Self::Legacy)
    }
}

#[cfg(feature = "local-separation")]
impl From<VocalSeparatorProfile> for veyro_separation::SeparationProfile {
    fn from(value: VocalSeparatorProfile) -> Self {
        match value {
            VocalSeparatorProfile::Quality => Self::Quality,
            VocalSeparatorProfile::Fast => Self::Fast,
            VocalSeparatorProfile::Legacy => Self::Legacy,
            VocalSeparatorProfile::MultiStem => Self::MultiStem,
        }
    }
}
