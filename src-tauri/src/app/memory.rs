use tauri::{AppHandle, Manager};

use crate::app::context::AppContext;
use crate::llm::model_store::{needs_local_llm, needs_local_whisper};
use crate::settings::{sherpa_stt_compiled, whisper_local_compiled, AppSettings};
use crate::transcription::local_stt_model_store;
use crate::window::{ABOUT_WINDOW_LABEL, SETTINGS_WINDOW_LABEL};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct MemorySnapshot {
    pub whisper_loaded: bool,
    pub llm_loaded: bool,
    pub settings_webview_alive: bool,
    pub about_webview_alive: bool,
}

pub fn collect_memory_snapshot(app: &AppHandle, ctx: &AppContext) -> MemorySnapshot {
    let whisper_loaded = ctx.runtime.is_whisper_model_loaded();
    let llm_loaded = ctx
        .llm_engine
        .read()
        .map(|engine| engine.is_ready())
        .unwrap_or(false);

    MemorySnapshot {
        whisper_loaded,
        llm_loaded,
        settings_webview_alive: app.get_webview_window(SETTINGS_WINDOW_LABEL).is_some(),
        about_webview_alive: app.get_webview_window(ABOUT_WINDOW_LABEL).is_some(),
    }
}

pub fn needs_local_stt(settings: &AppSettings) -> bool {
    settings.transcription_provider == "local"
        && (whisper_local_compiled() || sherpa_stt_compiled())
}

pub fn selected_local_stt_ready(settings: &AppSettings) -> bool {
    local_stt_model_store::resolve_model_bundle(settings)
        .ok()
        .is_some_and(|path| {
            local_stt_model_store::bundle_ready(&path, settings.local_stt_model)
        })
}

pub fn needs_whisper_prewarm(settings: &AppSettings) -> bool {
    needs_local_whisper(settings)
}

pub fn needs_llm_prewarm(settings: &AppSettings) -> bool {
    needs_local_llm(settings)
}
