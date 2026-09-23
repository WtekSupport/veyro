use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::app::activity_log::ActivityLogEntry;
use crate::app::state::{AppState, StatusSnapshot};
use crate::error::ErrorPayload;

pub const STATE_CHANGED: &str = "app://state-changed";
pub const LISTENING_STARTED: &str = "app://listening-started";
pub const LISTENING_STOPPED: &str = "app://listening-stopped";
/// Targeted at the recording overlay webview (reliable after lazy create / page load).
pub const OVERLAY_LISTENING: &str = "app://overlay-listening";
pub const TRANSCRIPTION_STARTED: &str = "app://transcription-started";
pub const TRANSCRIPTION_COMPLETED: &str = "app://transcription-completed";
pub const INJECTION_COMPLETED: &str = "app://injection-completed";
pub const ERROR: &str = "app://error";
pub const MICROPHONE_CHANGED: &str = "app://microphone-changed";
pub const ACTIVITY_LOG: &str = "app://activity-log";
pub const WHISPER_MODEL_DOWNLOAD_PROGRESS: &str = "app://whisper-model-download-progress";
pub const LLM_MODEL_DOWNLOAD_PROGRESS: &str = "app://llm-model-download-progress";
pub const SILERO_TE_DOWNLOAD_PROGRESS: &str = "app://silero-te-download-progress";
pub const SILERO_VAD_DOWNLOAD_PROGRESS: &str = "app://silero-vad-download-progress";
pub const SKILL_IMPORTED: &str = "app://skill-imported";
pub const SKILL_IMPORT_FLOW: &str = "app://skill-import-flow";
pub const SKILLS_CHANGED: &str = "app://skills-changed";

#[derive(Clone, Serialize)]
pub struct SkillImportFlowPayload {
    pub phase: SkillImportPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<crate::text::skill::AiSkillInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillImportPhase {
    Preparing,
    Preview,
    AlreadyInstalled,
    Installing,
    Done,
    Error,
}

pub fn emit_skill_import_flow(app: &AppHandle, payload: SkillImportFlowPayload) {
    let _ = app.emit(SKILL_IMPORT_FLOW, payload);
}

#[derive(Clone, Serialize)]
pub struct SkillImportedPayload {
    pub skill: crate::text::skill::AiSkillInfo,
}

pub fn emit_skill_imported(app: &AppHandle, payload: SkillImportedPayload) {
    let _ = app.emit(SKILL_IMPORTED, payload);
}

pub fn emit_skills_changed(app: &AppHandle) {
    let _ = app.emit(SKILLS_CHANGED, ());
}

#[derive(Clone, Serialize)]
pub struct StateChangedPayload {
    pub state: AppState,
    pub status: StatusSnapshot,
}

pub fn emit_state_changed(app: &AppHandle, status: &StatusSnapshot) {
    let payload = StateChangedPayload {
        state: status.state,
        status: status.clone(),
    };
    let _ = app.emit(STATE_CHANGED, payload);
}

pub fn emit_listening_started(app: &AppHandle) {
    let _ = app.emit(LISTENING_STARTED, ());
}

pub fn emit_listening_stopped(app: &AppHandle) {
    let _ = app.emit(LISTENING_STOPPED, ());
}

pub fn emit_transcription_started(app: &AppHandle) {
    let _ = app.emit(TRANSCRIPTION_STARTED, ());
}

#[derive(Clone, Serialize)]
pub struct TranscriptionCompletedPayload {
    pub char_count: usize,
}

pub fn emit_transcription_completed(app: &AppHandle, payload: TranscriptionCompletedPayload) {
    let _ = app.emit(TRANSCRIPTION_COMPLETED, payload);
}

pub fn emit_injection_completed(app: &AppHandle) {
    let _ = app.emit(INJECTION_COMPLETED, ());
}

pub fn emit_error(app: &AppHandle, payload: ErrorPayload) {
    let _ = app.emit(ERROR, payload);
}

pub fn emit_microphone_changed(app: &AppHandle, device_id: Option<String>) {
    let _ = app.emit(MICROPHONE_CHANGED, device_id);
}

pub fn emit_activity_log(app: &AppHandle, entries: &[ActivityLogEntry]) {
    let _ = app.emit(ACTIVITY_LOG, entries.to_vec());
}
