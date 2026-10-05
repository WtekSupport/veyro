use serde::{Deserialize, Serialize};

const TWISTERS_TOML: &str = include_str!("../../../resources/speech_analysis/tongue_twisters_ru.toml");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TongueTwisterPreset {
    pub id: String,
    pub title: String,
    pub text: String,
}

#[derive(Deserialize)]
struct PresetsFile {
    presets: Vec<TongueTwisterPreset>,
}

pub fn list_tongue_twister_presets() -> Vec<TongueTwisterPreset> {
    toml::from_str::<PresetsFile>(TWISTERS_TOML)
        .map(|f| f.presets)
        .unwrap_or_default()
}

pub fn preset_text(id: &str) -> Option<String> {
    list_tongue_twister_presets()
        .into_iter()
        .find(|p| p.id == id)
        .map(|p| p.text)
}
