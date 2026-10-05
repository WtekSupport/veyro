use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;
use tracing::info;

use crate::app::context::AppContext;
use crate::app::events::{
    emit_speech_analysis_progress, SpeechAnalysisProgressPayload, SpeechAnalysisProgressPhase,
};
use crate::app::transcribe_audio::{transcribe_segment_with_retries, TranscribeSegmentFlags};
use crate::text::dictionary::load_dictionary_for_settings;
use crate::transcription::prompt::WhisperPromptInput;
use crate::transcription::models::WhisperProgressCallback;
use crate::vad::detect_speech_spans;

use super::align::{
    apply_energy_refined_word_timings, ensure_timed_segments_from_transcript, refine_word_timings,
};
use super::articulation::analyze_articulation;
use super::config::SpeechAnalysisConfig;
use super::export_md::write_markdown_report;
use super::fluency::analyze_fluency;
use super::intelligibility::analyze_intelligibility;
use super::prosody::analyze_prosody;
use super::qc::analyze_qc;
use super::coach::{generate_coach, should_run_coach};
use super::confidence_from_articulation::apply_word_confidence_from_articulation;
use super::ensure_models::ensure_plan_models;
use super::model_plan::{resolve_plan, SpeechAnalysisModelPolicy};
use super::summary::{build_empty_summary, build_summary, caveat_export_keys};
use super::transcript_qc::analyze_transcript_quality;
use crate::transcription::sherpa::merge_transcript_pieces;
use super::types::{SpeechAnalysisOptions, SpeechAnalysisReport};
use crate::settings::LocalSttVariant;
use crate::tools::audio_to_srt_timing::{
    expand_stt_chunks, merge_region_timed_segments, sort_timed_segments_by_start,
};
use crate::tools::heavy_job::run_stage;
use crate::tools::shared::{
    decode_and_preprocess_for_tools, stt_failed_in_auto_mode, throttled_percent_callback,
    tool_transcriber, ToolsTranscriptionGuard, STT_SELECT_LANGUAGE_ERROR, validate_tool_file_path,
};
use crate::vad::detect_speech_regions;

pub async fn analyze_speech_analysis_file(
    app: AppHandle,
    ctx: Arc<AppContext>,
    path: String,
    options: SpeechAnalysisOptions,
) -> Result<SpeechAnalysisReport, String> {
    let _guard = ToolsTranscriptionGuard::try_begin(&ctx)?;

    let (path_key, path_buf, file_name) = validate_tool_file_path(&path, "tools.speechAnalysis")?;

    let base_settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();
    let mut settings = base_settings.clone();
    if let Some(language) = options
        .stt_language_override
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.language = Some(language.to_string());
    }
    let stt_auto_mode =
        stt_failed_in_auto_mode(&base_settings, options.stt_language_override.as_deref());
    let language_for_plan = options
        .stt_language_override
        .as_deref()
        .or(settings.language.as_deref());
    let plan = resolve_plan(
        &settings,
        language_for_plan,
        options.model_policy,
        options.manual_variant.map(LocalSttVariant::from),
    );
    if !options.fill_gaps_only
        && options.auto_download_models
        && options.model_policy == SpeechAnalysisModelPolicy::Auto
        && !plan.to_download.is_empty()
    {
        ensure_plan_models(&app, &ctx, &settings, &plan, &path_key).await?;
    }

    info!(
        target: "veyro.tools.heavy",
        file = %file_name,
        "speech analysis begin"
    );

    emit_phase(&app, &path_key, SpeechAnalysisProgressPhase::Decoding, None);
    let decode_progress: WhisperProgressCallback = throttled_percent_callback(Arc::new({
        let app = app.clone();
        let path_key = path_key.clone();
        move |percent: u8| {
            emit_progress(
                &app,
                &path_key,
                SpeechAnalysisProgressPhase::Decoding,
                Some(map_decode_percent(percent)),
            );
        }
    }));
    let path_buf_decode = path_buf.clone();
    let settings_decode = settings.clone();
    let decode_progress_cb = decode_progress.clone();
    let prepared = run_stage("speech-analysis-decode", move || {
        decode_and_preprocess_for_tools(
            &path_buf_decode,
            &settings_decode,
            Some(decode_progress_cb),
            "tools.speechAnalysis",
        )
    })
    .await?;

    let config = SpeechAnalysisConfig::default();
    let segment = prepared.segment.clone();
    let total_duration_ms = segment.duration_ms.max(1);

    if prepared.skipped_as_silence {
        emit_phase(&app, &path_key, SpeechAnalysisProgressPhase::Done, Some(100));
        return Ok(empty_report(path_key, file_name));
    }

    let vad_config = settings.vad_config();
    let vad_config_for_spans = vad_config.clone();
    let speech_spans = run_stage("speech-analysis-vad", move || {
        detect_speech_spans(&segment, &vad_config_for_spans).map_err(|error| error.to_string())
    })
    .await?;

    let qc = if options.fill_gaps_only {
        options
            .cache
            .as_ref()
            .map(|cache| cache.qc.clone())
            .unwrap_or_else(|| {
                analyze_qc(
                    &prepared.captured,
                    &speech_spans,
                    Some(prepared.source_file_sample_rate_hz),
                    prepared.segment.sample_rate,
                )
            })
    } else {
        analyze_qc(
            &prepared.captured,
            &speech_spans,
            Some(prepared.source_file_sample_rate_hz),
            prepared.segment.sample_rate,
        )
    };

    let (mut timed_segments, detected_language, transcript, transcript_confidence) =
        if options.fill_gaps_only {
            let cache = options
                .cache
                .as_ref()
                .ok_or_else(|| "tools.speechAnalysis.fillGapsMissingCache".to_string())?;
            (
                cache.timed_segments.clone(),
                cache.detected_language.clone(),
                cache.transcript.clone(),
                None,
            )
        } else {
            emit_phase(
                &app,
                &path_key,
                SpeechAnalysisProgressPhase::Transcribing,
                None,
            );
            let dictionary = load_dictionary_for_settings(&settings).unwrap_or_default();
            let transcriber = transcriber_for_analysis(&ctx, &settings, &plan);
            let speech_regions = detect_speech_regions(&prepared.segment, &vad_config)
                .map_err(|error| error.to_string())?;
            let outcome = transcribe_for_analysis(
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
            if outcome.2.trim().is_empty() && stt_auto_mode {
                return Err(STT_SELECT_LANGUAGE_ERROR.to_string());
            }
            outcome
        };

    let speech_start_ms = speech_spans.iter().map(|(start, _)| *start).min().unwrap_or(0);
    let speech_end_ms = speech_spans
        .iter()
        .map(|(_, end)| *end)
        .max()
        .unwrap_or(total_duration_ms)
        .max(speech_start_ms.saturating_add(1));
    ensure_timed_segments_from_transcript(
        &mut timed_segments,
        &transcript,
        speech_start_ms,
        speech_end_ms,
    );
    let audio_for_word_align = prepared.segment.clone();
    timed_segments = run_stage("speech-analysis-word-align", move || {
        apply_energy_refined_word_timings(&mut timed_segments, &audio_for_word_align);
        Ok(timed_segments)
    })
    .await?;

    emit_phase(
        &app,
        &path_key,
        SpeechAnalysisProgressPhase::Analyzing,
        Some(85),
    );

    let language_hint = detected_language.as_deref().or(settings.language.as_deref());

    emit_phase(
        &app,
        &path_key,
        SpeechAnalysisProgressPhase::Analyzing,
        Some(88),
    );

    let segment_for_metrics = prepared.segment.clone();
    let spans_for_metrics = speech_spans.clone();
    let config_for_prosody = config.clone();
    let prosody = if options.fill_gaps_only {
        options
            .cache
            .as_ref()
            .map(|cache| cache.prosody.clone())
            .unwrap_or_else(|| {
                analyze_prosody(
                    &segment_for_metrics,
                    &spans_for_metrics,
                    &config_for_prosody,
                )
            })
    } else {
        run_stage("speech-analysis-prosody", move || {
            Ok(analyze_prosody(
                &segment_for_metrics,
                &spans_for_metrics,
                &config_for_prosody,
            ))
        })
        .await?
    };
    emit_phase(
        &app,
        &path_key,
        SpeechAnalysisProgressPhase::Analyzing,
        Some(92),
    );

    let audio_for_articulation = prepared.segment.clone();
    let timed_for_articulation = timed_segments.clone();
    let transcript_for_articulation = transcript.clone();
    let settings_for_articulation = settings.clone();
    let language_for_ctc = detected_language.clone();
    let articulation = analyze_articulation(
        &settings_for_articulation,
        &audio_for_articulation,
        &timed_for_articulation,
        &transcript_for_articulation,
        language_for_ctc,
        &config,
    )
    .await;
    apply_word_confidence_from_articulation(&mut timed_segments, &articulation);
    let intelligibility = analyze_intelligibility(&timed_segments, transcript_confidence);
    let mono_for_fluency = crate::audio::resampler::to_mono(
        &prepared.segment.samples,
        prepared.segment.channels,
    );
    let fluency = analyze_fluency(
        &timed_segments,
        &speech_spans,
        total_duration_ms,
        language_hint,
        &config,
        &mono_for_fluency,
        prepared.segment.sample_rate,
    );
    emit_phase(
        &app,
        &path_key,
        SpeechAnalysisProgressPhase::Analyzing,
        Some(99),
    );

    let word_timings = refine_word_timings(&prepared.segment, &timed_segments);
    let language_ru = detected_language
        .as_deref()
        .map(|l| l.starts_with("ru"))
        .unwrap_or(true);
    let dictionary_vocab = load_dictionary_for_settings(&settings)
        .map(|d| d.vocabulary)
        .unwrap_or_default();
    let transcript_qc =
        analyze_transcript_quality(&transcript, language_ru, &dictionary_vocab);
    let summary = build_summary(
        &qc,
        &fluency,
        &prosody,
        &intelligibility,
        &articulation,
        &transcript,
        &word_timings,
        &config,
        language_ru,
        &transcript_qc,
        &dictionary_vocab,
    );
    let limitations = caveat_export_keys(&summary.caveats);

    let mut coach = if options.fill_gaps_only && !options.regenerate_coach {
        options
            .cache
            .as_ref()
            .and_then(|cache| cache.coach.clone())
    } else {
        None
    };

    let mut report = SpeechAnalysisReport {
        path_key: path_key.clone(),
        file_name,
        transcript,
        timed_segments,
        detected_language,
        qc,
        fluency,
        prosody,
        intelligibility,
        articulation,
        summary,
        limitations,
        coach: None,
    };

    if coach.is_none() && should_run_coach(&options, &settings) {
        emit_phase(
            &app,
            &path_key,
            SpeechAnalysisProgressPhase::Interpreting,
            Some(99),
        );
        coach = Some(generate_coach(&ctx, &settings, &options, &report).await);
    }
    report.coach = coach;

    if let Err(error) = super::disk_cache::save_report_snapshot(&settings, &report) {
        tracing::warn!(target: "veyro.tools.heavy", "speech analysis disk cache: {error}");
    }

    emit_phase(&app, &path_key, SpeechAnalysisProgressPhase::Done, Some(100));
    info!(
        target: "veyro.tools.heavy",
        file = %report.file_name,
        words = report.fluency.word_count,
        "speech analysis complete"
    );
    Ok(report)
}

async fn transcribe_for_analysis(
    app: AppHandle,
    path_key: String,
    transcriber: Arc<dyn crate::transcription::TranscriptionProvider>,
    prepared: &crate::tools::shared::PreparedToolAudio,
    speech_regions: &[crate::vad::SpeechRegion],
    settings: &crate::settings::AppSettings,
    vocabulary: &[String],
    stt_auto_mode: bool,
) -> Result<
    (
        Vec<crate::timed_text::TimedTextSegment>,
        Option<String>,
        String,
        Option<f32>,
    ),
    String,
> {
    let raw_regions: Vec<(u64, crate::audio::segment::AudioSegment)> = if speech_regions.is_empty() {
        vec![(0, prepared.segment.clone())]
    } else {
        speech_regions
            .iter()
            .map(|region| (region.start_ms, region.audio.clone()))
            .collect()
    };
    let max_chunk_ms = settings.vad_config().maximum_segment_ms as u64;
    let max_chunk_ms = max_chunk_ms.min(crate::transcription::sherpa::max_offline_audio_ms(
        settings,
    ));
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
    let mut text_pieces: Vec<String> = Vec::new();
    let mut confidence_values = Vec::new();

    for (offset_ms, audio) in &regions {
        let prompt_input = WhisperPromptInput {
            settings,
            vocabulary,
            previous_text: None,
        };
        let chunk_progress = throttled_percent_callback(Arc::new({
            let app = app.clone();
            let path_key = path_key.clone();
            let base_ms = processed_ms;
            let span_ms = audio.duration_ms.max(1);
            move |local_percent: u8| {
                let local = (local_percent as u64).saturating_mul(span_ms) / 100;
                let overall = ((base_ms.saturating_add(local)).saturating_mul(100) / total_ms)
                    .min(100) as u8;
                emit_progress(
                    &app,
                    &path_key,
                    SpeechAnalysisProgressPhase::Transcribing,
                    Some(map_transcribe_percent(overall)),
                );
            }
        }));

        let transcription = transcribe_segment_with_retries(
            transcriber.clone(),
            audio.clone(),
            audio.clone(),
            settings,
            prompt_input,
            Some(chunk_progress),
            TranscribeSegmentFlags {
                request_segment_timestamps: true,
                request_word_timestamps: true,
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

        if let Some(confidence) = transcription.confidence {
            confidence_values.push(confidence);
        }

        if !transcription.text.trim().is_empty() {
            text_pieces.push(transcription.text.trim().to_string());
        }

        if let Some(segments) = transcription.timed_segments {
            merge_region_timed_segments(&mut merged, segments, *offset_ms);
        }

        processed_ms = processed_ms.saturating_add(audio.duration_ms);
    }

    sort_timed_segments_by_start(&mut merged);

    let full_text = merge_transcript_pieces(&text_pieces);
    if full_text.trim().is_empty() && stt_auto_mode {
        return Ok((merged, detected_language, String::new(), None));
    }

    let mean_confidence = if confidence_values.is_empty() {
        None
    } else {
        Some(confidence_values.iter().sum::<f32>() / confidence_values.len() as f32)
    };

    Ok((merged, detected_language, full_text, mean_confidence))
}

fn empty_report(path_key: String, file_name: String) -> SpeechAnalysisReport {
    let qc = analyze_qc(
        &crate::audio::segment::AudioSegment::new(Vec::new(), 16_000, 1),
        &[],
        None,
        16_000,
    );
    let fluency = analyze_fluency(
        &[],
        &[],
        0,
        None,
        &SpeechAnalysisConfig::default(),
        &[],
        16_000,
    );
    let prosody = analyze_prosody(
        &crate::audio::segment::AudioSegment::new(Vec::new(), 16_000, 1),
        &[],
        &SpeechAnalysisConfig::default(),
    );
    let intelligibility = analyze_intelligibility(&[], None);
    let articulation = super::types::ArticulationReport {
        reliability: super::types::ReliabilityLevel::Unavailable,
        unavailable_reason_key: Some("tools.speechAnalysis.noSpeech".to_string()),
        ctc_variant_used: None,
        missing_ctc_download: None,
        mean_gop: None,
        per: None,
        low_gop_token_count: 0,
        alignment_token_count: 0,
        letter_baseline_error_rate_percent: None,
        weak_symbols: Vec::new(),
        top_substitutions: Vec::new(),
        phoneme_segments: Vec::new(),
        word_gop_hits: Vec::new(),
    };
    let summary = build_empty_summary(true, "");
    let limitations = caveat_export_keys(&summary.caveats);
    SpeechAnalysisReport {
        path_key,
        file_name,
        transcript: String::new(),
        timed_segments: Vec::new(),
        detected_language: None,
        qc,
        fluency,
        prosody,
        intelligibility,
        articulation,
        summary,
        limitations,
        coach: None,
    }
}

fn map_decode_percent(local: u8) -> u8 {
    ((local as u16).saturating_mul(25) / 100) as u8
}

fn map_transcribe_percent(local: u8) -> u8 {
    (25 + (local as u16).saturating_mul(60) / 100).min(100) as u8
}

fn transcriber_for_analysis(
    ctx: &AppContext,
    settings: &crate::settings::AppSettings,
    plan: &super::model_plan::SpeechAnalysisPlan,
) -> Arc<dyn crate::transcription::TranscriptionProvider> {
    if let Some(main) = plan.main {
        let mut scoped = settings.clone();
        scoped.set_local_stt_variant(LocalSttVariant::from(main));
        tool_transcriber(ctx, &scoped)
    } else {
        tool_transcriber(ctx, settings)
    }
}

fn emit_phase(app: &AppHandle, path: &str, phase: SpeechAnalysisProgressPhase, percent: Option<u8>) {
    emit_speech_analysis_progress(
        app,
        SpeechAnalysisProgressPayload {
            path: path.to_string(),
            phase,
            percent,
        },
    );
}

fn emit_progress(
    app: &AppHandle,
    path: &str,
    phase: SpeechAnalysisProgressPhase,
    percent: Option<u8>,
) {
    emit_phase(app, path, phase, percent);
}

pub fn pick_export_path(default_name: &str, extension: &str) -> Result<Option<PathBuf>, String> {
    let dialog = match extension {
        "md" => rfd::FileDialog::new()
            .set_file_name(default_name)
            .add_filter("Markdown", &["md"]),
        _ => rfd::FileDialog::new().set_file_name(default_name),
    };
    Ok(dialog.save_file())
}

pub fn export_markdown(path: &PathBuf, report: &SpeechAnalysisReport) -> Result<(), String> {
    write_markdown_report(path.as_path(), report)
}
