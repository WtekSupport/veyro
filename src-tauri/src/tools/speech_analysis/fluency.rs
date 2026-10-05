use std::collections::HashMap;

use crate::timed_text::{TimedTextSegment, TimedWord};

use super::config::{load_fillers, SpeechAnalysisConfig};
use super::pause_classify::classify_pauses;
use super::pause_energy::detect_pauses_from_energy;
use super::qc::SpeechSpan;
use super::phonemize::count_syllables_in_words;
use super::types::{FillerHit, FluencyReport};

pub fn analyze_fluency(
    timed_segments: &[TimedTextSegment],
    speech_spans: &[SpeechSpan],
    total_duration_ms: u64,
    language_hint: Option<&str>,
    config: &SpeechAnalysisConfig,
    mono: &[f32],
    sample_rate: u32,
) -> FluencyReport {
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

    let speech_duration_ms = speech_spans
        .iter()
        .map(|(start, end)| end.saturating_sub(*start))
        .sum::<u64>()
        .max(1);
    let total_duration_ms = total_duration_ms.max(1);

    let speech_minutes = speech_duration_ms as f32 / 60_000.0;
    let total_minutes = total_duration_ms as f32 / 60_000.0;

    let wpm_phonation = if word_count > 0 && speech_minutes > 0.0 {
        Some(word_count as f32 / speech_minutes)
    } else {
        None
    };
    let wpm_overall = if word_count > 0 && total_minutes > 0.0 {
        Some(word_count as f32 / total_minutes)
    } else {
        None
    };

    let phonation_ratio = speech_duration_ms as f32 / total_duration_ms as f32;

    let energy = detect_pauses_from_energy(
        mono,
        sample_rate,
        speech_spans,
        total_duration_ms,
        config,
    );
    let pauses = energy.pauses;
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
    let pause_total_ms: u64 = pauses.iter().map(|p| p.duration_ms).sum();
    let long_pause_count = pauses
        .iter()
        .filter(|p| p.duration_ms >= config.long_pause_ms)
        .count();
    let classified = classify_pauses(&pauses, &words, config.very_long_pause_ms);

    let fillers = load_fillers(language_hint);
    let (filler_total, filler_hits) = count_fillers(&words, &fillers, config.filler_top_n);
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

    FluencyReport {
        word_count,
        syllable_count,
        wpm_overall,
        wpm_phonation,
        phonation_ratio,
        pause_count,
        mean_pause_ms,
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
    }
}

fn collect_words(timed_segments: &[TimedTextSegment]) -> Vec<TimedWord> {
    timed_segments
        .iter()
        .flat_map(|segment| segment.words.clone())
        .collect()
}

fn normalize_token(word: &str) -> String {
    word.to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

fn count_fillers(
    words: &[TimedWord],
    fillers: &[String],
    top_n: usize,
) -> (usize, Vec<FillerHit>) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut total = 0usize;
    for word in words {
        let token = normalize_token(&word.text);
        if token.is_empty() {
            continue;
        }
        for filler in fillers {
            if token == normalize_token(filler) {
                *counts.entry(filler.clone()).or_default() += 1;
                total += 1;
                break;
            }
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

