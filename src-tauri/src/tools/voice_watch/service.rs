use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::app::activity_log::ActivityLevel;
use crate::app::context::AppContext;
use crate::settings::{save_settings, TextRewriteProvider, SettingsPatch, VoiceWatchSettings};
use crate::tools::voice_watch::watcher::VoiceWatcherHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceWatchStatus {
    pub enabled: bool,
    pub running: bool,
    pub paused_for_cloud: bool,
    pub folder_count: usize,
    pub folders_existing: usize,
}

pub struct VoiceWatchRuntime {
    watcher: VoiceWatcherHandle,
    paused_for_cloud: AtomicBool,
    last_settings: Mutex<VoiceWatchSettings>,
}

impl Default for VoiceWatchRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceWatchRuntime {
    pub fn new() -> Self {
        Self {
            watcher: VoiceWatcherHandle::new(),
            paused_for_cloud: AtomicBool::new(false),
            last_settings: Mutex::new(VoiceWatchSettings::default()),
        }
    }

    pub fn status(&self, settings: &VoiceWatchSettings) -> VoiceWatchStatus {
        let existing = settings
            .folders
            .iter()
            .filter(|f| std::path::Path::new(f.as_str()).is_dir())
            .count();
        VoiceWatchStatus {
            enabled: settings.enabled,
            running: self.watcher.is_running(),
            paused_for_cloud: self.paused_for_cloud.load(Ordering::SeqCst),
            folder_count: settings.folders.len(),
            folders_existing: existing,
        }
    }
}

use std::sync::OnceLock;

pub fn voice_watch_runtime() -> &'static Arc<VoiceWatchRuntime> {
    static RUNTIME: OnceLock<Arc<VoiceWatchRuntime>> = OnceLock::new();
    RUNTIME.get_or_init(|| Arc::new(VoiceWatchRuntime::new()))
}

fn uses_cloud_for_voice(settings: &crate::settings::AppSettings) -> bool {
    let stt_cloud = settings.transcription_provider.eq_ignore_ascii_case("openai")
        || settings.transcription_provider.eq_ignore_ascii_case("cloud");
    let text_cloud = settings.effective_text_processing_mode().uses_ai()
        && matches!(settings.text_rewrite_provider, TextRewriteProvider::Openai);
    stt_cloud || text_cloud
}

pub fn start_voice_watch_runtime(app: &AppHandle, ctx: Arc<AppContext>) {
    let settings = ctx
        .controller
        .lock()
        .ok()
        .map(|c| c.settings().clone())
        .unwrap_or_default();
    apply_settings_to_voice_watch(app, ctx, &settings);
}

pub fn apply_settings_to_voice_watch(
    app: &AppHandle,
    ctx: Arc<AppContext>,
    settings: &crate::settings::AppSettings,
) {
    let runtime = voice_watch_runtime();
    let watch = settings.voice_watch.clone();

    let cloud = uses_cloud_for_voice(settings);
    let pause = watch.enabled && watch.only_local_providers && cloud;
    let was_paused = runtime.paused_for_cloud.swap(pause, Ordering::SeqCst);

    if let Ok(mut last) = runtime.last_settings.lock() {
        *last = watch.clone();
    }

    let _ = crate::tools::voice_watch::history::purge_expired(watch.history_retention_days);

    if pause {
        runtime.watcher.stop();
        if !was_paused {
            ctx.record_activity(
                Some(app),
                ActivityLevel::Warn,
                "activity.voiceWatch.pausedCloud",
                serde_json::json!({}),
            );
            let locale = settings.ui_locale;
            let title =
                crate::i18n::translate(locale, "notify.voiceWatch.paused_cloud_title", &[]);
            let body =
                crate::i18n::translate(locale, "notify.voiceWatch.paused_cloud_body", &[]);
            crate::notify::notify(app, &title, &body);
        }
        return;
    }

    if watch.enabled {
        runtime
            .watcher
            .start_or_update(app.clone(), ctx, watch);
    } else {
        runtime.watcher.stop();
        ctx.record_activity(
            Some(app),
            ActivityLevel::Info,
            "activity.voiceWatch.status",
            serde_json::json!({ "enabled": "off", "folders": "0" }),
        );
    }
}

pub fn voice_watch_set_enabled(
    app: &AppHandle,
    ctx: Arc<AppContext>,
    enabled: bool,
) -> Result<crate::settings::AppSettings, String> {
    let plan = {
        let mut controller = ctx
            .controller
            .lock()
            .map_err(|_| "controller lock poisoned".to_string())?;
        let mut watch = controller.settings().voice_watch.clone();
        watch.enabled = enabled;
        let patch = SettingsPatch {
            voice_watch: Some(watch),
            ..Default::default()
        };
        controller
            .plan_settings_update(patch)
            .map_err(|e| e.to_string())?
    };
    save_settings(&plan.settings).map_err(|e| e.to_string())?;
    apply_settings_to_voice_watch(app, ctx, &plan.settings);
    let _ = app.emit("app://settings-changed", &plan.settings);
    crate::tray::menu::refresh_tray_menu(app);
    Ok(plan.settings)
}

pub fn is_cloud_provider_active(settings: &crate::settings::AppSettings) -> bool {
    uses_cloud_for_voice(settings)
}
