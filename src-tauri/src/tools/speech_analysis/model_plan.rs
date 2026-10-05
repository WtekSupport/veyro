use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::settings::{AppSettings, LocalSttFamily, LocalSttQuant, LocalSttVariant};
use crate::transcription::local_stt_model_store::bundle_ready_for_settings;

use super::model_registry::{catalog_entries, caps_for_variant, SttCapability, SpeechModelEntry};

pub const METRIC_NEEDS: &[(&str, &[SttCapability])] = &[
    ("confidence", &[SttCapability::WordProbs]),
    ("intelligibility", &[SttCapability::WordProbs]),
    (
        "articulation",
        &[SttCapability::CtcPosteriors, SttCapability::CharTiming],
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeechAnalysisModelPolicy {
    #[default]
    Auto,
    FollowGlobal,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisPlan {
    pub main: Option<LocalSttVariantDto>,
    pub aux: Vec<LocalSttVariantDto>,
    pub to_download: Vec<LocalSttVariantDto>,
    pub satisfied_caps: Vec<SttCapability>,
    pub missing_caps: Vec<SttCapability>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalSttVariantDto {
    pub family: LocalSttFamily,
    pub quant: LocalSttQuant,
}

impl From<LocalSttVariant> for LocalSttVariantDto {
    fn from(value: LocalSttVariant) -> Self {
        Self {
            family: value.family,
            quant: value.quant,
        }
    }
}

impl From<LocalSttVariantDto> for LocalSttVariant {
    fn from(value: LocalSttVariantDto) -> Self {
        LocalSttVariant::new(value.family, value.quant)
    }
}

pub fn union_metric_needs() -> HashSet<SttCapability> {
    let mut needs = HashSet::new();
    needs.insert(SttCapability::Text);
    for (_, caps) in METRIC_NEEDS {
        for cap in *caps {
            needs.insert(*cap);
        }
    }
    needs
}

pub fn resolve_plan(
    settings: &AppSettings,
    lang: Option<&str>,
    policy: SpeechAnalysisModelPolicy,
    manual_variant: Option<LocalSttVariant>,
) -> SpeechAnalysisPlan {
    let lang = lang.unwrap_or("ru");
    let needs = union_metric_needs();

    let installed: Vec<LocalSttVariant> = catalog_entries()
        .iter()
        .filter(|e| lang_matches(e, lang))
        .filter(|e| bundle_ready_for_settings(settings, e.variant))
        .map(|e| e.variant)
        .collect();

    if policy == SpeechAnalysisModelPolicy::Manual {
        if let Some(manual) = manual_variant {
            let caps = caps_for_variant(manual);
            let missing: Vec<_> = needs
                .iter()
                .filter(|cap| !caps.contains(cap))
                .copied()
                .collect();
            return SpeechAnalysisPlan {
                main: Some(manual.into()),
                aux: Vec::new(),
                to_download: recommend_downloads(&missing, lang, settings),
                satisfied_caps: caps.intersection(&needs).copied().collect(),
                missing_caps: missing,
            };
        }
    }

    if let Some(single) = pick_best_covering(&installed, &needs, lang) {
        let caps = caps_for_variant(single);
        return SpeechAnalysisPlan {
            main: Some(single.into()),
            aux: Vec::new(),
            to_download: Vec::new(),
            satisfied_caps: caps.intersection(&needs).copied().collect(),
            missing_caps: needs
                .iter()
                .filter(|c| !caps.contains(c))
                .copied()
                .collect(),
        };
    }

    let main = pick_main_transcriber(settings, policy, &installed, lang);
    let main_caps = main.map(caps_for_variant).unwrap_or_default();
    let mut covered = main_caps.clone();
    let mut aux = Vec::new();

    for entry in catalog_entries().iter().filter(|e| lang_matches(e, lang)) {
        if !bundle_ready_for_settings(settings, entry.variant) {
            continue;
        }
        if main.is_some_and(|m| m == entry.variant) {
            continue;
        }
        let adds: HashSet<_> = entry
            .caps
            .iter()
            .filter(|c| needs.contains(c) && !covered.contains(c))
            .copied()
            .collect();
        if !adds.is_empty() {
            covered.extend(adds);
            aux.push(entry.variant);
        }
    }

    let missing: Vec<_> = needs
        .iter()
        .filter(|c| !covered.contains(c))
        .copied()
        .collect();

    SpeechAnalysisPlan {
        main: main.map(Into::into),
        aux: aux.into_iter().map(Into::into).collect(),
        to_download: recommend_downloads(&missing, lang, settings),
        satisfied_caps: covered.intersection(&needs).copied().collect(),
        missing_caps: missing,
    }
}

fn lang_matches(entry: &SpeechModelEntry, lang: &str) -> bool {
    entry.lang_tags.contains(&"multi") || entry.lang_tags.contains(&lang)
}

fn pick_best_covering(
    installed: &[LocalSttVariant],
    needs: &HashSet<SttCapability>,
    lang: &str,
) -> Option<LocalSttVariant> {
    let mut best: Option<(u8, LocalSttVariant)> = None;
    for variant in installed {
        let caps = caps_for_variant(*variant);
        if needs.iter().all(|need| caps.contains(need)) {
            let priority = catalog_entries()
                .iter()
                .find(|e| e.variant == *variant && lang_matches(e, lang))
                .map(|e| e.priority)
                .unwrap_or(100);
            if best.as_ref().is_none_or(|(p, _)| priority < *p) {
                best = Some((priority, *variant));
            }
        }
    }
    best.map(|(_, v)| v)
}

fn pick_main_transcriber(
    settings: &AppSettings,
    policy: SpeechAnalysisModelPolicy,
    installed: &[LocalSttVariant],
    lang: &str,
) -> Option<LocalSttVariant> {
    if policy == SpeechAnalysisModelPolicy::FollowGlobal {
        let global = settings.local_stt_variant();
        if bundle_ready_for_settings(settings, global) {
            return Some(global);
        }
    }
    for entry in catalog_entries().iter().filter(|e| lang_matches(e, lang)) {
        if entry.caps.contains(&SttCapability::Text)
            && installed.contains(&entry.variant)
        {
            return Some(entry.variant);
        }
    }
    installed.first().copied()
}

fn recommend_downloads(
    missing: &[SttCapability],
    lang: &str,
    settings: &AppSettings,
) -> Vec<LocalSttVariantDto> {
    if missing.is_empty() {
        return Vec::new();
    }
    let catalog = catalog_entries();
    let mut candidates: Vec<&SpeechModelEntry> = catalog
        .iter()
        .filter(|e| lang_matches(e, lang))
        .filter(|e| !bundle_ready_for_settings(settings, e.variant))
        .collect();
    candidates.sort_by_key(|e| e.priority);

    let mut out = Vec::new();
    let mut still_missing: HashSet<_> = missing.iter().copied().collect();
    for entry in candidates {
        let covers: HashSet<_> = entry
            .caps
            .iter()
            .filter(|c| still_missing.contains(c))
            .copied()
            .collect();
        if covers.is_empty() && !entry.caps.contains(&SttCapability::CtcPosteriors) {
            continue;
        }
        if entry.variant.family == LocalSttFamily::GigaAmV3E2eCtc
            && still_missing.iter().any(|c| {
                matches!(
                    c,
                    SttCapability::CtcPosteriors
                        | SttCapability::CharTiming
                        | SttCapability::WordProbs
                )
            })
        {
            out.push(entry.variant.into());
            still_missing.retain(|c| !entry.caps.contains(c));
        } else if !covers.is_empty() {
            out.push(entry.variant.into());
            for cap in &entry.caps {
                still_missing.remove(cap);
            }
        }
        if still_missing.is_empty() {
            break;
        }
    }

    if out.is_empty()
        && still_missing
            .iter()
            .any(|c| matches!(c, SttCapability::CtcPosteriors | SttCapability::CharTiming))
    {
        let fallback = LocalSttVariant::new(LocalSttFamily::GigaAmV3E2eCtc, LocalSttQuant::Int8);
        if !bundle_ready_for_settings(settings, fallback) {
            out.push(fallback.into());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppSettings;

    #[test]
    fn plan_recommends_ctc_when_nothing_installed() {
        let settings = AppSettings::default();
        let plan = resolve_plan(&settings, Some("ru"), SpeechAnalysisModelPolicy::Auto, None);
        let metric_caps = [
            SttCapability::CtcPosteriors,
            SttCapability::CharTiming,
            SttCapability::WordProbs,
        ];
        if metric_caps.iter().all(|cap| plan.satisfied_caps.contains(cap)) {
            return;
        }
        assert!(
            plan.to_download
                .iter()
                .any(|v| v.family == LocalSttFamily::GigaAmV3E2eCtc),
            "missing caps should recommend GigaAM CTC download"
        );
    }
}
