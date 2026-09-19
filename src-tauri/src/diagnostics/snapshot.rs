use tauri::AppHandle;

use crate::app::context::AppContext;
use crate::app::memory::collect_memory_snapshot;
use crate::app::state::DiagnosticsSnapshot;
use crate::settings::{
    has_api_key, local_llm_compiled, local_llm_gpu_compiled, sherpa_stt_compiled,
    whisper_gpu_compiled, whisper_local_compiled, LocalSttEngine,
};
use crate::transcription::model_store::whisper_backend_label;

pub fn collect_diagnostics(ctx: &AppContext, app: &AppHandle) -> DiagnosticsSnapshot {
    let memory = collect_memory_snapshot(app, ctx);
    let llm_ready = ctx
        .llm_engine
        .read()
        .map(|engine| engine.is_ready())
        .unwrap_or(false);
    let audio_running;
    let audio_capturing;
    let active_microphone;
    if let Ok(audio) = ctx.audio.try_lock() {
        audio_running = audio.is_running();
        audio_capturing = audio.is_capturing();
        active_microphone = audio.active_input_device();
    } else {
        audio_running = false;
        audio_capturing = false;
        active_microphone = None;
    }

    let pending_segments = ctx.runtime.pending_count();
    let callbacks_enabled = ctx.audio_callbacks_enabled();

    if let Ok(controller) = ctx.controller.try_lock() {
        let status = controller.status();
        return DiagnosticsSnapshot {
            state: status.state,
            audio_running,
            audio_capturing,
            push_to_talk: status.push_to_talk,
            hotkey: status.hotkey,
            injection_available: status.injection_available,
            injection_backend: status.injection_backend,
            has_api_key: has_api_key(),
            microphone_device: active_microphone.or(status.microphone_device),
            pending_segments,
            callbacks_enabled,
            whisper_backend: whisper_backend_label().to_string(),
            whisper_local_compiled: whisper_local_compiled(),
            whisper_gpu_compiled: whisper_gpu_compiled(),
            capslock_ptt_supported: crate::hotkey::capslock::SUPPORTED,
            hotkey_game_mode_supported: crate::hotkey::game_mode_supported(),
            hotkey_backend: crate::game_input::backend_label().to_string(),
            hook_active: crate::game_input::hook_active(),
            text_processing_mode: controller.settings().text_processing_mode.as_str().to_string(),
            transcription_provider: controller.settings().transcription_provider.clone(),
            text_rewrite_provider: controller
                .settings()
                .text_rewrite_provider
                .as_str()
                .to_string(),
            local_llm_model: controller.settings().local_llm_model.as_str().to_string(),
            local_llm_compiled: local_llm_compiled(),
            local_llm_gpu_compiled: local_llm_gpu_compiled(),
            local_llm_ready: llm_ready,
            whisper_loaded: memory.whisper_loaded,
            local_stt_loaded: memory.whisper_loaded,
            local_stt_engine: match controller.settings().local_stt_model.engine() {
                LocalSttEngine::Whisper => "whisper".to_string(),
                LocalSttEngine::Sherpa => "sherpa".to_string(),
            },
            local_stt_model: controller.settings().local_stt_model.as_api_str().to_string(),
            sherpa_stt_compiled: sherpa_stt_compiled(),
            llm_loaded: memory.llm_loaded,
            settings_webview_alive: memory.settings_webview_alive,
            about_webview_alive: memory.about_webview_alive,
            process_elevated: crate::game_input::is_process_elevated(),
            hotkeys_blocked_by_elevation: crate::game_input::hotkeys_blocked_by_foreground_elevation(),
            show_notifications: controller.settings().show_notifications,
            system_notifications_available: crate::notify::system_notifications_available(),
        };
    }

    DiagnosticsSnapshot {
        state: crate::app::state::AppState::Initializing,
        audio_running,
        audio_capturing: false,
        push_to_talk: false,
        hotkey: String::new(),
        injection_available: false,
        injection_backend: "unknown".to_string(),
        has_api_key: has_api_key(),
        microphone_device: None,
        pending_segments,
        callbacks_enabled,
        whisper_backend: whisper_backend_label().to_string(),
        whisper_local_compiled: whisper_local_compiled(),
        whisper_gpu_compiled: whisper_gpu_compiled(),
        capslock_ptt_supported: crate::hotkey::capslock::SUPPORTED,
        hotkey_game_mode_supported: crate::hotkey::game_mode_supported(),
        hotkey_backend: crate::game_input::backend_label().to_string(),
        hook_active: crate::game_input::hook_active(),
        text_processing_mode: "unknown".to_string(),
        transcription_provider: "unknown".to_string(),
        text_rewrite_provider: "unknown".to_string(),
        local_llm_model: "unknown".to_string(),
        local_llm_compiled: local_llm_compiled(),
        local_llm_gpu_compiled: local_llm_gpu_compiled(),
        local_llm_ready: llm_ready,
        whisper_loaded: memory.whisper_loaded,
        local_stt_loaded: memory.whisper_loaded,
        local_stt_engine: "unknown".to_string(),
        local_stt_model: "unknown".to_string(),
        sherpa_stt_compiled: sherpa_stt_compiled(),
        llm_loaded: memory.llm_loaded,
        settings_webview_alive: memory.settings_webview_alive,
        about_webview_alive: memory.about_webview_alive,
        process_elevated: crate::game_input::is_process_elevated(),
        hotkeys_blocked_by_elevation: crate::game_input::hotkeys_blocked_by_foreground_elevation(),
        show_notifications: false,
        system_notifications_available: crate::notify::system_notifications_available(),
    }
}

