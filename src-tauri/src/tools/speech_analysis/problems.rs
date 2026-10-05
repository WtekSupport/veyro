use std::collections::HashMap;

use crate::word_align::WordTiming;

use super::config::SpeechAnalysisConfig;
use super::phonology::alignment_tokens_match;
use super::types::{
    ArticulationReport, FluencyReport, IntelligibilityReport, ProblemWord, SpeechAnalysisProblems,
    WeakSymbolSummary,
};

fn is_reportable_symbol(symbol: &str) -> bool {
    let trimmed = symbol.trim();
    !trimmed.is_empty() && trimmed.chars().any(|c| c.is_alphanumeric())
}

pub fn aggregate_weak_symbols(
    char_alignment: &[(String, String, bool)],
    config: &SpeechAnalysisConfig,
) -> (Option<f32>, Vec<WeakSymbolSummary>) {
    if char_alignment.is_empty() {
        return (None, Vec::new());
    }
    let mut by_symbol: HashMap<String, (usize, usize)> = HashMap::new();
    let mut total_errors = 0usize;
    let mut total = 0usize;
    for (expected, observed, matched) in char_alignment {
        if !is_reportable_symbol(expected) {
            continue;
        }
        total += 1;
        let ok = alignment_tokens_match(expected, observed, *matched);
        let entry = by_symbol.entry(expected.clone()).or_insert((0, 0));
        entry.0 += 1;
        if !ok {
            entry.1 += 1;
            total_errors += 1;
        }
    }
    if total == 0 {
        return (None, Vec::new());
    }
    let baseline = (total_errors as f32 / total as f32) * 100.0;
    let mut weak: Vec<WeakSymbolSummary> = by_symbol
        .into_iter()
        .map(|(symbol, (token_count, error_count))| {
            let error_rate = (error_count as f32 / token_count.max(1) as f32) * 100.0;
            let excess = error_rate - baseline;
            WeakSymbolSummary {
                low_sample: token_count < config.min_weak_symbol_samples,
                symbol,
                token_count,
                error_count,
                error_rate_percent: error_rate,
                excess_vs_baseline_percent: excess,
            }
        })
        .filter(|row| {
            !row.low_sample
                && row.excess_vs_baseline_percent >= config.weak_symbol_baseline_margin_pp
        })
        .collect();
    weak.sort_by(|a, b| {
        b.excess_vs_baseline_percent
            .partial_cmp(&a.excess_vs_baseline_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    weak.truncate(config.weak_symbol_top_n);
    (Some(baseline), weak)
}

pub fn build_problems(
    transcript: &str,
    fluency: &FluencyReport,
    intelligibility: &IntelligibilityReport,
    articulation: &ArticulationReport,
    word_timings: &[WordTiming],
    config: &SpeechAnalysisConfig,
    language_ru: bool,
    transcript_degraded: bool,
    extra_vocabulary: &[String],
) -> SpeechAnalysisProblems {
    let context_by_start: HashMap<u64, String> = word_timings
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let ctx = word_timings
                .iter()
                .skip(index.saturating_sub(1))
                .take(3)
                .map(|w| normalize_display_token(&w.text))
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            (word.start_ms, ctx)
        })
        .collect();

    let mut problem_words = Vec::new();
    for word in &intelligibility.low_confidence_words {
        if !should_surface_low_confidence(&word.text, word.confidence) {
            continue;
        }
        problem_words.push(ProblemWord {
            text: normalize_display_token(&word.text),
            start_ms: word.start_ms,
            end_ms: word.end_ms,
            reason_key: "tools.speechAnalysis.problems.lowConfidence".to_string(),
            score_hint: Some(word.confidence),
            context: context_by_start.get(&word.start_ms).cloned(),
        });
    }
    for word in word_timings {
        let token = normalize_display_token(&word.text);
        if token.len() < 2 {
            continue;
        }
        if super::transcript_qc::is_suspicious_word(&word.text, language_ru, extra_vocabulary) {
            if problem_words.iter().any(|p| p.start_ms == word.start_ms) {
                continue;
            }
            problem_words.push(ProblemWord {
                text: token,
                start_ms: word.start_ms,
                end_ms: word.end_ms,
                reason_key: "tools.speechAnalysis.problems.suspiciousWord".to_string(),
                score_hint: None,
                context: context_by_start.get(&word.start_ms).cloned(),
            });
        }
    }
    for hit in &articulation.word_gop_hits {
        if problem_words.iter().any(|p| p.start_ms == hit.start_ms) {
            continue;
        }
        problem_words.push(ProblemWord {
            text: hit.text.clone(),
            start_ms: hit.start_ms,
            end_ms: hit.end_ms,
            reason_key: "tools.speechAnalysis.problems.lowGop".to_string(),
            score_hint: Some(hit.gop),
            context: None,
        });
    }
    problem_words.truncate(16);

    let source_key = if transcript_degraded {
        "tools.speechAnalysis.problems.transcriptQualitySource".to_string()
    } else if articulation.ctc_variant_used.is_some() {
        "tools.speechAnalysis.problems.uncertaintySource".to_string()
    } else {
        "tools.speechAnalysis.problems.sttUncertaintySource".to_string()
    };

    let mut repetition_examples: Vec<_> = super::repetitions::find_repetition_examples(
        word_timings,
        transcript,
        config,
        8,
    )
    .into_iter()
    .map(|row| super::types::RepetitionExample {
        token: row.token,
        context: row.context,
        start_ms: row.start_ms,
        end_ms: row.end_ms,
        kind: row.kind,
    })
    .collect();

    for phrase in super::transcript_qc::analyze_transcript_quality(
        transcript,
        language_ru,
        extra_vocabulary,
    )
    .duplicate_ngram_examples
    {
        repetition_examples.push(super::types::RepetitionExample {
            token: phrase.clone(),
            context: phrase,
            start_ms: 0,
            end_ms: 0,
            kind: super::types::RepetitionKind::PossibleDeliberate,
        });
    }
    repetition_examples.truncate(8);

    let repetition_count = fluency.repetition_count;

    SpeechAnalysisProblems {
        source_key,
        problem_words,
        weak_symbols: Vec::new(),
        substitutions: Vec::new(),
        repetition_count,
        repetition_examples,
    }
}

fn normalize_display_token(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c: char| c == ',' || c == '.' || c == '!' || c == '?' || c == ';')
        .to_string()
}

fn should_surface_low_confidence(text: &str, confidence: f32) -> bool {
    let token = normalize_display_token(text);
    if token.len() < 4 {
        return false;
    }
    if is_function_word(&token) {
        return false;
    }
    if looks_like_proper_name(text) {
        return false;
    }
    if confidence >= 0.55 {
        return false;
    }
    if (confidence - 0.5).abs() < 0.011 && token.len() < 6 {
        return false;
    }
    if confidence <= 0.001 {
        return false;
    }
    if confidence <= 0.01 && token.len() < 5 {
        return false;
    }
    true
}

fn is_function_word(token: &str) -> bool {
    const RU: &[&str] = &[
        "не", "ты", "вы", "мы", "он", "она", "они", "я", "и", "в", "на", "за", "по", "от", "до",
        "из", "у", "к", "с", "о", "а", "но", "же", "ли", "бы", "то", "это", "как", "что", "где",
        "when", "the", "and", "or", "to", "of", "in", "on", "at", "is", "it", "you", "we", "they",
    ];
    let lower = token.to_lowercase();
    RU.iter().any(|w| *w == lower)
}

fn looks_like_proper_name(raw: &str) -> bool {
    let trimmed = raw.trim();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_uppercase() {
        return false;
    }
    trimmed.len() >= 3 && trimmed.chars().skip(1).all(|c| c.is_lowercase() || c == '-')
}
