use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::settings::{variant_spec, AppSettings, LocalSttFamily, LocalSttQuant, LocalSttVariant};
use crate::transcription::local_stt_model_store::bundle_ready_for_settings;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SttCapability {
    Text,
    WordTimestamps,
    WordProbs,
    CtcPosteriors,
    CharTiming,
}

#[derive(Debug, Clone)]
pub struct SpeechModelEntry {
    pub variant: LocalSttVariant,
    pub lang_tags: &'static [&'static str],
    pub priority: u8,
    pub caps: HashSet<SttCapability>,
}

pub use crate::speech_analysis_models::{ModelInstallStatus, SessionStatusRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModelStatusDto {
    pub variant_id: String,
    pub family: LocalSttFamily,
    pub quant: LocalSttQuant,
    pub lang_tags: Vec<String>,
    pub priority: u8,
    pub caps: Vec<SttCapability>,
    pub size_mb: u32,
    pub status: ModelInstallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_message_key: Option<String>,
}

pub fn catalog_entries() -> Vec<SpeechModelEntry> {
    use SttCapability::*;
    vec![
        SpeechModelEntry {
            variant: LocalSttVariant::new(LocalSttFamily::GigaAmV3E2eCtc, LocalSttQuant::Int8),
            lang_tags: &["ru"],
            priority: 1,
            caps: HashSet::from([Text, WordTimestamps, WordProbs, CtcPosteriors, CharTiming]),
        },
        SpeechModelEntry {
            variant: LocalSttVariant::new(LocalSttFamily::GigaAmV3E2eRnnt, LocalSttQuant::Int8),
            lang_tags: &["ru"],
            priority: 2,
            caps: HashSet::from([Text, WordTimestamps]),
        },
        SpeechModelEntry {
            variant: LocalSttVariant::new(LocalSttFamily::WhisperBase, LocalSttQuant::Legacy),
            lang_tags: &["multi"],
            priority: 50,
            caps: HashSet::from([Text, WordTimestamps]),
        },
        SpeechModelEntry {
            variant: LocalSttVariant::new(LocalSttFamily::WhisperSmall, LocalSttQuant::Q8_0),
            lang_tags: &["multi"],
            priority: 40,
            caps: HashSet::from([Text, WordTimestamps]),
        },
    ]
}

pub fn entry_for_variant(variant: LocalSttVariant) -> Option<SpeechModelEntry> {
    catalog_entries()
        .into_iter()
        .find(|entry| entry.variant.family == variant.family && entry.variant.quant == variant.quant)
        .or_else(|| {
            catalog_entries()
                .into_iter()
                .find(|entry| entry.variant.family == variant.family)
        })
}

pub fn caps_for_variant(variant: LocalSttVariant) -> HashSet<SttCapability> {
    entry_for_variant(variant)
        .map(|entry| entry.caps)
        .unwrap_or_else(|| HashSet::from([SttCapability::Text, SttCapability::WordTimestamps]))
}

pub fn list_model_statuses(
    settings: &AppSettings,
    lang: Option<&str>,
    session_status: &std::collections::HashMap<String, crate::speech_analysis_models::SessionStatusRecord>,
) -> Vec<SpeechModelStatusDto> {
    let lang = lang.unwrap_or("ru");
    catalog_entries()
        .into_iter()
        .filter(|entry| entry.lang_tags.contains(&"multi") || entry.lang_tags.contains(&lang))
        .map(|entry| {
            let variant_id = entry.variant.as_api_id();
            let spec = variant_spec(entry.variant);
            let disk_installed = bundle_ready_for_settings(settings, entry.variant);
            let status = resolve_status(&variant_id, disk_installed, session_status);
            SpeechModelStatusDto {
                variant_id: variant_id.clone(),
                family: entry.variant.family,
                quant: entry.variant.quant,
                lang_tags: entry.lang_tags.iter().map(|s| s.to_string()).collect(),
                priority: entry.priority,
                caps: entry.caps.iter().copied().collect(),
                size_mb: spec.download_size_mb,
                status,
                failed_message_key: session_status
                    .get(&variant_id)
                    .and_then(|r| r.failed_message_key.clone()),
            }
        })
        .collect()
}

fn resolve_status(
    variant_id: &str,
    disk_installed: bool,
    session_status: &std::collections::HashMap<String, SessionStatusRecord>,
) -> ModelInstallStatus {
    if let Some(record) = session_status.get(variant_id) {
        if record.status == ModelInstallStatus::Downloading
            || record.status == ModelInstallStatus::Verifying
            || record.status == ModelInstallStatus::Failed
        {
            return record.status;
        }
    }
    if disk_installed {
        ModelInstallStatus::Installed
    } else {
        ModelInstallStatus::NotInstalled
    }
}
