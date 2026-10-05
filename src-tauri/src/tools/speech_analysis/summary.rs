use super::config::SpeechAnalysisConfig;
use super::transcript_qc::TranscriptQualityReport;
use crate::word_align::WordTiming;

use super::problems::build_problems;
use super::scoring::{bell_score, prosody_intonation_score};
use super::types::{
    ArticulationReport, DimensionStatus, FluencyReport, IntelligibilityReport, OverallScoreMode,
    ProsodyReport, QcReport, ReliabilityLevel, SpeechAnalysisCaveat, SpeechAnalysisCoverage,
    SpeechAnalysisReliability, SpeechAnalysisSummary, SummaryDimension,
};

const WEIGHT_CONFIDENCE: f32 = 10.0;
const WEIGHT_INTELLIGIBILITY: f32 = 20.0;
const WEIGHT_ARTICULATION: f32 = 35.0;
const WEIGHT_FLUENCY: f32 = 20.0;
const WEIGHT_PROSODY: f32 = 15.0;

const DICTION_AXIS_IDS: [&str; 5] = [
    "articulation",
    "intelligibility",
    "fluency",
    "prosody",
    "confidence",
];

pub fn build_summary(
    qc: &QcReport,
    fluency: &FluencyReport,
    prosody: &ProsodyReport,
    intelligibility: &IntelligibilityReport,
    articulation: &ArticulationReport,
    transcript: &str,
    word_timings: &[WordTiming],
    config: &SpeechAnalysisConfig,
    language_ru: bool,
    transcript_qc: &TranscriptQualityReport,
    extra_vocabulary: &[String],
) -> SpeechAnalysisSummary {
    let speech_ms = qc.speech_duration_ms;
    let reliability = build_reliability(qc);

    let signal_quality = dimension_signal_quality(qc);
    let confidence = dimension_confidence(intelligibility, fluency.word_count, config);
    let mut intelligibility_dim =
        dimension_intelligibility(intelligibility, fluency.word_count, config);
    if transcript_qc.degraded {
        if let Some(score) = intelligibility_dim.score {
            intelligibility_dim.score = Some(score.min(85));
            intelligibility_dim.grade_key = grade_key_for_score(intelligibility_dim.score);
        }
    }
    if intelligibility_dim.status != DimensionStatus::Unavailable {
        intelligibility_dim.detail_key =
            Some("tools.speechAnalysis.summary.detail.intelligibilityMerged".to_string());
    }
    let articulation_dim = dimension_articulation(articulation, speech_ms, config);
    let fluency_dim = dimension_fluency(fluency, speech_ms, config);
    let prosody_dim = dimension_prosody(prosody, speech_ms, config);

    let show_confidence_card = intelligibility_dim.status == DimensionStatus::Unavailable;
    let mut dimensions = vec![signal_quality];
    if show_confidence_card {
        dimensions.push(confidence.clone());
    }
    dimensions.extend([
        intelligibility_dim.clone(),
        articulation_dim.clone(),
        fluency_dim.clone(),
        prosody_dim.clone(),
    ]);

    let mut caveats = collect_caveats(prosody, intelligibility, articulation, &articulation_dim);
    if transcript_qc.degraded {
        caveats.push(SpeechAnalysisCaveat {
            code: "transcriptQualityDegraded".to_string(),
            param: Some(format!(
                "suspicious={};duplicatePhrases={}",
                transcript_qc.suspicious_word_count, transcript_qc.duplicate_ngram_count
            )),
        });
    }

    let articulation_char_proxy = articulation_is_char_level_proxy(&articulation_dim);
    let (overall_score_mode, overall_score, overall_label_key, coverage, weak_spot_id) =
        compute_overall(
            &confidence,
            &intelligibility_dim,
            &articulation_dim,
            &fluency_dim,
            &prosody_dim,
            articulation_char_proxy,
            transcript_qc.degraded,
        );

    if overall_score_mode == OverallScoreMode::Preliminary {
        caveats.push(SpeechAnalysisCaveat {
            code: "preliminaryScore".to_string(),
            param: None,
        });
    }

    let overall_show_grade = coverage.as_ref().is_some_and(|c| {
        c.included >= 4 && !articulation_char_proxy && !transcript_qc.degraded
    });
    let overall_grade_key = if overall_show_grade {
        grade_key_for_score(overall_score)
    } else {
        "tools.speechAnalysis.summary.grade.unavailable".to_string()
    };
    let problems = build_problems(
        transcript,
        fluency,
        intelligibility,
        articulation,
        word_timings,
        config,
        language_ru,
        transcript_qc.degraded,
        extra_vocabulary,
    );

    SpeechAnalysisSummary {
        overall_score_mode,
        overall_score,
        overall_grade_key,
        overall_label_key,
        overall_show_grade,
        overall_coverage: coverage,
        overall_weak_spot_id: weak_spot_id,
        reliability,
        dimensions,
        caveats,
        problems,
    }
}

pub fn build_empty_summary(no_speech: bool, transcript: &str) -> SpeechAnalysisSummary {
    let qc = QcReport {
        duration_ms: 0,
        speech_duration_ms: 0,
        sample_rate_hz: 16_000,
        source_file_sample_rate_hz: None,
        snr_db_estimate: None,
        clip_ratio: 0.0,
        narrowband: false,
        reliability: ReliabilityLevel::Unavailable,
        flags: Vec::new(),
    };
    let fluency = FluencyReport {
        word_count: 0,
        syllable_count: 0,
        wpm_overall: None,
        wpm_phonation: None,
        phonation_ratio: 0.0,
        pause_count: 0,
        mean_pause_ms: 0.0,
        long_pause_count: 0,
        pause_total_ms: 0,
        min_pause_ms: 200,
        long_pause_ms: 250,
        very_long_pause_ms: 1500,
        pause_count_punctuation: 0,
        pause_count_mid_phrase: 0,
        very_long_pause_count: 0,
        pause_measurement_reliable: false,
        fillers_per_100_words: 0.0,
        filler_hits: Vec::new(),
        repetition_count: 0,
    };
    let prosody = ProsodyReport {
        f0_median_hz: None,
        f0_range_semitones: None,
        f0_std_semitones: 0.0,
        voiced_fraction: 0.0,
        expressiveness: super::types::PitchExpressiveness::Moderate,
        low_confidence: true,
        f0_contour: Vec::new(),
        voice_quality: None,
    };
    let intelligibility = IntelligibilityReport {
        mean_word_confidence: None,
        low_confidence_word_count: 0,
        low_confidence_words: Vec::new(),
        reliability: ReliabilityLevel::Unavailable,
    };
    let articulation = ArticulationReport {
        reliability: ReliabilityLevel::Unavailable,
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
    let config = SpeechAnalysisConfig::default();
    let mut summary = build_summary(
        &qc,
        &fluency,
        &prosody,
        &intelligibility,
        &articulation,
        transcript,
        &[],
        &config,
        true,
        &TranscriptQualityReport::default(),
        &[],
    );
    if no_speech {
        summary.caveats.push(SpeechAnalysisCaveat {
            code: "noSpeech".to_string(),
            param: None,
        });
    }
    summary
}

pub fn caveat_export_keys(caveats: &[SpeechAnalysisCaveat]) -> Vec<String> {
    caveats
        .iter()
        .map(|c| format!("tools.speechAnalysis.caveat.{}", c.code))
        .collect()
}

fn build_reliability(qc: &QcReport) -> SpeechAnalysisReliability {
    let mut score = 100.0f32;
    if let Some(snr) = qc.snr_db_estimate {
        score = score.min(snr_to_score(snr));
    }
    score = score.min((1.0 - qc.clip_ratio.min(1.0)) * 100.0);
    if qc.clip_ratio > 0.001 {
        score -= (qc.clip_ratio * 2000.0).min(25.0);
    }
    if qc.narrowband {
        score -= 15.0;
    }
    if qc.snr_db_estimate.is_none() {
        score = score.min(85.0);
    }
    let score = score.clamp(0.0, 100.0).round() as u8;
    let level = match qc.reliability {
        ReliabilityLevel::High => ReliabilityLevel::High,
        ReliabilityLevel::Medium => ReliabilityLevel::Medium,
        ReliabilityLevel::Low => ReliabilityLevel::Low,
        ReliabilityLevel::Unavailable => ReliabilityLevel::Low,
    };
    SpeechAnalysisReliability {
        level,
        score,
        label_key: grade_key_for_score(Some(score)),
    }
}

fn collect_caveats(
    prosody: &ProsodyReport,
    intelligibility: &IntelligibilityReport,
    articulation: &ArticulationReport,
    articulation_dim: &SummaryDimension,
) -> Vec<SpeechAnalysisCaveat> {
    let mut out = Vec::new();

    if intelligibility.reliability == ReliabilityLevel::Unavailable {
        out.push(SpeechAnalysisCaveat {
            code: "wordConfidenceUnavailable".to_string(),
            param: None,
        });
    }
    if articulation.missing_ctc_download.is_some() {
        out.push(SpeechAnalysisCaveat {
            code: "articulationCtcMissing".to_string(),
            param: None,
        });
    } else if articulation
        .unavailable_reason_key
        .as_deref()
        .is_some_and(|key| key.contains("ctcDecodeFailed"))
    {
        out.push(SpeechAnalysisCaveat {
            code: "articulationCtcDecodeFailed".to_string(),
            param: None,
        });
    }
    if articulation_dim.status == DimensionStatus::Available {
        out.push(SpeechAnalysisCaveat {
            code: "articulationCharLevel".to_string(),
            param: None,
        });
    }
    if prosody.low_confidence {
        out.push(SpeechAnalysisCaveat {
            code: "prosodyLowConfidence".to_string(),
            param: None,
        });
    }
    out
}

fn dimension_signal_quality(qc: &QcReport) -> SummaryDimension {
    if qc.reliability == ReliabilityLevel::Unavailable {
        return unavailable_dimension("signalQuality", None);
    }
    let mut score = 100.0f32;
    if let Some(snr) = qc.snr_db_estimate {
        score = score.min(snr_to_score(snr));
    }
    score = score.min((1.0 - qc.clip_ratio.min(1.0)) * 100.0);
    if qc.clip_ratio > 0.001 {
        score -= (qc.clip_ratio * 2000.0).min(25.0);
    }
    if qc.narrowband {
        score -= 15.0;
    }
    if qc.snr_db_estimate.is_none() {
        score = score.min(85.0);
    }
    let score = score.clamp(0.0, 100.0).round() as u8;
    SummaryDimension {
        id: "signalQuality".to_string(),
        status: DimensionStatus::Available,
        score: Some(score),
        grade_key: grade_key_for_score(Some(score)),
        detail_key: Some("tools.speechAnalysis.summary.detail.signalQuality".to_string()),
        reason_key: None,
        reason_required_sec: None,
        reason_required_words: None,
        reason_required_tokens: None,
    }
}

fn snr_to_score(snr: f32) -> f32 {
    if snr >= 30.0 {
        100.0
    } else if snr >= 25.0 {
        92.0
    } else if snr >= 20.0 {
        85.0
    } else if snr >= 15.0 {
        75.0
    } else if snr >= 10.0 {
        55.0
    } else {
        35.0
    }
}

fn dimension_confidence(
    intelligibility: &IntelligibilityReport,
    word_count: usize,
    config: &SpeechAnalysisConfig,
) -> SummaryDimension {
    if intelligibility.reliability == ReliabilityLevel::Unavailable {
        return unavailable_dimension(
            "confidence",
            Some("tools.speechAnalysis.summary.reason.wordConfidenceUnavailable".to_string()),
        );
    }
    if word_count < config.min_words_intelligibility {
        return insufficient_words(
            "confidence",
            config.min_words_intelligibility as u32,
        );
    }
    let Some(mean) = intelligibility.mean_word_confidence else {
        return unavailable_dimension(
            "confidence",
            Some("tools.speechAnalysis.summary.reason.wordConfidenceUnavailable".to_string()),
        );
    };
    let score = (mean.clamp(0.0, 1.0) * 100.0).round() as u8;
    available_dimension(
        "confidence",
        score,
        "tools.speechAnalysis.summary.detail.confidence".to_string(),
    )
}

fn dimension_intelligibility(
    intelligibility: &IntelligibilityReport,
    word_count: usize,
    config: &SpeechAnalysisConfig,
) -> SummaryDimension {
    if intelligibility.reliability == ReliabilityLevel::Unavailable {
        return unavailable_dimension(
            "intelligibility",
            Some("tools.speechAnalysis.summary.reason.wordConfidenceUnavailable".to_string()),
        );
    }
    if word_count < config.min_words_intelligibility {
        return insufficient_words(
            "intelligibility",
            config.min_words_intelligibility as u32,
        );
    }
    let low_ratio = intelligibility.low_confidence_word_count as f32 / word_count.max(1) as f32;
    let score = ((1.0 - low_ratio.min(1.0)) * 100.0).round() as u8;
    available_dimension(
        "intelligibility",
        score,
        "tools.speechAnalysis.summary.detail.intelligibilityProxy".to_string(),
    )
}

fn dimension_articulation(
    articulation: &ArticulationReport,
    speech_ms: u64,
    config: &SpeechAnalysisConfig,
) -> SummaryDimension {
    if articulation.missing_ctc_download.is_some() {
        return SummaryDimension {
            id: "articulation".to_string(),
            status: DimensionStatus::Unavailable,
            score: None,
            grade_key: "tools.speechAnalysis.summary.grade.unavailable".to_string(),
            detail_key: None,
            reason_key: Some("tools.speechAnalysis.summary.reason.needsCtcModel".to_string()),
            reason_required_sec: None,
            reason_required_words: None,
            reason_required_tokens: None,
        };
    }
    if articulation.reliability == ReliabilityLevel::Unavailable {
        return unavailable_dimension(
            "articulation",
            Some("tools.speechAnalysis.summary.reason.articulationUnavailable".to_string()),
        );
    }
    let Some(score) = articulation_proxy_score(articulation) else {
        if articulation.alignment_token_count < config.min_tokens_articulation {
            return insufficient_tokens(
                "articulation",
                config.min_tokens_articulation as u32,
            );
        }
        return unavailable_dimension("articulation", None);
    };

    if speech_ms < config.min_speech_ms_articulation {
        let min_sec = (config.min_speech_ms_articulation / 1000) as u32;
        return SummaryDimension {
            id: "articulation".to_string(),
            status: DimensionStatus::InsufficientData,
            score: None,
            grade_key: "tools.speechAnalysis.summary.grade.unavailable".to_string(),
            detail_key: Some("tools.speechAnalysis.summary.detail.articulationDraft".to_string()),
            reason_key: Some(
                "tools.speechAnalysis.summary.reason.needsLongerRecording".to_string(),
            ),
            reason_required_sec: Some(min_sec),
            reason_required_words: None,
            reason_required_tokens: None,
        };
    }
    if articulation.alignment_token_count < config.min_tokens_articulation {
        return insufficient_tokens_with_score(
            "articulation",
            config.min_tokens_articulation as u32,
            score,
            Some("tools.speechAnalysis.summary.detail.articulationProxy".to_string()),
        );
    }
    available_dimension(
        "articulation",
        score,
        "tools.speechAnalysis.summary.detail.articulationProxy".to_string(),
    )
}

fn articulation_proxy_score(articulation: &ArticulationReport) -> Option<u8> {
    let mut parts = Vec::new();
    if let Some(per) = articulation.per {
        parts.push((1.0 - per.clamp(0.0, 1.0)) * 100.0);
    }
    if let Some(gop) = articulation.mean_gop {
        parts.push((gop.clamp(-1.0, 0.0) + 1.0) * 100.0);
    }
    if parts.is_empty() {
        None
    } else {
        Some((parts.iter().sum::<f32>() / parts.len() as f32).round() as u8)
    }
}

fn dimension_fluency(
    fluency: &FluencyReport,
    speech_ms: u64,
    config: &SpeechAnalysisConfig,
) -> SummaryDimension {
    if fluency.word_count == 0 {
        return unavailable_dimension(
            "fluency",
            Some("tools.speechAnalysis.summary.reason.noRecognizedWords".to_string()),
        );
    }
    if speech_ms < config.min_speech_ms_fluency {
        let min_sec = (config.min_speech_ms_fluency / 1000) as u32;
        return insufficient_duration("fluency", min_sec);
    }
    if !fluency.pause_measurement_reliable {
        return SummaryDimension {
            id: "fluency".to_string(),
            status: DimensionStatus::InsufficientData,
            score: None,
            grade_key: "tools.speechAnalysis.summary.grade.unavailable".to_string(),
            detail_key: Some("tools.speechAnalysis.summary.detail.fluency".to_string()),
            reason_key: Some(
                "tools.speechAnalysis.summary.reason.pauseMeasurementUnreliable".to_string(),
            ),
            reason_required_sec: None,
            reason_required_words: None,
            reason_required_tokens: None,
        };
    }
    let mut score = 100.0f32;
    score -= (fluency.fillers_per_100_words * 2.0).min(30.0);
    score -= (fluency.pause_count_mid_phrase as f32 * 2.5).min(22.0);
    score -= (fluency.very_long_pause_count as f32 * 5.0).min(20.0);
    score -= (fluency.long_pause_count as f32 * 2.0).min(15.0);
    score -= (fluency.repetition_count as f32 * 3.0).min(20.0);
    if let Some(wpm) = fluency.wpm_phonation {
        score = score * 0.4 + bell_score(wpm, 120.0, 160.0, 35.0) * 0.6;
    }
    let score = score.clamp(0.0, 100.0).round() as u8;
    available_dimension(
        "fluency",
        score,
        "tools.speechAnalysis.summary.detail.fluency".to_string(),
    )
}

fn dimension_prosody(prosody: &ProsodyReport, speech_ms: u64, config: &SpeechAnalysisConfig) -> SummaryDimension {
    if speech_ms < config.min_speech_ms_prosody {
        let min_sec = (config.min_speech_ms_prosody / 1000) as u32;
        return insufficient_duration("prosody", min_sec);
    }
    if prosody.low_confidence {
        return SummaryDimension {
            id: "prosody".to_string(),
            status: DimensionStatus::InsufficientData,
            score: None,
            grade_key: "tools.speechAnalysis.summary.grade.unavailable".to_string(),
            detail_key: Some("tools.speechAnalysis.summary.detail.prosodyLow".to_string()),
            reason_key: Some("tools.speechAnalysis.summary.reason.needsVoicedFrames".to_string()),
            reason_required_sec: None,
            reason_required_words: None,
            reason_required_tokens: None,
        };
    }
    let spread_score = prosody_intonation_score(
        prosody.f0_std_semitones,
        config.prosody_spread_ideal_lo,
        config.prosody_spread_ideal_hi,
    );
    let range_bonus = prosody
        .f0_range_semitones
        .map(|range| bell_score(range, 4.0, 14.0, 8.0))
        .unwrap_or(90.0);
    let score = (spread_score * 0.85 + range_bonus * 0.15).round() as u8;
    available_dimension(
        "prosody",
        score,
        "tools.speechAnalysis.summary.detail.prosody".to_string(),
    )
}

fn available_dimension(id: &str, score: u8, detail_key: String) -> SummaryDimension {
    SummaryDimension {
        id: id.to_string(),
        status: DimensionStatus::Available,
        score: Some(score),
        grade_key: grade_key_for_score(Some(score)),
        detail_key: Some(detail_key),
        reason_key: None,
        reason_required_sec: None,
        reason_required_words: None,
        reason_required_tokens: None,
    }
}

fn insufficient_duration(id: &str, min_sec: u32) -> SummaryDimension {
    insufficient_duration_with_score(id, min_sec, None, None)
}

fn insufficient_duration_with_score(
    id: &str,
    min_sec: u32,
    score: Option<u8>,
    detail_key: Option<String>,
) -> SummaryDimension {
    SummaryDimension {
        id: id.to_string(),
        status: DimensionStatus::InsufficientData,
        score,
        grade_key: if score.is_some() {
            grade_key_for_score(score)
        } else {
            "tools.speechAnalysis.summary.grade.unavailable".to_string()
        },
        detail_key,
        reason_key: Some(
            "tools.speechAnalysis.summary.reason.needsLongerRecording".to_string(),
        ),
        reason_required_sec: Some(min_sec),
        reason_required_words: None,
        reason_required_tokens: None,
    }
}

fn insufficient_tokens_with_score(
    id: &str,
    min_tokens: u32,
    score: u8,
    detail_key: Option<String>,
) -> SummaryDimension {
    SummaryDimension {
        id: id.to_string(),
        status: DimensionStatus::InsufficientData,
        score: Some(score),
        grade_key: grade_key_for_score(Some(score)),
        detail_key,
        reason_key: Some(
            "tools.speechAnalysis.summary.reason.needsMoreArticulationTokens".to_string(),
        ),
        reason_required_sec: None,
        reason_required_words: None,
        reason_required_tokens: Some(min_tokens),
    }
}

fn insufficient_words(id: &str, min_words: u32) -> SummaryDimension {
    SummaryDimension {
        id: id.to_string(),
        status: DimensionStatus::InsufficientData,
        score: None,
        grade_key: "tools.speechAnalysis.summary.grade.unavailable".to_string(),
        detail_key: None,
        reason_key: Some("tools.speechAnalysis.summary.reason.needsMoreWords".to_string()),
        reason_required_sec: None,
        reason_required_words: Some(min_words),
        reason_required_tokens: None,
    }
}

fn insufficient_tokens(id: &str, min_tokens: u32) -> SummaryDimension {
    SummaryDimension {
        id: id.to_string(),
        status: DimensionStatus::InsufficientData,
        score: None,
        grade_key: "tools.speechAnalysis.summary.grade.unavailable".to_string(),
        detail_key: None,
        reason_key: Some(
            "tools.speechAnalysis.summary.reason.needsMoreArticulationTokens".to_string(),
        ),
        reason_required_sec: None,
        reason_required_words: None,
        reason_required_tokens: Some(min_tokens),
    }
}

fn unavailable_dimension(id: &str, reason_key: Option<String>) -> SummaryDimension {
    SummaryDimension {
        id: id.to_string(),
        status: DimensionStatus::Unavailable,
        score: None,
        grade_key: "tools.speechAnalysis.summary.grade.unavailable".to_string(),
        detail_key: None,
        reason_key,
        reason_required_sec: None,
        reason_required_words: None,
        reason_required_tokens: None,
    }
}

fn grade_key_for_score(score: Option<u8>) -> String {
    let Some(score) = score else {
        return "tools.speechAnalysis.summary.grade.unavailable".to_string();
    };
    let key = match score {
        85..=100 => "excellent",
        70..=84 => "good",
        50..=69 => "fair",
        _ => "weak",
    };
    format!("tools.speechAnalysis.summary.grade.{key}")
}

fn is_scorable(dim: &SummaryDimension) -> bool {
    dim.status == DimensionStatus::Available && dim.score.is_some()
}

fn articulation_is_char_level_proxy(dim: &SummaryDimension) -> bool {
    if dim.id != "articulation" {
        return false;
    }
    if dim.status == DimensionStatus::InsufficientData {
        return true;
    }
    dim.detail_key.as_deref().is_some_and(|key| {
        key.contains("articulationProxy")
            || key.contains("articulationDraft")
            || key.contains("articulationCharLevel")
    })
}

fn compute_overall(
    confidence: &SummaryDimension,
    intelligibility: &SummaryDimension,
    articulation: &SummaryDimension,
    fluency: &SummaryDimension,
    prosody: &SummaryDimension,
    articulation_char_proxy: bool,
    transcript_degraded: bool,
) -> (
    OverallScoreMode,
    Option<u8>,
    String,
    Option<SpeechAnalysisCoverage>,
    Option<String>,
) {
    let articulation_ok = is_scorable(articulation) && !articulation_char_proxy;
    let intelligibility_ok = is_scorable(intelligibility);

    let use_intelligibility = intelligibility_ok;
    let use_confidence = !use_intelligibility && is_scorable(confidence);

    let mut weights: Vec<(Option<u8>, f32, &str)> = Vec::new();
    if articulation_ok {
        weights.push((articulation.score, WEIGHT_ARTICULATION, "articulation"));
    }
    if use_intelligibility {
        weights.push((intelligibility.score, WEIGHT_INTELLIGIBILITY, "intelligibility"));
    } else if use_confidence {
        weights.push((confidence.score, WEIGHT_CONFIDENCE, "confidence"));
    }
    if is_scorable(fluency) {
        weights.push((fluency.score, WEIGHT_FLUENCY, "fluency"));
    }
    if is_scorable(prosody) {
        weights.push((prosody.score, WEIGHT_PROSODY, "prosody"));
    }

    let included = weights.len() as u8;
    let total = DICTION_AXIS_IDS.len() as u8;
    if weights.is_empty() {
        return (
            OverallScoreMode::Hidden,
            None,
            "tools.speechAnalysis.summary.label.hidden".to_string(),
            Some(SpeechAnalysisCoverage {
                included: 0,
                total,
                included_ids: Vec::new(),
                missing_ids: DICTION_AXIS_IDS.iter().map(|s| s.to_string()).collect(),
            }),
            None,
        );
    }

    let score = weighted_overall(
        &weights
            .iter()
            .map(|(s, w, _)| (*s, *w))
            .collect::<Vec<_>>(),
    );

    let included_ids: Vec<String> = weights
        .iter()
        .map(|(_, _, axis)| (*axis).to_string())
        .collect();
    let missing_ids: Vec<String> = DICTION_AXIS_IDS
        .iter()
        .filter(|id| !included_ids.iter().any(|axis| axis == *id))
        .filter(|id| {
            !(**id == "confidence" && included_ids.iter().any(|axis| axis == "intelligibility"))
        })
        .map(|s| (*s).to_string())
        .collect();

    let weak_spot_id = weights
        .iter()
        .filter_map(|(score, _, axis)| score.map(|s| (s, *axis)))
        .min_by_key(|(score, _)| *score)
        .map(|(_, axis)| axis.to_string());

    let coverage = SpeechAnalysisCoverage {
        included,
        total,
        included_ids,
        missing_ids,
    };

    if articulation_ok && intelligibility_ok && !transcript_degraded {
        (
            OverallScoreMode::Full,
            score,
            "tools.speechAnalysis.summary.label.full".to_string(),
            Some(coverage),
            weak_spot_id,
        )
    } else {
        (
            OverallScoreMode::Preliminary,
            score,
            "tools.speechAnalysis.summary.label.preliminary".to_string(),
            Some(coverage),
            weak_spot_id,
        )
    }
}

fn weighted_overall(parts: &[(Option<u8>, f32)]) -> Option<u8> {
    let mut sum = 0.0f32;
    let mut weight = 0.0f32;
    for (score, w) in parts {
        if let Some(score) = score {
            sum += *score as f32 * w;
            weight += w;
        }
    }
    if weight <= 0.0 {
        return None;
    }
    Some((sum / weight).round().clamp(0.0, 100.0) as u8)
}
