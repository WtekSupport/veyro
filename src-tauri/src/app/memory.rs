use tauri::{AppHandle, Manager};

use crate::app::context::AppContext;
use crate::llm::model_store::{needs_local_llm, needs_local_whisper};
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

pub fn needs_whisper_prewarm(settings: &crate::settings::AppSettings) -> bool {
    needs_local_whisper(settings)
}

pub fn needs_llm_prewarm(settings: &crate::settings::AppSettings) -> bool {
    needs_local_llm(settings)
}
