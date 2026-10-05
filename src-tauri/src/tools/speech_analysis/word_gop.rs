use crate::word_align::WordTiming;

use super::gop::{gop_from_match, mean_gop};
use super::phonemize::phonemize_word;
use super::phonology::alignment_tokens_match;
use super::types::WordGopHit;

/// Maps char-level alignment (reference = transcript graphemes) to per-word GOP proxies.
pub fn word_gop_hits(
    transcript: &str,
    word_timings: &[WordTiming],
    char_alignment: &[(String, String, bool)],
    config: &super::config::SpeechAnalysisConfig,
) -> Vec<WordGopHit> {
    let mut ranges: Vec<(String, usize, usize)> = Vec::new();
    let mut offset = 0usize;
    for word in transcript.split_whitespace() {
        let phones = phonemize_word(word);
        if phones.is_empty() {
            continue;
        }
        ranges.push((word.to_string(), offset, offset + phones.len()));
        offset += phones.len();
    }
    if ranges.is_empty() || char_alignment.is_empty() {
        return Vec::new();
    }

    let timing_by_norm: std::collections::HashMap<String, (u64, u64)> = word_timings
        .iter()
        .map(|w| {
            let key = w.text.to_lowercase();
            (key, (w.start_ms, w.end_ms))
        })
        .collect();

    let mut out = Vec::new();
    for (word, start_idx, end_idx) in ranges {
        let slice = char_alignment.get(start_idx..end_idx.min(char_alignment.len()));
        let Some(slice) = slice else {
            continue;
        };
        if slice.is_empty() {
            continue;
        }
        let gops: Vec<f32> = slice
            .iter()
            .map(|(exp, hyp, matched)| {
                gop_from_match(alignment_tokens_match(exp, hyp, *matched))
            })
            .collect();
        let Some(mean) = mean_gop(&gops) else {
            continue;
        };
        if mean >= config.low_gop_threshold {
            continue;
        }
        let norm = word.to_lowercase();
        let (start_ms, end_ms) = timing_by_norm
            .get(&norm)
            .copied()
            .unwrap_or((0, 0));
        out.push(WordGopHit {
            text: word,
            start_ms,
            end_ms,
            gop: mean,
        });
    }
    out.sort_by(|a, b| a.gop.partial_cmp(&b.gop).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(16);
    out
}
