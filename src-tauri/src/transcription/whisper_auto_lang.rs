use std::collections::HashMap;

use whisper_rs::{get_lang_max_id, get_lang_str, WhisperContext};

use crate::audio::segment::AudioSegment;
use crate::transcription::normalize_stt_language_code;
use crate::transcription::provider::TranscriptionError;

const DETECT_SAMPLE_MS: u64 = 30_000;
const DETECT_OFFSETS_MS: [usize; 4] = [0, 5_000, 10_000, 15_000];
const DETECT_THREADS: usize = 2;

/// Ranked Whisper language guesses from several offsets in the first ~30s (auto mode).
pub fn rank_spoken_language_candidates(
    context: &WhisperContext,
    audio: &AudioSegment,
) -> Result<Vec<(String, f32)>, TranscriptionError> {
    let sample = audio.clip_ms(0, DETECT_SAMPLE_MS.min(audio.duration_ms.max(1)));
    if sample.sample_rate == 0 || sample.samples.is_empty() {
        return Ok(Vec::new());
    }
    let min_samples = sample.sample_rate as usize / 2;
    if sample.samples.len() < min_samples {
        return Ok(Vec::new());
    }

    let mut state = context
        .create_state()
        .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?;
    state
        .pcm_to_mel(&sample.samples, DETECT_THREADS)
        .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?;

    let max_id = get_lang_max_id();
    let mut best_by_lang: HashMap<String, f32> = HashMap::new();
    for offset_ms in DETECT_OFFSETS_MS {
        if (offset_ms as u64) >= sample.duration_ms {
            continue;
        }
        let Ok((lang_id, probs)) = state.lang_detect(offset_ms, DETECT_THREADS) else {
            continue;
        };
        if lang_id < 0 || lang_id > max_id {
            continue;
        }
        let Some(code) = get_lang_str(lang_id) else {
            continue;
        };
        let code = normalize_stt_language_code(code);
        let prob = probs.get(lang_id as usize).copied().unwrap_or(0.0);
        best_by_lang
            .entry(code)
            .and_modify(|best| *best = best.max(prob))
            .or_insert(prob);
    }

    let mut ranked: Vec<(String, f32)> = best_by_lang.into_iter().collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(ranked)
}

pub fn best_spoken_language(
    context: &WhisperContext,
    audio: &AudioSegment,
) -> Result<Option<String>, TranscriptionError> {
    Ok(rank_spoken_language_candidates(context, audio)?
        .into_iter()
        .next()
        .map(|(code, _)| code))
}

#[cfg(all(test, feature = "local-whisper"))]
mod tests {
    use super::*;

    #[test]
    fn rank_returns_empty_for_tiny_clip() {
        let ctx_path = std::env::var("VEYRO_TEST_WHISPER_MODEL").ok();
        let Some(path) = ctx_path else {
            return;
        };
        let context = whisper_rs::WhisperContext::new(&path).expect("model");
        let audio = AudioSegment::new(vec![0.0; 100], 16_000, 1);
        assert!(rank_spoken_language_candidates(&context, &audio)
            .unwrap()
            .is_empty());
    }
}
