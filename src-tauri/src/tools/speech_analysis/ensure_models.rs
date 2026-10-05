use std::sync::Arc;

use tauri::AppHandle;

use crate::app::context::AppContext;
use crate::app::events::{
    emit_speech_analysis_progress, SpeechAnalysisProgressPayload, SpeechAnalysisProgressPhase,
};
use crate::settings::{AppSettings, LocalSttVariant};
use crate::transcription::local_stt_model_store::{self, bundle_ready_for_settings};

use super::model_plan::SpeechAnalysisPlan;
use crate::speech_analysis_models::ModelInstallStatus;

pub async fn ensure_plan_models(
    app: &AppHandle,
    ctx: &Arc<AppContext>,
    settings: &AppSettings,
    plan: &SpeechAnalysisPlan,
    path_key: &str,
) -> Result<(), String> {
    if plan.to_download.is_empty() {
        return Ok(());
    }

    let http = local_stt_model_store::download_client_for_stt()
        .map_err(|error| error.to_string())?;
    let session = &ctx.speech_analysis_models;

    for dto in &plan.to_download {
        let variant = LocalSttVariant::from(*dto);
        let variant_id = variant.as_api_id();
        if bundle_ready_for_settings(settings, variant) {
            continue;
        }

        session.set_status(&variant_id, ModelInstallStatus::Downloading);
        emit_speech_analysis_progress(
            app,
            SpeechAnalysisProgressPayload {
                path: path_key.to_string(),
                phase: SpeechAnalysisProgressPhase::DownloadingModel,
                percent: Some(0),
            },
        );

        let app_progress = app.clone();
        let path_key_progress = path_key.to_string();
        let download_result = local_stt_model_store::download_model(
            &http,
            settings,
            variant,
            |progress| {
                let percent = progress
                    .percent
                    .map(|value| value.min(99.0).round() as u8);
                emit_speech_analysis_progress(
                    &app_progress,
                    SpeechAnalysisProgressPayload {
                        path: path_key_progress.clone(),
                        phase: SpeechAnalysisProgressPhase::DownloadingModel,
                        percent,
                    },
                );
            },
        )
        .await;

        match download_result {
            Ok(_) => {
                session.set_status(&variant_id, ModelInstallStatus::Verifying);
                if bundle_ready_for_settings(settings, variant) {
                    session.clear_transient(&variant_id);
                } else {
                    session.set_failed(
                        &variant_id,
                        "tools.speechAnalysis.models.verifyFailed".to_string(),
                    );
                    return Err("tools.speechAnalysis.models.verifyFailed".to_string());
                }
            }
            Err(error) => {
                session.set_failed(
                    &variant_id,
                    format!("tools.speechAnalysis.models.downloadFailed|{error}"),
                );
                return Err(format!("tools.speechAnalysis.models.downloadFailed|{error}"));
            }
        }
    }
    Ok(())
}
