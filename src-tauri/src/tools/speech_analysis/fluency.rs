use std::collections::HashMap;

use crate::timed_text::{TimedTextSegment, TimedWord};

use super::config::{load_fillers, RegisterNorms, SpeechAnalysisConfig};
use super::pause_classify::classify_pauses;
use super::pause_energy::{detect_pauses_from_energy, PauseInterval};
use super::qc::SpeechSpan;
use super::phonemize::count_syllables_in_words;
use super::speech_timing::{
    gross_speech_duration_ms, net_speech_duration_ms, pauses_inside_speech,
    trim_speech_span_edges,
};
use super::types::{F0ContourPoint, FillerHit, FluencyReport, SpeechRegister};

pub fn analyze_fluency(
    timed_segments: &[TimedTextSegment],
    speech_spans: &[SpeechSpan],
    total_duration_ms: u64,
    language_hint: Option<&str>,
    config: &SpeechAnalysisConfig,
    mono: &[f32],
    sample_rate: u32,
    f0_contour: &[F0ContourPoint],
) -> (FluencyReport, Vec<PauseInterval>, Vec<TimedWord>) {
    let words = collect_words(timed_segments);
    let _transcript = timed_segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let word_count = words.len();
    let syllable_count = count_syllables_in_words(
        &words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>(),
    );

    let trimmed_spans = trim_speech_span_edges(speech_spans, total_duration_ms);
    let spans_for_metrics: &[SpeechSpan] = if trimmed_spans.is_empty() {
        speech_spans
    } else {
        &trimmed_spans
    };
    let gross_speech_ms = gross_speech_duration_ms(spans_for_metrics);
    let total_duration_ms = total_duration_ms.max(1);

    let energy = detect_pauses_from_energy(
        mono,
        sample_rate,
        spans_for_metrics,
        total_duration_ms,
        config,
    );
    let pauses = pauses_inside_speech(&energy.pauses, spans_for_metrics, total_duration_ms);
    let pause_total_ms: u64 = pauses.iter().map(|p| p.duration_ms).sum();
    let net_speech_ms = net_speech_duration_ms(gross_speech_ms, pause_total_ms);

    let net_minutes = net_speech_ms as f32 / 60_000.0;
    let total_minutes = total_duration_ms as f32 / 60_000.0;

    let wpm_phonation = if word_count > 0 && net_minutes > 0.0 {
        Some(word_count as f32 / net_minutes)
    } else {
        None
    };
    let wpm_overall = if word_count > 0 && total_minutes > 0.0 {
        Some(word_count as f32 / total_minutes)
    } else {
        None
    };

    let phonation_ratio = net_speech_ms as f32 / total_duration_ms as f32;
    let pause_measurement_reliable = energy.measurement_reliable;
    let pause_count = pauses.len();
    let mean_pause_ms = if pause_count > 0 {
        pauses
            .iter()
            .map(|p| p.duration_ms)
            .sum::<u64>() as f32
            / pause_count as f32
    } else {
        0.0
    };
    let median_pause_ms = if pause_count > 0 {
        let mut durs: Vec<u64> = pauses.iter().map(|p| p.duration_ms).collect();
        durs.sort_unstable();
        durs[durs.len() / 2] as f32
    } else {
        0.0
    };
    let long_pause_count = pauses
        .iter()
        .filter(|p| p.duration_ms >= config.long_pause_ms)
        .count();
    let classified = classify_pauses(
        &pauses,
        &words,
        config.very_long_pause_ms,
        f0_contour,
        config.long_pause_ms,
    );

    let fillers = load_fillers(language_hint);
    let (filler_total, filler_hits) =
        count_fillers(&words, &fillers, config.filler_top_n, config.min_pause_ms);
    let fillers_per_100 = if word_count > 0 {
        filler_total as f32 * 100.0 / word_count as f32
    } else {
        0.0
    };

    let word_timings: Vec<crate::word_align::WordTiming> = words
        .iter()
        .map(|word| crate::word_align::WordTiming {
            text: word.text.clone(),
            start_ms: word.start_ms,
            end_ms: word.end_ms,
        })
        .collect();
    let repetition_count =
        super::repetitions::count_scored_repetitions(&word_timings, config);

    let report = FluencyReport {
        word_count,
        syllable_count,
        net_speech_duration_ms: net_speech_ms,
        speech_register: SpeechRegister::Spontaneous,
        speech_register_auto: true,
        wpm_overall,
        wpm_phonation,
        phonation_ratio,
        pause_count,
        mean_pause_ms,
        median_pause_ms,
        long_pause_count,
        pause_total_ms,
        min_pause_ms: config.min_pause_ms,
        long_pause_ms: config.long_pause_ms,
        very_long_pause_ms: config.very_long_pause_ms,
        pause_count_punctuation: classified.punctuation,
        pause_count_mid_phrase: classified.mid_phrase,
        very_long_pause_count: classified.very_long,
        pause_measurement_reliable,
        fillers_per_100_words: fillers_per_100,
        filler_hits,
        repetition_count,
    };
    (report, pauses, words)
}

pub fn apply_register_pause_stats(
    fluency: &mut FluencyReport,
    pauses: &[PauseInterval],
    words: &[TimedWord],
    f0_contour: &[F0ContourPoint],
    norms: &RegisterNorms,
    config: &SpeechAnalysisConfig,
) {
    fluency.long_pause_ms = norms.long_pause_ms;
    fluency.long_pause_count = pauses
        .iter()
        .filter(|p| p.duration_ms >= norms.long_pause_ms)
        .count();
    let classified = classify_pauses(
        pauses,
        words,
        config.very_long_pause_ms,
        f0_contour,
        norms.long_pause_ms,
    );
    fluency.pause_count_punctuation = classified.punctuation;
    fluency.pause_count_mid_phrase = classified.mid_phrase;
    fluency.very_long_pause_count = classified.very_long;
}

fn collect_words(timed_segments: &[TimedTextSegment]) -> Vec<TimedWord> {
    timed_segments
        .iter()
        .flat_map(|segment| segment.words.clone())
        .collect()
}

fn is_contextual_filler(
    index: usize,
    words: &[TimedWord],
    token: &str,
    min_pause_ms: u64,
) -> bool {
    if !CONTEXTUAL_FILLERS.contains(&token) {
        return true;
    }
    let gap_before = if index > 0 {
        words[index]
            .start_ms
            .saturating_sub(words[index - 1].end_ms)
    } else {
        min_pause_ms
    };
    let gap_after = words
        .get(index + 1)
        .map(|next| next.start_ms.saturating_sub(words[index].end_ms))
        .unwrap_or(min_pause_ms);
    if gap_before >= min_pause_ms || gap_after >= min_pause_ms {
        return true;
    }
    if token == "да" {
        if let Some(next) = words.get(index + 1) {
            let next_t = normalize_token(&next.text);
            const NOT_FILLER_AFTER_DA: &[&str] = &[
                "так", "и", "что", "чтобы", "же", "ли", "бы", "то", "это", "он", "она", "они",
            ];
            if NOT_FILLER_AFTER_DA.contains(&next_t.as_str()) {
                return false;
            }
        }
    }
    index == 0
}

fn normalize_token(word: &str) -> String {
    word.to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

const CONTEXTUAL_FILLERS: &[&str] = &["да", "ну", "э", "м", "а", "вот"];

fn count_fillers(
    words: &[TimedWord],
    fillers: &[String],
    top_n: usize,
    min_pause_ms: u64,
) -> (usize, Vec<FillerHit>) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut total = 0usize;
    for (index, word) in words.iter().enumerate() {
        let token = normalize_token(&word.text);
        if token.is_empty() {
            continue;
        }
        for filler in fillers {
            if token != normalize_token(filler) {
                continue;
            }
            if !is_contextual_filler(index, words, &token, min_pause_ms) {
                break;
            }
            *counts.entry(filler.clone()).or_default() += 1;
            total += 1;
            break;
        }
    }
    let mut hits: Vec<FillerHit> = counts
        .into_iter()
        .map(|(phrase, count)| FillerHit { phrase, count })
        .collect();
    hits.sort_by(|a, b| b.count.cmp(&a.count));
    hits.truncate(top_n);
    (total, hits)
}

