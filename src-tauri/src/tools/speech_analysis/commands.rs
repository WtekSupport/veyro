use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::app::context::AppContext;
use crate::settings::{AppSettings, LocalSttVariant};

use super::ensure_models::ensure_plan_models;
use super::model_plan::{resolve_plan, SpeechAnalysisPlan};
use super::model_registry::{list_model_statuses, SpeechModelStatusDto};
use super::disk_cache;
use super::speaker_profiles::{
    ensure_speaker_profile, list_speaker_profiles, SpeakerProfileSummary,
};
use super::types::{SpeechAnalysisOptions, SpeechAnalysisReport};

fn settings_from_ctx(ctx: &AppContext) -> Result<AppSettings, String> {
    ctx.controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())
        .map(|controller| controller.settings().clone())
}

pub fn list_speech_analysis_models(
    ctx: State<'_, Arc<AppContext>>,
    lang: Option<String>,
) -> Result<Vec<SpeechModelStatusDto>, String> {
    let settings = settings_from_ctx(ctx.inner())?;
    let session = ctx.inner().speech_analysis_models.snapshot();
    Ok(list_model_statuses(
        &settings,
        lang.as_deref(),
        &session,
    ))
}

pub fn speech_analysis_resolve_plan(
    ctx: State<'_, Arc<AppContext>>,
    lang: Option<String>,
    options: Option<SpeechAnalysisOptions>,
) -> Result<SpeechAnalysisPlan, String> {
    let settings = settings_from_ctx(ctx.inner())?;
    let options = options.unwrap_or_default();
    Ok(resolve_plan(
        &settings,
        lang.as_deref().or(options.stt_language_override.as_deref()),
        options.model_policy,
        options.manual_variant.map(LocalSttVariant::from),
    ))
}

pub async fn speech_analysis_ensure_models(
    app: AppHandle,
    ctx: State<'_, Arc<AppContext>>,
    plan: SpeechAnalysisPlan,
    path_key: String,
) -> Result<(), String> {
    let settings = settings_from_ctx(ctx.inner())?;
    ensure_plan_models(
        &app,
        ctx.inner(),
        &settings,
        &plan,
        &path_key,
    )
    .await
}

pub fn speech_analysis_load_disk_cache(
    ctx: State<'_, Arc<AppContext>>,
    path_key: String,
) -> Result<Option<SpeechAnalysisReport>, String> {
    let settings = settings_from_ctx(ctx.inner())?;
    disk_cache::load_report_snapshot(&settings, &path_key)
}

pub fn list_speech_analysis_speaker_profiles() -> Vec<SpeakerProfileSummary> {
    list_speaker_profiles()
}

pub fn create_speech_analysis_speaker_profile(label: String) -> Result<SpeakerProfileSummary, String> {
    ensure_speaker_profile(&label)
}

pub fn list_speech_analysis_tongue_twisters(
) -> Vec<super::reference_presets::TongueTwisterPreset> {
    super::reference_presets::list_tongue_twister_presets()
}
