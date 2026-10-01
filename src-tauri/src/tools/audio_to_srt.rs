use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tracing::info;

use crate::app::context::AppContext;
use crate::app::events::{
    emit_audio_srt_progress, AudioSrtProgressPayload, AudioSrtProgressPhase,
};
use crate::app::transcribe_audio::{transcribe_segment_with_retries, TranscribeSegmentFlags};
use crate::error::AppError;
use crate::llm::LlmEngine;
use crate::settings::{AppSettings, LocalSttEngine, TextProcessingMode};
use crate::subtitles::{
    apply_speaker_prefixes, assign_cue_speakers, build_subtitle_cues, render_srt, render_vtt,
    SubtitleLineEnding, SubtitleOptions,
};
use crate::diarization::{self, DiarizeOptions, SpeakerCountMode};
use crate::text::dictionary::{load_dictionary_for_settings, protected_terms, Dictionary};
use crate::text::pipeline::{process_transcription_immediate_sync, rewrite_processed_text};
use crate::timed_text::TimedTextSegment;
use crate::transcription::models::WhisperProgressCallback;
use crate::transcription::prompt::WhisperPromptInput;
use crate::transcription::TranscriptionProvider;
use crate::vad::detect_speech_regions;

use super::audio_to_srt_timing::{
    expand_stt_chunks, merge_region_timed_segments, sort_timed_segments_by_start,
};
use super::shared::{
    decode_and_preprocess_for_tools, throttled_percent_callback, ToolsTranscriptionGuard,
    validate_tool_file_path,
};

/// Canonical pipeline (do not reorder without updating `todo/TZ_audio_to_srt.md` §12
/// and `todo/ТЗ_ диаризация в SRT-инструменте Veyro.md`):
/// decode → VAD regions → STT per region → text cleanup → [optional diarization] →
/// `build_subtitle_cues` (+ speaker assign/prefix).
/// Do **not** route the main path through word-align / energy gate — that regresses pause sync.

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SubtitleSttCapability {
    Supported,
    UnsupportedProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioToSrtOptions {
    #[serde(default = "default_max_line_length")]
    pub max_line_length: usize,
    #[serde(default = "default_max_lines")]
    pub max_lines_per_cue: u8,
    #[serde(default = "default_max_cue_duration_ms")]
    pub max_cue_duration_ms: u64,
    #[serde(default = "default_min_cue_duration_ms")]
    pub min_cue_duration_ms: u64,
    #[serde(default = "default_pause_split_ms")]
    pub pause_split_ms: u64,
    #[serde(default)]
    pub global_offset_ms: i64,
    #[serde(default)]
    pub utf8_bom: bool,
    #[serde(default = "default_use_dictation_text_settings")]
    pub use_dictation_text_settings: bool,
    #[serde(default = "default_smart_split")]
    pub smart_split: bool,
    /// Legacy field (ignored; timing follows VAD + STT per `todo/TZ_audio_to_srt.md`).
    #[serde(default = "default_speech_gate_percent")]
    pub speech_gate_percent: u8,
    #[serde(default = "default_reading_tail_ms")]
    pub reading_tail_ms: u64,
    /// Per-request STT language (does not persist settings); used after user picks a language in tools UI.
    #[serde(default)]
    pub stt_language_override: Option<String>,
    /// Optional speaker diarization after ASR (`todo/ТЗ_ диаризация…`).
    #[serde(default)]
    pub speaker_diarization: bool,
    #[serde(default)]
    pub speaker_count_mode: SpeakerCountModeDto,
    #[serde(default = "default_speaker_exact_count")]
    pub speaker_exact_count: u8,
    #[serde(default = "default_speaker_min_count")]
    pub speaker_min_count: u8,
    #[serde(default = "default_speaker_max_count")]
    pub speaker_max_count: u8,
    /// Include `Speaker N:` prefixes in exported SRT text.
    #[serde(default = "default_include_speaker_names")]
    pub include_speaker_names: bool,
    /// Minimum speech blob length for diarization (seconds); expert.
    #[serde(default = "default_diarization_min_speech_secs")]
    pub diarization_min_speech_secs: f32,
    /// Clustering sensitivity (AHC threshold when not default); expert.
    #[serde(default = "default_diarization_sensitivity")]
    pub diarization_sensitivity: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SpeakerCountModeDto {
    #[default]
    Auto,
    Exact,
    Range,
}

fn default_speaker_exact_count() -> u8 {
    2
}
fn default_speaker_min_count() -> u8 {
    1
}
fn default_speaker_max_count() -> u8 {
    8
}
fn default_include_speaker_names() -> bool {
    true
}
fn default_diarization_min_speech_secs() -> f32 {
    0.25
}
fn default_diarization_sensitivity() -> f32 {
    0.45
}

fn default_use_dictation_text_settings() -> bool {
    false
}

fn default_reading_tail_ms() -> u64 {
    200
}

fn default_smart_split() -> bool {
    true
}

fn default_speech_gate_percent() -> u8 {
    25
}

fn default_max_line_length() -> usize {
    42
}
fn default_max_lines() -> u8 {
    2
}
fn default_max_cue_duration_ms() -> u64 {
    7_000
}
fn default_min_cue_duration_ms() -> u64 {
    1_000
}
fn default_pause_split_ms() -> u64 {
    400
}

impl Default for AudioToSrtOptions {
    fn default() -> Self {
        Self {
            max_line_length: default_max_line_length(),
            max_lines_per_cue: default_max_lines(),
            max_cue_duration_ms: default_max_cue_duration_ms(),
            min_cue_duration_ms: default_min_cue_duration_ms(),
            pause_split_ms: default_pause_split_ms(),
            global_offset_ms: 0,
            utf8_bom: false,
            use_dictation_text_settings: false,
            smart_split: true,
            speech_gate_percent: default_speech_gate_percent(),
            reading_tail_ms: default_reading_tail_ms(),
            stt_language_override: None,
            speaker_diarization: false,
            speaker_count_mode: SpeakerCountModeDto::Auto,
            speaker_exact_count: default_speaker_exact_count(),
            speaker_min_count: default_speaker_min_count(),
            speaker_max_count: default_speaker_max_count(),
            include_speaker_names: default_include_speaker_names(),
            diarization_min_speech_secs: default_diarization_min_speech_secs(),
            diarization_sensitivity: default_diarization_sensitivity(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioToSrtSpeakerInfo {
    pub id: u32,
    pub label: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioToSrtResult {
    pub file_name: String,
    pub srt: String,
    /// WebVTT export; uses `<v Name>` voice tags when speakers are assigned.
    pub vtt: String,
    pub rewrite_fallback: bool,
    pub rewrite_fallback_reason: Option<String>,
    pub ai_rewrite_applied: bool,
    #[serde(default)]
    pub speakers: Vec<AudioToSrtSpeakerInfo>,
    /// Speaker ids aligned with cues in export order (same count as non-empty rendered cues).
    #[serde(default)]
    pub cue_speaker_ids: Vec<Option<u32>>,
}

pub fn subtitle_stt_capability(settings: &AppSettings) -> SubtitleSttCapability {
    if settings.transcription_provider == "local" {
        match settings.local_stt_variant().engine() {
            LocalSttEngine::Whisper => SubtitleSttCapability::Supported,
            LocalSttEngine::Sherpa | LocalSttEngine::Sidecar => {
                SubtitleSttCapability::UnsupportedProvider
            }
        }
    } else {
        SubtitleSttCapability::Supported
    }
}

fn subtitle_settings(base: &AppSettings, options: &AudioToSrtOptions) -> AppSettings {
    let mut settings = base.clone();
    if !options.use_dictation_text_settings {
        settings.numbers_as_words = false;
    }
    if let Some(language) = options
        .stt_language_override
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.language = Some(language.to_string());
    }
    settings
}

fn default_speaker_label(locale: crate::settings::UiLocale, speaker_id: u32) -> String {
    let n = speaker_id + 1;
    match locale {
        crate::settings::UiLocale::Ru => format!("Спикер {n}"),
        crate::settings::UiLocale::En => format!("Speaker {n}"),
    }
}

fn emit_phase(app: &AppHandle, path: &str, phase: AudioSrtProgressPhase) {
    emit_progress(app, path, phase, None);
}

fn emit_progress(
    app: &AppHandle,
    path: &str,
    phase: AudioSrtProgressPhase,
    percent: Option<u8>,
) {
    emit_audio_srt_progress(
        app,
        AudioSrtProgressPayload {
            path: path.to_string(),
            phase,
            percent,
        },
    );
}

fn throttled_phase_progress(
    app: AppHandle,
    path: String,
    phase: AudioSrtProgressPhase,
) -> WhisperProgressCallback {
    throttled_percent_callback(Arc::new(move |percent: u8| {
        emit_progress(&app, &path, phase, Some(percent));
    }))
}

pub async fn transcribe_audio_to_srt(
    app: AppHandle,
    ctx: Arc<AppContext>,
    path: String,
    options: AudioToSrtOptions,
) -> Result<AudioToSrtResult, String> {
    let _guard = ToolsTranscriptionGuard::try_begin(&ctx)?;

    let (path_key, path_buf, file_name) = validate_tool_file_path(&path, "tools.audioSrt")?;

    let base_settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();

    if subtitle_stt_capability(&base_settings) == SubtitleSttCapability::UnsupportedProvider {
        return Err("tools.audioSrt.unsupportedProvider".to_string());
    }

    let settings = subtitle_settings(&base_settings, &options);
    let ai_mode = settings.effective_text_processing_mode();

    emit_phase(&app, &path_key, AudioSrtProgressPhase::Decoding);
    let decode_progress = throttled_phase_progress(
        app.clone(),
        path_key.clone(),
        AudioSrtProgressPhase::Decoding,
    );
    let prepared = decode_and_preprocess_for_tools(
        &path_buf,
        &settings,
        Some(decode_progress),
        "tools.audioSrt",
    )?;

    if prepared.skipped_as_silence {
        emit_phase(&app, &path_key, AudioSrtProgressPhase::Done);
        return Ok(AudioToSrtResult {
            file_name,
            srt: String::new(),
            vtt: String::new(),
            rewrite_fallback: false,
            rewrite_fallback_reason: None,
            ai_rewrite_applied: false,
            speakers: Vec::new(),
            cue_speaker_ids: Vec::new(),
        });
    }

    let dictionary = load_dictionary_for_settings(&settings).unwrap_or_default();

    emit_phase(&app, &path_key, AudioSrtProgressPhase::Transcribing);
    let vad_config = settings.vad_config();
    let speech_regions = detect_speech_regions(&prepared.segment, &vad_config)
        .map_err(|error| error.to_string())?;

    let transcriber = ctx.runtime.transcriber();
    let stt_auto_mode = super::shared::stt_failed_in_auto_mode(
        &base_settings,
        options.stt_language_override.as_deref(),
    );
    let timed = transcribe_speech_regions(
        app.clone(),
        path_key.clone(),
        transcriber,
        &prepared,
        &speech_regions,
        &settings,
        &dictionary.vocabulary,
        stt_auto_mode,
    )
    .await?;

    emit_phase(&app, &path_key, AudioSrtProgressPhase::TextCleanup);
    let processed_segments = process_timed_segments_for_subtitles(
        &app,
        &ctx,
        &settings,
        &path_key,
        &timed.segments,
        timed.detected_language.as_deref(),
        ai_mode,
        &dictionary,
    )
    .await?;

    let speaker_intervals = if options.speaker_diarization {
        if speech_regions.is_empty() {
            Vec::new()
        } else {
            emit_phase(&app, &path_key, AudioSrtProgressPhase::Diarizing);
            let diarize_opts = DiarizeOptions {
                count_mode: match options.speaker_count_mode {
                    SpeakerCountModeDto::Auto => SpeakerCountMode::Auto,
                    SpeakerCountModeDto::Exact => SpeakerCountMode::Exact,
                    SpeakerCountModeDto::Range => SpeakerCountMode::Range,
                },
                exact_count: options.speaker_exact_count,
                min_count: options.speaker_min_count,
                max_count: options.speaker_max_count,
                min_speech_secs: options.diarization_min_speech_secs,
                sensitivity: options.diarization_sensitivity,
            };
            // Run blocking polyvoice off the async runtime.
            let settings_for_diar = settings.clone();
            let segment_for_diar = prepared.segment.clone();
            tauri::async_runtime::spawn_blocking(move || {
                diarization::diarize_segment(&settings_for_diar, &segment_for_diar, &diarize_opts)
            })
            .await
            .map_err(|error| format!("tools.audioSrt.diarizationFailed|{error}"))??
        }
    } else {
        Vec::new()
    };

    emit_phase(&app, &path_key, AudioSrtProgressPhase::GeneratingSubtitles);
    let subtitle_opts = SubtitleOptions {
        max_line_length: options.max_line_length.clamp(20, 60),
        max_lines_per_cue: options.max_lines_per_cue.clamp(1, 2),
        max_cue_duration_ms: options.max_cue_duration_ms.clamp(1_000, 10_000),
        min_cue_duration_ms: options.min_cue_duration_ms.clamp(500, 5_000),
        pause_split_ms: options.pause_split_ms.clamp(100, 2_000),
        reading_tail_ms: options.reading_tail_ms.clamp(0, 1_000),
        smart_split: options.smart_split,
    };
    let mut cues = build_subtitle_cues(&processed_segments.segments, &subtitle_opts);
    if !speaker_intervals.is_empty() {
        assign_cue_speakers(&mut cues, &speaker_intervals);
    }

    let mut speaker_ids: Vec<u32> = cues.iter().filter_map(|cue| cue.speaker_id).collect();
    speaker_ids.sort_unstable();
    speaker_ids.dedup();
    let speakers: Vec<AudioToSrtSpeakerInfo> = speaker_ids
        .iter()
        .map(|id| AudioToSrtSpeakerInfo {
            id: *id,
            label: default_speaker_label(settings.ui_locale, *id),
        })
        .collect();

    let label_map: std::collections::HashMap<u32, String> = speakers
        .iter()
        .map(|speaker| (speaker.id, speaker.label.clone()))
        .collect();

    // SRT: optional text prefixes. VTT: `<v Name>` from speaker_id (clean lines).
    let mut srt_cues = cues.clone();
    if options.speaker_diarization && options.include_speaker_names && !speakers.is_empty() {
        apply_speaker_prefixes(&mut srt_cues, |id| {
            label_map
                .get(&id)
                .cloned()
                .unwrap_or_else(|| default_speaker_label(settings.ui_locale, id))
        });
    }

    let cue_speaker_ids: Vec<Option<u32>> = cues.iter().map(|cue| cue.speaker_id).collect();
    let srt = render_srt(
        &srt_cues,
        SubtitleLineEnding::CrLf,
        options.global_offset_ms,
        options.utf8_bom,
    );
    let vtt = render_vtt(
        &cues,
        |id| label_map.get(&id).cloned(),
        SubtitleLineEnding::CrLf,
        options.global_offset_ms,
        options.utf8_bom,
    );

    emit_phase(&app, &path_key, AudioSrtProgressPhase::Done);

    Ok(AudioToSrtResult {
        file_name,
        srt,
        vtt,
        rewrite_fallback: processed_segments.rewrite_fallback,
        rewrite_fallback_reason: processed_segments.rewrite_fallback_reason,
        ai_rewrite_applied: processed_segments.ai_rewrite_applied,
        speakers,
        cue_speaker_ids,
    })
}

struct RegionTranscriptionOutcome {
    segments: Vec<TimedTextSegment>,
    detected_language: Option<String>,
}

async fn transcribe_speech_regions(
    app: AppHandle,
    path_key: String,
    transcriber: Arc<dyn TranscriptionProvider>,
    prepared: &super::shared::PreparedToolAudio,
    speech_regions: &[crate::vad::SpeechRegion],
    settings: &AppSettings,
    vocabulary: &[String],
    stt_auto_mode: bool,
) -> Result<RegionTranscriptionOutcome, String> {
    let raw_regions: Vec<(u64, crate::audio::segment::AudioSegment)> = if speech_regions.is_empty() {
        vec![(0, prepared.segment.clone())]
    } else {
        speech_regions
            .iter()
            .map(|region| (region.start_ms, region.audio.clone()))
            .collect()
    };
    let max_chunk_ms = settings.vad_config().maximum_segment_ms as u64;
    let regions = expand_stt_chunks(raw_regions, max_chunk_ms);

    let total_ms = regions
        .iter()
        .map(|(_, audio)| audio.duration_ms.max(1))
        .sum::<u64>()
        .max(1);

    let mut merged = Vec::new();
    let mut detected_language = None;
    let mut auto_language_hint: Option<String> = None;
    let mut processed_ms = 0u64;

    for (offset_ms, audio) in &regions {
        let prompt_input = WhisperPromptInput {
            settings,
            vocabulary,
            previous_text: None,
        };
        let chunk_progress = {
            let app = app.clone();
            let path_key = path_key.clone();
            let base_ms = processed_ms;
            let span_ms = audio.duration_ms.max(1);
            throttled_percent_callback(Arc::new(move |local_percent: u8| {
                let local = (local_percent as u64).saturating_mul(span_ms) / 100;
                let overall = ((base_ms.saturating_add(local)).saturating_mul(100) / total_ms)
                    .min(100) as u8;
                emit_progress(
                    &app,
                    &path_key,
                    AudioSrtProgressPhase::Transcribing,
                    Some(overall),
                );
            }))
        };

        let transcription = transcribe_segment_with_retries(
            transcriber.clone(),
            audio.clone(),
            audio.clone(),
            settings,
            prompt_input,
            Some(chunk_progress),
            TranscribeSegmentFlags {
                request_segment_timestamps: true,
            },
            auto_language_hint.clone(),
        )
        .await
        .map_err(|error| error.user_message(settings.ui_locale))?;

        if settings.language.is_none() {
            if auto_language_hint.is_none() {
                auto_language_hint = transcription.detected_language.clone();
            }
            if detected_language.is_none() {
                detected_language = transcription.detected_language.clone();
            }
        } else if detected_language.is_none() {
            detected_language = settings.language.clone();
        }

        if let Some(segments) = transcription.timed_segments {
            merge_region_timed_segments(&mut merged, segments, *offset_ms);
        }

        processed_ms = processed_ms.saturating_add(audio.duration_ms);
    }

    sort_timed_segments_by_start(&mut merged);

    if merged.is_empty() {
        if stt_auto_mode {
            return Err(super::shared::STT_SELECT_LANGUAGE_ERROR.to_string());
        }
        return Err("tools.audioSrt.noTimestamps".to_string());
    }

    Ok(RegionTranscriptionOutcome {
        segments: merged,
        detected_language,
    })
}

struct ProcessedSegmentsOutcome {
    segments: Vec<TimedTextSegment>,
    rewrite_fallback: bool,
    rewrite_fallback_reason: Option<String>,
    ai_rewrite_applied: bool,
}

async fn process_timed_segments_for_subtitles(
    app: &AppHandle,
    ctx: &AppContext,
    settings: &AppSettings,
    path_key: &str,
    segments: &[TimedTextSegment],
    whisper_detected_language: Option<&str>,
    ai_mode: TextProcessingMode,
    dictionary: &Dictionary,
) -> Result<ProcessedSegmentsOutcome, String> {
    if ai_mode.uses_ai() {
        emit_phase(app, path_key, AudioSrtProgressPhase::AiRewrite);
        info!("audio SRT AI text rewrite per segment ({})", ai_mode.as_str());
        if crate::llm::model_store::needs_local_llm(settings) {
            LlmEngine::ensure_loaded(settings, &ctx.llm_engine)
                .map_err(|error| AppError::Internal(error).to_string())?;
        }
    }

    let llm_snapshot = ctx
        .llm_engine
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone());

    let terms = protected_terms(dictionary);
    let mut rewrite_fallback = false;
    let mut rewrite_fallback_reason = None;
    let mut ai_applied = false;
    let mut processed = Vec::with_capacity(segments.len());

    for segment in segments {
        let immediate = process_transcription_immediate_sync(
            &segment.text,
            None,
            settings,
            whisper_detected_language,
            None,
        )
        .map_err(|error| error.to_string())?;

        let mut text = immediate.text;
        if ai_mode.uses_ai() {
            ai_applied = true;
            let rewritten = rewrite_processed_text(
                &text,
                Some(&immediate.stt_cleanup_text),
                ai_mode,
                settings,
                settings.ai_rewrite_skill.as_deref(),
                &ctx.http,
                &llm_snapshot,
                &terms,
                dictionary,
                immediate.press_enter,
            )
            .await
            .map_err(|error| error.to_string())?;
            if rewritten.rewrite_fallback {
                rewrite_fallback = true;
                rewrite_fallback_reason = rewritten.rewrite_fallback_reason.clone();
            }
            text = rewritten.text;
        }

        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        processed.push(TimedTextSegment {
            text: trimmed.to_string(),
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            words: Vec::new(),
        });
    }

    Ok(ProcessedSegmentsOutcome {
        segments: processed,
        rewrite_fallback,
        rewrite_fallback_reason,
        ai_rewrite_applied: ai_applied && !rewrite_fallback,
    })
}
