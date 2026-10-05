use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;
use crate::timed_text::TimedTextSegment;
use super::align::{phoneme_segments_from_words, refine_word_timings};
use super::config::SpeechAnalysisConfig;
use super::ctc::{
    align_char_tokens, decode_with_gigaam_ctc, gigaam_ctc_available, gigaam_ctc_variant,
    preferred_missing_gigaam_ctc_variant, variant_download_hint,
};
use super::gop::{count_below_threshold, gop_from_match, mean_gop};
use super::phonology::alignment_tokens_match;
use super::per::{phoneme_error_rate, top_substitutions};
use super::phonemize::phonemize_text;
use super::problems::aggregate_weak_symbols;
use super::word_gop::word_gop_hits;
use super::types::{ArticulationReport, ReliabilityLevel};

pub async fn analyze_articulation(
    settings: &AppSettings,
    audio: &AudioSegment,
    timed_segments: &[TimedTextSegment],
    transcript: &str,
    language: Option<String>,
    config: &SpeechAnalysisConfig,
) -> ArticulationReport {
    if !gigaam_ctc_available(settings) {
        let missing = preferred_missing_gigaam_ctc_variant();
        return ArticulationReport {
            reliability: ReliabilityLevel::Unavailable,
            unavailable_reason_key: None,
            ctc_variant_used: None,
            missing_ctc_download: Some(variant_download_hint(missing)),
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
    }

    let ctc_variant = gigaam_ctc_variant(settings);
    let ctc_variant_used = ctc_variant.map(|variant| variant.as_api_id());

    let reference_phonemes = phonemize_text(transcript);
    let word_timings = refine_word_timings(audio, timed_segments);
    let mut phoneme_segments = phoneme_segments_from_words(&word_timings);

    let ctc = match decode_with_gigaam_ctc(settings, audio, language).await {
        Ok(outcome) => outcome,
        Err(_) => {
            return ArticulationReport {
                reliability: ReliabilityLevel::Unavailable,
                unavailable_reason_key: Some("tools.speechAnalysis.ctcDecodeFailed".to_string()),
                ctc_variant_used,
                missing_ctc_download: None,
                mean_gop: None,
                per: None,
                low_gop_token_count: 0,
                alignment_token_count: 0,
                letter_baseline_error_rate_percent: None,
                weak_symbols: Vec::new(),
                top_substitutions: Vec::new(),
                phoneme_segments,
                word_gop_hits: Vec::new(),
            };
        }
    };

    let hypothesis_phonemes = phonemize_text(&ctc.hypothesis_text);
    let per = Some(phoneme_error_rate(&reference_phonemes, &hypothesis_phonemes));
    let substitutions = top_substitutions(
        &reference_phonemes,
        &hypothesis_phonemes,
        config.confusion_top_n,
        config.min_substitution_count,
    );

    let char_alignment = align_char_tokens(&reference_phonemes, &hypothesis_phonemes);
    let alignment_token_count = char_alignment.len();
    let (letter_baseline_error_rate_percent, weak_symbols) =
        aggregate_weak_symbols(&char_alignment, config);
    let gop_values: Vec<f32> = char_alignment
        .iter()
        .map(|(exp, hyp, matched)| gop_from_match(alignment_tokens_match(exp, hyp, *matched)))
        .collect();
    let mean = mean_gop(&gop_values);
    let low_gop = count_below_threshold(&gop_values, config.low_gop_threshold);

    if !phoneme_segments.is_empty() && !gop_values.is_empty() {
        for (segment, gop) in phoneme_segments.iter_mut().zip(gop_values.iter()) {
            segment.gop = Some(*gop);
        }
    }

    let word_gop_hits = word_gop_hits(
        transcript,
        &word_timings,
        &char_alignment,
        config,
    );

    ArticulationReport {
        reliability: ReliabilityLevel::Medium,
        unavailable_reason_key: None,
        ctc_variant_used,
        missing_ctc_download: None,
        mean_gop: mean,
        per,
        low_gop_token_count: low_gop,
        alignment_token_count,
        letter_baseline_error_rate_percent,
        weak_symbols,
        top_substitutions: substitutions,
        phoneme_segments,
        word_gop_hits,
    }
}
