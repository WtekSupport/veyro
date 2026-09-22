use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::AppHandle;
use tracing::debug;

use crate::app::activity_log::ActivityLevel;
use crate::app::context::AppContext;
use crate::app::memory::{needs_local_stt, selected_local_stt_ready};
use crate::llm::LlmEngine;
use crate::llm::model_store::needs_local_llm;
use crate::settings::AppSettings;

const WATCH_INTERVAL: Duration = Duration::from_secs(30);

pub fn dictation_activity_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn is_dictation_activity(message_key: &str) -> bool {
    message_key.starts_with("activity.vad.")
        || message_key.starts_with("activity.ptt.")
        || message_key.starts_with("activity.inject.")
        || message_key.starts_with("activity.transcription.")
        || message_key.starts_with("activity.live.")
        || message_key.starts_with("activity.queue.")
        || message_key.starts_with("activity.ai.")
        || message_key.starts_with("activity.text.")
        || message_key == "activity.segment_no_api_key"
}

pub fn touch_dictation_activity(activity_ms: &AtomicU64) {
    activity_ms.store(dictation_activity_now_ms(), Ordering::Relaxed);
}

pub fn note_dictation_activity(activity_ms: &AtomicU64, message_key: &str) {
    if is_dictation_activity(message_key) {
        touch_dictation_activity(activity_ms);
    }
}

pub fn start_model_idle_watchdog(app: AppHandle, ctx: Arc<AppContext>) {
    thread::spawn(move || loop {
        thread::sleep(WATCH_INTERVAL);
        if let Err(error) = check_and_unload_idle_llm(&app, ctx.as_ref()) {
            debug!("LLM idle check skipped: {error}");
        }
        if let Err(error) = check_and_unload_idle_stt(&app, &ctx) {
            debug!("STT idle check skipped: {error}");
        }
        if let Err(error) = check_and_unload_idle_silero_te(ctx.as_ref()) {
            debug!("Silero TE idle check skipped: {error}");
        }
    });
}

fn idle_elapsed_ms(last_ms: u64, now_ms: u64, after: Duration) -> bool {
    now_ms.saturating_sub(last_ms) >= after.as_millis() as u64
}

fn check_and_unload_idle_llm(app: &AppHandle, ctx: &AppContext) -> Result<(), &'static str> {
    let settings = settings_snapshot(ctx)?;

    let Some(after) = settings.llm_idle_unload_after() else {
        return Ok(());
    };

    if !needs_local_llm(&settings) {
        return Ok(());
    }

    if !ctx
        .llm_engine
        .read()
        .map(|engine| engine.is_ready())
        .unwrap_or(false)
    {
        return Ok(());
    }

    if ctx.runtime.pending_count() > 0 {
        touch_dictation_activity(&ctx.llm_dictation_activity_ms);
        return Ok(());
    }

    let last_ms = ctx.llm_dictation_activity_ms.load(Ordering::Relaxed);
    let now_ms = dictation_activity_now_ms();
    if !idle_elapsed_ms(last_ms, now_ms, after) {
        return Ok(());
    }

    ctx.unload_local_llm_for_idle(Some(app));
    Ok(())
}

fn check_and_unload_idle_stt(app: &AppHandle, ctx: &Arc<AppContext>) -> Result<(), &'static str> {
    let settings = settings_snapshot(ctx.as_ref())?;

    let Some(after) = settings.stt_idle_unload_after() else {
        return Ok(());
    };

    if !needs_local_stt(&settings) || !selected_local_stt_ready(&settings) {
        return Ok(());
    }

    if !ctx.runtime.is_whisper_model_loaded() {
        return Ok(());
    }

    if ctx.runtime.pending_count() > 0 {
        touch_dictation_activity(&ctx.llm_dictation_activity_ms);
        return Ok(());
    }

    let last_ms = ctx.llm_dictation_activity_ms.load(Ordering::Relaxed);
    let now_ms = dictation_activity_now_ms();
    if !idle_elapsed_ms(last_ms, now_ms, after) {
        return Ok(());
    }

    let app = app.clone();
    let ctx = Arc::clone(ctx);
    tauri::async_runtime::spawn(async move {
        ctx.unload_local_stt_for_idle(Some(&app)).await;
    });
    Ok(())
}

fn check_and_unload_idle_silero_te(ctx: &AppContext) -> Result<(), &'static str> {
    let settings = settings_snapshot(ctx)?;
    if !settings.silero_te {
        return Ok(());
    }

    let Some(after) = settings.stt_idle_unload_after() else {
        return Ok(());
    };

    if ctx.runtime.pending_count() > 0 {
        touch_dictation_activity(&ctx.llm_dictation_activity_ms);
        return Ok(());
    }

    let last_ms = ctx.llm_dictation_activity_ms.load(Ordering::Relaxed);
    let now_ms = dictation_activity_now_ms();
    if !idle_elapsed_ms(last_ms, now_ms, after) {
        return Ok(());
    }

    let Ok(engine) = crate::text::silero_te::SileroTeEngine::global().lock() else {
        return Ok(());
    };
    if engine.is_loaded() {
        engine.unload();
    }
    Ok(())
}

fn settings_snapshot(ctx: &AppContext) -> Result<AppSettings, &'static str> {
    ctx.controller
        .lock()
        .map_err(|_| "controller lock poisoned")
        .map(|controller| controller.settings().clone())
}

impl AppContext {
    pub fn unload_local_llm_for_idle(&self, app: Option<&AppHandle>) {
        let was_ready = self
            .llm_engine
            .read()
            .map(|engine| engine.is_ready())
            .unwrap_or(false);
        if !was_ready {
            return;
        }

        let previous = self.llm_engine.write().ok().map(|mut guard| {
            std::mem::replace(
                &mut *guard,
                LlmEngine::unloaded(Some(
                    "local LLM unloaded after idle (no dictation)".to_string(),
                )),
            )
        });
        if let Some(previous) = previous {
            if previous.is_ready() {
                previous.unload();
            }
        }

        let unloaded = self
            .llm_engine
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());
        self.runtime.set_llm_engine(unloaded);

        self.record_activity(
            app,
            ActivityLevel::Info,
            "activity.memory.llm_idle_unloaded",
            serde_json::Value::Object(Default::default()),
        );
    }

    pub async fn unload_local_stt_for_idle(&self, app: Option<&AppHandle>) {
        if !self.runtime.is_whisper_model_loaded() {
            return;
        }
        self.unload_silero_te_engine();
        let settings = self
            .controller
            .lock()
            .map(|c| c.settings().clone())
            .unwrap_or_else(|_| crate::settings::AppSettings::default());
        let _ = self.rotate_transcriber_cancel();
        self.reload_transcriber(&settings).await;
        self.record_activity(
            app,
            ActivityLevel::Info,
            "activity.memory.stt_idle_unloaded",
            serde_json::Value::Object(Default::default()),
        );
    }

    /// Drop in-memory local STT / LLM / Silero TE weights (manual action from the status UI).
    pub async fn force_unload_loaded_models(&self, app: Option<&AppHandle>) {
        let llm_loaded = self
            .llm_engine
            .read()
            .map(|engine| engine.is_ready())
            .unwrap_or(false);
        let stt_loaded = self.runtime.is_whisper_model_loaded();
        let te_loaded = crate::text::silero_te::SileroTeEngine::global()
            .lock()
            .map(|engine| engine.is_loaded())
            .unwrap_or(false);

        if !llm_loaded && !stt_loaded && !te_loaded {
            return;
        }

        if llm_loaded {
            self.unload_local_llm_for_idle(app);
        }
        if stt_loaded {
            self.unload_local_stt_for_idle(app).await;
        } else if te_loaded {
            self.unload_silero_te_engine();
        }

        self.record_activity(
            app,
            ActivityLevel::Info,
            "activity.memory.force_unloaded",
            serde_json::Value::Object(Default::default()),
        );
    }

    /// Load local STT in a background thread (PTT press, VAD speech start, manual prewarm).
    pub fn spawn_local_stt_load_in_background(self: &Arc<Self>, settings: &AppSettings) {
        if !needs_local_stt(settings) || !selected_local_stt_ready(settings) {
            return;
        }
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            let _ = tauri::async_runtime::block_on(runtime.prewarm_transcriber());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictation_activity_keys() {
        assert!(is_dictation_activity("activity.ptt.pressed"));
        assert!(is_dictation_activity("activity.inject.done"));
        assert!(!is_dictation_activity("activity.llm.prewarm_done"));
        assert!(!is_dictation_activity("activity.memory.llm_idle_unloaded"));
        assert!(!is_dictation_activity("activity.memory.stt_idle_unloaded"));
    }
}
