use serde::{Deserialize, Serialize};
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
pub const VOICE_FILE_PROGRESS: &str = "app://voice-file-progress";
pub const VOICE_FILES_WINDOW_READY: &str = "app://voice-files-window-ready";
pub const DICTATION_TRANSCRIPT_UPDATED: &str = "app://dictation-transcript-updated";
pub const DICTATION_TRANSCRIPTS_WINDOW_READY: &str = "app://dictation-transcripts-window-ready";
pub const AUDIO_SRT_PROGRESS: &str = "app://audio-srt-progress";
pub const AUDIO_SRT_WINDOW_READY: &str = "app://audio-srt-window-ready";
pub const VOCAL_SEPARATOR_PROGRESS: &str = "app://vocal-separator-progress";
pub const VOCAL_SEPARATOR_WINDOW_READY: &str = "app://vocal-separator-window-ready";
pub const SPEECH_ANALYSIS_PROGRESS: &str = "app://speech-analysis-progress";
pub const SPEECH_ANALYSIS_WINDOW_READY: &str = "app://speech-analysis-window-ready";
pub const SEPARATION_MODEL_DOWNLOAD_PROGRESS: &str = "app://separation-model-download-progress";
pub const DIARIZATION_MODEL_DOWNLOAD_PROGRESS: &str = "app://diarization-model-download-progress";

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceFileProgressPhase {
    Decoding,
    Transcribing,
    TextCleanup,
    AiRewrite,
    Done,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceFileProgressPayload {
    pub path: String,
    pub phase: VoiceFileProgressPhase,
    /// Progress within the current phase (0–100), when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
}

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

pub fn emit_voice_file_progress(app: &AppHandle, payload: VoiceFileProgressPayload) {
    let _ = app.emit(VOICE_FILE_PROGRESS, payload);
}

pub fn emit_voice_files_window_ready(app: &AppHandle) {
    let _ = app.emit(VOICE_FILES_WINDOW_READY, ());
}

pub fn emit_dictation_transcript_updated(
    app: &AppHandle,
    entry: crate::tools::dictation_transcripts::TranscriptEntry,
) {
    let _ = app.emit(DICTATION_TRANSCRIPT_UPDATED, entry);
}

pub fn emit_dictation_transcripts_window_ready(app: &AppHandle) {
    let _ = app.emit(DICTATION_TRANSCRIPTS_WINDOW_READY, ());
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSrtProgressPhase {
    Decoding,
    Transcribing,
    TextCleanup,
    WordAlignment,
    Diarizing,
    GeneratingSubtitles,
    AiRewrite,
    Done,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSrtProgressPayload {
    pub path: String,
    pub phase: AudioSrtProgressPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
}

pub fn emit_audio_srt_progress(app: &AppHandle, payload: AudioSrtProgressPayload) {
    let _ = app.emit(AUDIO_SRT_PROGRESS, payload);
}

pub fn emit_audio_srt_window_ready(app: &AppHandle) {
    let _ = app.emit(AUDIO_SRT_WINDOW_READY, ());
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VocalSeparatorProgressPhase {
    Decoding,
    Separating,
    Writing,
    /// Second independent multi-stem pass on the original mix.
    SplittingInstruments,
    Done,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocalSeparatorProgressPayload {
    pub path: String,
    pub phase: VocalSeparatorProgressPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vocals_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrumental_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instrument_stems: Vec<VocalSeparatorInstrumentStem>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocalSeparatorInstrumentStem {
    pub name: String,
    pub path: String,
}

pub fn emit_vocal_separator_progress(app: &AppHandle, payload: VocalSeparatorProgressPayload) {
    let _ = app.emit(VOCAL_SEPARATOR_PROGRESS, payload);
}

pub fn emit_vocal_separator_window_ready(app: &AppHandle) {
    let _ = app.emit(VOCAL_SEPARATOR_WINDOW_READY, ());
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeparationModelDownloadProgressPayload {
    pub profile: crate::settings::VocalSeparatorProfile,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: Option<f32>,
}

pub fn emit_separation_model_download_progress(
    app: &AppHandle,
    payload: SeparationModelDownloadProgressPayload,
) {
    let _ = app.emit(SEPARATION_MODEL_DOWNLOAD_PROGRESS, payload);
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationModelDownloadProgressPayload {
    pub quality: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: Option<f32>,
}

pub fn emit_diarization_model_download_progress(
    app: &AppHandle,
    payload: DiarizationModelDownloadProgressPayload,
) {
    let _ = app.emit(DIARIZATION_MODEL_DOWNLOAD_PROGRESS, payload);
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechAnalysisProgressPhase {
    Decoding,
    DownloadingModel,
    Transcribing,
    Analyzing,
    Interpreting,
    Done,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechAnalysisProgressPayload {
    pub path: String,
    pub phase: SpeechAnalysisProgressPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
}

pub fn emit_speech_analysis_progress(app: &AppHandle, payload: SpeechAnalysisProgressPayload) {
    let _ = app.emit(SPEECH_ANALYSIS_PROGRESS, payload);
}

pub fn emit_speech_analysis_window_ready(app: &AppHandle) {
    let _ = app.emit(SPEECH_ANALYSIS_WINDOW_READY, ());
}
