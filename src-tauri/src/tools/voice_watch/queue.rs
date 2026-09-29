use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::app::activity_log::ActivityLevel;
use crate::app::context::AppContext;
use crate::notify;
use crate::settings::{
    TextProcessingMode, VoiceWatchNotifyMode, VoiceWatchSettings,
};
use crate::tools::shared::STT_SELECT_LANGUAGE_ERROR;
use crate::tools::voice_file::{transcribe_voice_file, VoiceFileOptions};
use crate::tools::voice_watch::history::{
    self, HistoryEntry, HistoryStatus, new_history_id, touch_now_ms, upsert_history_entry,
};
use crate::tools::voice_watch::presets::guess_messenger;
use crate::tools::voice_watch::registry::DedupRegistry;

pub const VOICE_QUEUE_CHANGED: &str = "app://voice-queue-changed";
pub const VOICE_HISTORY_CHANGED: &str = "app://voice-history-changed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceJobSource {
    Manual,
    #[serde(rename = "auto-watch", alias = "auto_watch")]
    AutoWatch,
}

impl VoiceJobSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AutoWatch => "auto-watch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceJobStatus {
    Pending,
    Processing,
    Done,
    Error,
    SpeechUnrecognized,
    TooLong,
    NotAudio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceJobMeta {
    pub source: VoiceJobSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messenger: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appeared_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_language_override: Option<String>,
}

impl Default for VoiceJobMeta {
    fn default() -> Self {
        Self {
            source: VoiceJobSource::Manual,
            messenger: None,
            appeared_at_ms: None,
            content_sha256: None,
            stt_language_override: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceJob {
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub status: VoiceJobStatus,
    pub meta: VoiceJobMeta,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceQueueSnapshot {
    pub jobs: Vec<VoiceJob>,
}

struct QueueState {
    pending: VecDeque<VoiceJob>,
    active: Option<VoiceJob>,
    recent: Vec<VoiceJob>,
}

pub struct VoiceQueue {
    state: Mutex<QueueState>,
    worker_scheduled: AtomicBool,
}

impl Default for VoiceQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceQueue {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QueueState {
                pending: VecDeque::new(),
                active: None,
                recent: Vec::new(),
            }),
            worker_scheduled: AtomicBool::new(false),
        }
    }

    fn snapshot_locked(state: &QueueState) -> VoiceQueueSnapshot {
        let mut jobs = Vec::new();
        if let Some(active) = &state.active {
            jobs.push(active.clone());
        }
        jobs.extend(state.pending.iter().cloned());
        jobs.extend(state.recent.iter().rev().cloned());
        VoiceQueueSnapshot { jobs }
    }

    pub fn snapshot(&self) -> VoiceQueueSnapshot {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Self::snapshot_locked(&state)
    }
}

use std::sync::OnceLock;

fn global_queue() -> &'static Arc<VoiceQueue> {
    static QUEUE: OnceLock<Arc<VoiceQueue>> = OnceLock::new();
    QUEUE.get_or_init(|| Arc::new(VoiceQueue::new()))
}

pub fn get_queue_snapshot() -> VoiceQueueSnapshot {
    global_queue().snapshot()
}

/// Drop a job from the in-memory queue (pending / recent). Active processing is left alone.
pub fn remove_job_from_queue(id: &str) -> bool {
    let mut state = match global_queue().state.lock() {
        Ok(s) => s,
        Err(e) => e.into_inner(),
    };
    let before = state.pending.len() + state.recent.len();
    state.pending.retain(|j| j.id != id);
    state.recent.retain(|j| j.id != id);
    if state.active.as_ref().is_some_and(|j| j.id == id) {
        // Don't abort in-flight STT; just ignore — caller should avoid this.
    }
    before != state.pending.len() + state.recent.len()
}

/// Remove from history index and queue UI (does not delete the audio file on disk).
pub fn remove_from_index(app: &AppHandle, id: &str) -> Result<Vec<HistoryEntry>, String> {
    remove_job_from_queue(id);
    let entries = history::delete_history_entry(id)?;
    emit_queue(app);
    emit_history(app);
    Ok(entries)
}

fn emit_queue(app: &AppHandle) {
    let snap = get_queue_snapshot();
    let _ = app.emit(VOICE_QUEUE_CHANGED, &snap);
}

fn emit_history(app: &AppHandle) {
    let entries = history::list_history();
    let _ = app.emit(VOICE_HISTORY_CHANGED, &entries);
}

pub fn enqueue_paths(
    app: &AppHandle,
    ctx: Arc<AppContext>,
    paths: Vec<String>,
    meta: VoiceJobMeta,
) -> Result<Vec<VoiceJob>, String> {
    let mut created = Vec::new();
    let queue = global_queue();
    {
        let mut state = queue.state.lock().map_err(|_| "voice queue lock poisoned")?;
        for path in paths {
            let path = path.trim().to_string();
            if path.is_empty() {
                continue;
            }
            // Debounce: skip if already pending/active with same path
            let dup = state
                .pending
                .iter()
                .chain(state.active.iter())
                .any(|j| j.path == path);
            if dup {
                continue;
            }
            let file_name = Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&path)
                .to_string();
            let mut job_meta = meta.clone();
            if job_meta.messenger.is_none() {
                job_meta.messenger = guess_messenger(Path::new(&path));
            }
            if job_meta.appeared_at_ms.is_none() {
                job_meta.appeared_at_ms = Some(touch_now_ms());
            }
            let id = new_history_id();
            let job = VoiceJob {
                id: id.clone(),
                path: path.clone(),
                file_name: file_name.clone(),
                status: VoiceJobStatus::Pending,
                meta: job_meta.clone(),
                text: String::new(),
                error_key: None,
                duration_secs: None,
            };
            let entry = HistoryEntry {
                id: id.clone(),
                path: path.clone(),
                file_name,
                source: job_meta.source.as_str().to_string(),
                messenger: job_meta.messenger.clone(),
                appeared_at_ms: job_meta.appeared_at_ms.unwrap_or_else(touch_now_ms),
                updated_at_ms: touch_now_ms(),
                duration_secs: None,
                status: HistoryStatus::Pending,
                text: String::new(),
                error_key: None,
                content_sha256: job_meta.content_sha256.clone(),
            };
            let _ = upsert_history_entry(entry);
            state.pending.push_back(job.clone());
            created.push(job);
        }
    }

    if !created.is_empty() {
        ctx.record_activity(
            Some(app),
            ActivityLevel::Info,
            "activity.voiceWatch.enqueued",
            serde_json::json!({ "count": created.len() }),
        );
        emit_queue(app);
        emit_history(app);
        maybe_burst_notify(app, &ctx, created.len());
        schedule_worker(app.clone(), ctx);
    }
    Ok(created)
}

fn maybe_burst_notify(app: &AppHandle, ctx: &AppContext, just_added: usize) {
    if just_added < 5 {
        return;
    }
    let locale = ctx
        .controller
        .lock()
        .ok()
        .map(|c| c.settings().ui_locale)
        .unwrap_or_default();
    let title = crate::i18n::translate(locale, "notify.voiceWatch.burst_title", &[]);
    let body = crate::i18n::translate(
        locale,
        "notify.voiceWatch.burst_body",
        &[("count", &just_added.to_string())],
    );
    notify::notify(app, &title, &body);
}

fn schedule_worker(app: AppHandle, ctx: Arc<AppContext>) {
    let queue = global_queue();
    if queue
        .worker_scheduled
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    tauri::async_runtime::spawn(async move {
        run_worker(app.clone(), ctx).await;
        global_queue()
            .worker_scheduled
            .store(false, Ordering::SeqCst);
        // If work arrived while finishing, reschedule
        let has_pending = global_queue()
            .state
            .lock()
            .map(|s| !s.pending.is_empty())
            .unwrap_or(false);
        if has_pending {
            if let Some(ctx) = app.try_state::<Arc<AppContext>>() {
                schedule_worker(app.clone(), ctx.inner().clone());
            }
        }
    });
}

async fn wait_for_tools_slot(ctx: &AppContext) {
    loop {
        let blocked = ctx
            .controller
            .lock()
            .ok()
            .is_some_and(|c| c.blocks_tools_transcription() || c.is_tools_transcription_busy());
        if !blocked {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn run_worker(app: AppHandle, ctx: Arc<AppContext>) {
    loop {
        let next = {
            let mut state = match global_queue().state.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            let Some(mut job) = state.pending.pop_front() else {
                return;
            };
            job.status = VoiceJobStatus::Processing;
            state.active = Some(job.clone());
            job
        };
        emit_queue(&app);
        update_history_status(
            &next,
            HistoryStatus::Processing,
            None,
            String::new(),
            None,
        );
        emit_history(&app);

        wait_for_tools_slot(&ctx).await;

        let watch = voice_watch_settings(&ctx);
        let options = build_options(&next, &watch);

        let result = transcribe_voice_file(app.clone(), ctx.clone(), next.path.clone(), options).await;

        let finished = match result {
            Ok(res) => {
                let text = res.text;
                let info = res.info_message_key.clone();
                let status = if text.trim().is_empty()
                    && (info.as_deref() == Some(STT_SELECT_LANGUAGE_ERROR)
                        || info.as_deref().is_some_and(|k| {
                            k.contains("silence") || k.contains("music") || k.contains("empty")
                        }))
                {
                    VoiceJobStatus::SpeechUnrecognized
                } else if text.trim().is_empty() && info.is_some() {
                    VoiceJobStatus::SpeechUnrecognized
                } else {
                    VoiceJobStatus::Done
                };
                let mut job = next.clone();
                job.status = status;
                job.text = text.clone();
                job.error_key = info;
                job.file_name = res.file_name;
                Some(finish_job(&app, &ctx, &watch, job, None).await)
            }
            Err(err) => {
                let mut job = next.clone();
                if err == STT_SELECT_LANGUAGE_ERROR {
                    job.status = VoiceJobStatus::SpeechUnrecognized;
                    job.error_key = Some(STT_SELECT_LANGUAGE_ERROR.to_string());
                    Some(finish_job(&app, &ctx, &watch, job, None).await)
                } else if err == "tools.voiceFiles.pttBusy" || err == "tools.voiceFiles.busy" {
                    {
                        let mut state =
                            global_queue().state.lock().unwrap_or_else(|e| e.into_inner());
                        job.status = VoiceJobStatus::Pending;
                        state.active = None;
                        state.pending.push_front(job);
                    }
                    emit_queue(&app);
                    None
                } else {
                    job.status = VoiceJobStatus::Error;
                    job.error_key = Some(err);
                    Some(finish_job(&app, &ctx, &watch, job, None).await)
                }
            }
        };

        let Some(finished) = finished else {
            wait_for_tools_slot(&ctx).await;
            continue;
        };

        {
            let mut state = global_queue().state.lock().unwrap_or_else(|e| e.into_inner());
            state.active = None;
            state.recent.push(finished);
            if state.recent.len() > 40 {
                let drain = state.recent.len() - 40;
                state.recent.drain(0..drain);
            }
        }
        emit_queue(&app);
        emit_history(&app);
    }
}

fn voice_watch_settings(ctx: &AppContext) -> VoiceWatchSettings {
    ctx.controller
        .lock()
        .ok()
        .map(|c| c.settings().voice_watch.clone())
        .unwrap_or_default()
}

fn build_options(job: &VoiceJob, watch: &VoiceWatchSettings) -> VoiceFileOptions {
    let mut options = VoiceFileOptions {
        stt_language_override: job.meta.stt_language_override.clone(),
        text_mode_override: None,
        skip_language_prompt: matches!(job.meta.source, VoiceJobSource::AutoWatch),
    };
    let mode = watch.text_mode();
    if !mode.is_inherit() {
        options.text_mode_override = Some(mode.as_config_str());
    }
    options
}

async fn finish_job(
    app: &AppHandle,
    ctx: &AppContext,
    watch: &VoiceWatchSettings,
    job: VoiceJob,
    duration_secs: Option<f64>,
) -> VoiceJob {
    let hist_status = match job.status {
        VoiceJobStatus::Done => HistoryStatus::Done,
        VoiceJobStatus::SpeechUnrecognized => HistoryStatus::SpeechUnrecognized,
        VoiceJobStatus::TooLong => HistoryStatus::TooLong,
        VoiceJobStatus::NotAudio => HistoryStatus::NotAudio,
        VoiceJobStatus::Error => HistoryStatus::Error,
        _ => HistoryStatus::Error,
    };
    update_history_status(
        &job,
        hist_status,
        duration_secs.or(job.duration_secs),
        job.text.clone(),
        job.error_key.clone(),
    );

    if matches!(job.status, VoiceJobStatus::Done) {
        if let Some(sha) = &job.meta.content_sha256 {
            let mut reg = DedupRegistry::load();
            let meta = std::fs::metadata(&job.path).ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            reg.remember(&job.path, size, mtime, sha.clone());
        }
        maybe_notify_success(app, ctx, watch, &job);
        maybe_handle_source_file(watch, &job);
        ctx.record_activity(
            Some(app),
            ActivityLevel::Info,
            "activity.voiceWatch.done",
            serde_json::json!({
                "file": job.file_name,
            }),
        );
        crate::tray::menu::refresh_tray_menu(app);
    } else if matches!(job.status, VoiceJobStatus::Error) {
        ctx.record_activity(
            Some(app),
            ActivityLevel::Warn,
            "activity.voiceWatch.error",
            serde_json::json!({
                "file": job.file_name,
            }),
        );
    }

    job
}

fn update_history_status(
    job: &VoiceJob,
    status: HistoryStatus,
    duration_secs: Option<f64>,
    text: String,
    error_key: Option<String>,
) {
    let entry = HistoryEntry {
        id: job.id.clone(),
        path: job.path.clone(),
        file_name: job.file_name.clone(),
        source: job.meta.source.as_str().to_string(),
        messenger: job.meta.messenger.clone(),
        appeared_at_ms: job.meta.appeared_at_ms.unwrap_or_else(touch_now_ms),
        updated_at_ms: touch_now_ms(),
        duration_secs,
        status,
        text,
        error_key,
        content_sha256: job.meta.content_sha256.clone(),
    };
    let _ = upsert_history_entry(entry);
}

fn maybe_notify_success(
    app: &AppHandle,
    ctx: &AppContext,
    watch: &VoiceWatchSettings,
    job: &VoiceJob,
) {
    if matches!(watch.notify, VoiceWatchNotifyMode::Off) {
        return;
    }
    if matches!(watch.notify, VoiceWatchNotifyMode::ResultAndCopy) {
        let _ = crate::injection::clipboard::ClipboardGuard::set_text(&job.text);
    }
    let locale = ctx
        .controller
        .lock()
        .ok()
        .map(|c| c.settings().ui_locale)
        .unwrap_or_default();
    let title = crate::i18n::translate(locale, "notify.voiceWatch.done_title", &[]);
    let preview: String = job.text.chars().take(120).collect();
    let body = if preview.is_empty() {
        crate::i18n::translate(locale, "notify.voiceWatch.done_empty", &[])
    } else {
        preview
    };
    // Voice-watch results notify even when global show_notifications is off if mode is result*
    // but still require OS support. Use show_system_notification directly when watch mode wants it.
    if ctx
        .controller
        .lock()
        .ok()
        .is_some_and(|c| c.settings().show_notifications)
    {
        notify::notify(app, &title, &body);
    } else {
        let _ = notify::show_system_notification(app, &title, &body);
    }
}

fn maybe_handle_source_file(watch: &VoiceWatchSettings, job: &VoiceJob) {
    let path = PathBuf::from(&job.path);
    if !path.is_file() {
        return;
    }
    if watch.move_source_after {
        if let Some(parent) = path.parent() {
            let dest_dir = parent.join("Veyro").join("processed");
            if let Err(err) = std::fs::create_dir_all(&dest_dir) {
                warn!("voice watch move mkdir failed: {err}");
            } else {
                let dest = dest_dir.join(path.file_name().unwrap_or_default());
                if let Err(err) = std::fs::rename(&path, &dest) {
                    warn!("voice watch move failed: {err}");
                } else {
                    info!("voice watch moved source to {}", dest.display());
                    return;
                }
            }
        }
    }
    if watch.delete_source_after {
        if let Err(err) = std::fs::remove_file(&path) {
            warn!("voice watch delete failed: {err}");
        }
    }
}

pub fn retry_history_entry(
    app: &AppHandle,
    ctx: Arc<AppContext>,
    id: &str,
    language_override: Option<String>,
) -> Result<VoiceJob, String> {
    let entries = history::list_history();
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| "tools.voiceWatch.historyMissing".to_string())?;
    let meta = VoiceJobMeta {
        source: if entry.source == "auto-watch" {
            VoiceJobSource::AutoWatch
        } else {
            VoiceJobSource::Manual
        },
        messenger: entry.messenger,
        appeared_at_ms: Some(entry.appeared_at_ms),
        content_sha256: entry.content_sha256,
        stt_language_override: language_override,
    };
    let jobs = enqueue_paths(app, ctx, vec![entry.path], meta)?;
    jobs.into_iter()
        .next()
        .ok_or_else(|| "tools.voiceWatch.enqueueFailed".to_string())
}

/// Apply text mode override string onto a settings clone.
pub fn apply_text_mode_override(
    settings: &mut crate::settings::AppSettings,
    override_str: &str,
) {
    let mode = crate::settings::VoiceWatchTextMode::from_config_str(override_str);
    match mode {
        crate::settings::VoiceWatchTextMode::Named(
            crate::settings::VoiceWatchTextModeOverride::Inherit,
        ) => {}
        crate::settings::VoiceWatchTextMode::Named(
            crate::settings::VoiceWatchTextModeOverride::Original,
        ) => {
            settings.text_processing_mode = TextProcessingMode::Original;
        }
        crate::settings::VoiceWatchTextMode::Named(
            crate::settings::VoiceWatchTextModeOverride::Basic,
        ) => {
            settings.text_processing_mode = TextProcessingMode::Basic;
        }
        crate::settings::VoiceWatchTextMode::Named(
            crate::settings::VoiceWatchTextModeOverride::Skill,
        )
        | crate::settings::VoiceWatchTextMode::SkillName(_) => {
            settings.text_processing_mode = TextProcessingMode::CustomSkill;
            if let crate::settings::VoiceWatchTextMode::SkillName(name) = mode {
                let skill = name.strip_prefix("skill:").unwrap_or(&name);
                settings.ai_rewrite_skill = Some(skill.to_string());
            }
        }
    }
}
