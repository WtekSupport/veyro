use crate::audio::segment::AudioSegment;

use super::config::EnergyAlignConfig;
use super::normalize::segment_words_for_alignment;
use super::types::{CoarseSegment, WordTiming};

const FRAME_MS: u64 = 10;
const HOP_MS: u64 = 10;
const SMOOTH_WINDOW: usize = 5;
/// Treat gaps at least this wide as pauses (ms).
const MIN_ACOUSTIC_PAUSE_MS: u64 = 45;
const GLUE_SEARCH_AHEAD_MS: u64 = 2_500;
/// Minimum duration to treat an energy blob as speech (ms).
const MIN_SPEECH_RUN_MS: u64 = 100;
/// Silence at least this long ends one speech cluster / run (ms).
const SPEECH_CLUSTER_GAP_MS: u64 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpeechRun {
    start_ms: u64,
    end_ms: u64,
}

#[derive(Clone, Copy, Debug)]
struct SpeechGateLevels {
    /// Energy must reach this to enter / stay in strong speech.
    high: f32,
    /// Energy below this ends speech (hysteresis — lower than `high`).
    low: f32,
}

struct EnvelopeTrack {
    smoothed: Vec<f32>,
    hop_ms: u64,
    origin_ms: u64,
    gate_ratio: f32,
}

impl EnvelopeTrack {
    fn from_audio(
        audio: &AudioSegment,
        start_ms: u64,
        end_ms: u64,
        gate_ratio: f32,
    ) -> Self {
        let clip = extract_audio_ms(audio, start_ms, end_ms);
        let envelope = compute_log_rms_envelope(&clip, audio.sample_rate, FRAME_MS, HOP_MS);
        let smoothed = moving_average(&envelope, SMOOTH_WINDOW);
        Self {
            smoothed,
            hop_ms: HOP_MS,
            origin_ms: start_ms,
            gate_ratio: gate_ratio.clamp(0.05, 0.95),
        }
    }

    fn from_full_audio(audio: &AudioSegment, gate_ratio: f32) -> Self {
        Self::from_audio(audio, 0, audio.duration_ms.max(1), gate_ratio)
    }

    fn gate_levels(&self) -> SpeechGateLevels {
        speech_gate_levels(&self.smoothed, self.gate_ratio)
    }

    fn threshold(&self) -> f32 {
        self.gate_levels().high
    }

    fn frame_to_ms(&self, frame: usize) -> u64 {
        self.origin_ms.saturating_add(frame as u64 * self.hop_ms)
    }

    fn ms_to_frame(&self, ms: u64) -> usize {
        if ms <= self.origin_ms {
            return 0;
        }
        ((ms - self.origin_ms) / self.hop_ms) as usize
    }

    fn longest_silence_between(&self, from_ms: u64, to_ms: u64, min_pause_ms: u64) -> Option<(u64, u64)> {
        if self.smoothed.is_empty() || to_ms <= from_ms {
            return None;
        }

        let frame_lo = self.ms_to_frame(from_ms).min(self.smoothed.len().saturating_sub(1));
        let frame_hi = self
            .ms_to_frame(to_ms)
            .min(self.smoothed.len().saturating_sub(1));
        if frame_lo >= frame_hi {
            return None;
        }

        let mut best: Option<(usize, usize)> = None;
        let mut run_start: Option<usize> = None;

        let low = self.gate_levels().low;
        for frame in frame_lo..=frame_hi {
            let silent = self.smoothed[frame] < low;
            if silent {
                if run_start.is_none() {
                    run_start = Some(frame);
                }
            } else if let Some(start) = run_start {
                let end = frame.saturating_sub(1);
                if best.map(|(b_lo, b_hi)| end.saturating_sub(start) > b_hi.saturating_sub(b_lo)).unwrap_or(true)
                {
                    best = Some((start, end));
                }
                run_start = None;
            }
        }
        if let Some(start) = run_start {
            let end = frame_hi;
            if best
                .map(|(b_lo, b_hi)| end.saturating_sub(start) > b_hi.saturating_sub(b_lo))
                .unwrap_or(true)
            {
                best = Some((start, end));
            }
        }

        let (start_frame, end_frame) = best?;
        let duration_ms = (end_frame.saturating_sub(start_frame) as u64 + 1) * self.hop_ms;
        if duration_ms < min_pause_ms {
            return None;
        }

        let silence_start = self.frame_to_ms(start_frame);
        let silence_end = self.frame_to_ms(end_frame.saturating_add(1));
        Some((silence_start, silence_end.min(to_ms)))
    }

    fn next_speech_onset_after(&self, after_ms: u64, limit_ms: u64) -> Option<u64> {
        if self.smoothed.is_empty() {
            return None;
        }
        let threshold = self.threshold();
        let start = self.ms_to_frame(after_ms);
        let end = self
            .ms_to_frame(limit_ms)
            .min(self.smoothed.len().saturating_sub(1));
        for frame in start..=end {
            if self.smoothed[frame] >= threshold {
                return Some(self.frame_to_ms(frame));
            }
        }
        None
    }

    fn detect_speech_runs(&self) -> Vec<SpeechRun> {
        speech_clusters_to_runs(
            &self.smoothed,
            self.gate_ratio,
            self.origin_ms,
            self.hop_ms,
        )
    }

    fn prev_speech_offset_before(&self, before_ms: u64, limit_ms: u64) -> Option<u64> {
        if self.smoothed.is_empty() {
            return None;
        }
        let threshold = self.threshold();
        let end = self.ms_to_frame(before_ms).min(self.smoothed.len().saturating_sub(1));
        let start = self.ms_to_frame(limit_ms);
        for frame in (start..=end).rev() {
            if self.smoothed[frame] >= threshold {
                return Some(self.frame_to_ms(frame).saturating_add(FRAME_MS));
            }
        }
        None
    }
}

pub fn refine_via_energy(
    audio: &AudioSegment,
    segment: &CoarseSegment,
    words: &[String],
    gate_ratio: f32,
) -> Vec<WordTiming> {
    if words.is_empty() {
        return Vec::new();
    }

    let clip = extract_audio_ms(audio, segment.start_ms, segment.end_ms);
    if clip.is_empty() {
        return fallback_word_timings(words, segment.start_ms, segment.end_ms);
    }

    let envelope = compute_log_rms_envelope(&clip, audio.sample_rate, FRAME_MS, HOP_MS);
    if envelope.is_empty() {
        return fallback_word_timings(words, segment.start_ms, segment.end_ms);
    }

    let smoothed = moving_average(&envelope, SMOOTH_WINDOW);

    let gate = gate_ratio.clamp(0.05, 0.95);
    let levels = speech_gate_levels(&smoothed, gate);

    if words.len() == 1 {
        return vec![align_single_word_to_envelope(
            &words[0],
            segment.start_ms,
            &smoothed,
            HOP_MS,
            gate,
        )];
    }

    let clusters = speech_frame_clusters(&smoothed, levels, min_silence_frames(HOP_MS));
    if clusters.len() >= words.len() {
        return words
            .iter()
            .enumerate()
            .map(|(index, word)| {
                let (start, end) = clusters[index];
                word_timing_from_cluster_frames(word, segment.start_ms, start, end, HOP_MS)
            })
            .collect();
    }

    let boundaries = boundary_frame_indices(&smoothed, words.len());
    let mut timings = assign_word_timings(
        words,
        segment.start_ms,
        &boundaries,
        &smoothed,
        HOP_MS,
        gate,
    );
    split_words_at_boundaries(
        &mut timings,
        segment.start_ms,
        &boundaries,
        &smoothed,
        HOP_MS,
        gate,
    );
    timings
}

fn extract_audio_ms(audio: &AudioSegment, start_ms: u64, end_ms: u64) -> Vec<f32> {
    if audio.sample_rate == 0 || audio.samples.is_empty() {
        return Vec::new();
    }
    let rate = audio.sample_rate as u64;
    let start_sample = (start_ms.saturating_mul(rate) / 1000) as usize;
    let end_sample = (end_ms.saturating_mul(rate) / 1000) as usize;
    let end_sample = end_sample.min(audio.samples.len());
    if start_sample >= end_sample {
        return Vec::new();
    }
    audio.samples[start_sample..end_sample].to_vec()
}

fn compute_log_rms_envelope(
    samples: &[f32],
    sample_rate: u32,
    frame_ms: u64,
    hop_ms: u64,
) -> Vec<f32> {
    let frame_samples = ((sample_rate as u64 * frame_ms) / 1000).max(1) as usize;
    let hop_samples = ((sample_rate as u64 * hop_ms) / 1000).max(1) as usize;
    let mut envelope = Vec::new();
    let mut offset = 0usize;
    while offset + frame_samples <= samples.len() {
        let frame = &samples[offset..offset + frame_samples];
        let mean_sq = frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32;
        envelope.push(mean_sq.sqrt().max(1e-8).ln());
        offset += hop_samples;
    }
    envelope
}

fn moving_average(values: &[f32], window: usize) -> Vec<f32> {
    if values.is_empty() {
        return Vec::new();
    }
    let window = window.max(1);
    values
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let start = index.saturating_sub(window / 2);
            let end = (index + window / 2 + 1).min(values.len());
            values[start..end].iter().sum::<f32>() / (end - start) as f32
        })
        .collect()
}

fn envelope_percentile(values: &[f32], quantile: f32) -> f32 {
    let mut sorted: Vec<f32> = values.iter().copied().filter(|value| value.is_finite()).collect();
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let quantile = quantile.clamp(0.0, 1.0);
    let index = ((sorted.len() - 1) as f32 * quantile).round() as usize;
    sorted[index]
}

/// Noise floor from quiet portions + user gate; hysteresis separates speech from room noise.
fn speech_gate_levels(smoothed: &[f32], gate_ratio: f32) -> SpeechGateLevels {
    let floor = envelope_percentile(smoothed, 0.10);
    let ceiling = envelope_percentile(smoothed, 0.88);
    let span = (ceiling - floor).max(1e-4);
    let gr = gate_ratio.clamp(0.05, 0.95);
    let high = floor + span * gr;
    let low = floor + span * (gr * 0.42).min(0.30);
    SpeechGateLevels {
        high: high.min(ceiling),
        low: low.max(floor),
    }
}

fn speech_threshold(smoothed: &[f32], gate_ratio: f32) -> f32 {
    speech_gate_levels(smoothed, gate_ratio).high
}

fn frame_to_ms(segment_start_ms: u64, frame: usize, hop_ms: u64) -> u64 {
    segment_start_ms.saturating_add(frame as u64 * hop_ms)
}

fn first_speech_frame(smoothed: &[f32], lo: usize, hi: usize, threshold: f32) -> Option<usize> {
    if smoothed.is_empty() {
        return None;
    }
    let hi = hi.min(smoothed.len() - 1);
    if lo > hi {
        return None;
    }
    (lo..=hi).find(|&index| smoothed[index] >= threshold)
}

fn min_silence_frames(hop_ms: u64) -> usize {
    (SPEECH_CLUSTER_GAP_MS / hop_ms.max(1)).max(1) as usize
}

/// Contiguous speech blobs; enter above `gate.high`, leave below `gate.low` for `min_silence_frames`.
fn speech_frame_clusters(
    smoothed: &[f32],
    gate: SpeechGateLevels,
    min_silence_frames: usize,
) -> Vec<(usize, usize)> {
    if smoothed.is_empty() {
        return Vec::new();
    }

    let mut clusters = Vec::new();
    let mut in_speech = false;
    let mut run_start = 0usize;
    let mut silence_frames = 0usize;

    for (frame, &value) in smoothed.iter().enumerate() {
        if !in_speech {
            if value >= gate.high {
                run_start = frame;
                in_speech = true;
                silence_frames = 0;
            }
            continue;
        }

        if value < gate.low {
            silence_frames += 1;
            if silence_frames >= min_silence_frames {
                let speech_end = frame.saturating_sub(silence_frames);
                let last =
                    last_speech_frame(smoothed, run_start, speech_end, gate.high).unwrap_or(speech_end);
                if last >= run_start {
                    clusters.push((run_start, last));
                }
                in_speech = false;
                silence_frames = 0;
            }
        } else {
            silence_frames = 0;
        }
    }

    if in_speech {
        let last_frame = smoothed.len().saturating_sub(1);
        let last =
            last_speech_frame(smoothed, run_start, last_frame, gate.high).unwrap_or(last_frame);
        if last >= run_start {
            clusters.push((run_start, last));
        }
    }

    clusters
        .into_iter()
        .filter(|(start, end)| {
            (end.saturating_sub(*start) as u64 + 1) * HOP_MS >= MIN_SPEECH_RUN_MS
        })
        .collect()
}

const MAX_CLUSTER_MS_FOR_WORD_ASSIGN: u64 = 900;
const MIN_WORD_DURATION_MS: u64 = 55;

fn merge_nearby_clusters(
    clusters: Vec<(usize, usize)>,
    hop_ms: u64,
    merge_gap_ms: u64,
) -> Vec<(usize, usize)> {
    if clusters.is_empty() {
        return clusters;
    }

    let mut merged = Vec::new();
    let mut current = clusters[0];
    for (lo, hi) in clusters.into_iter().skip(1) {
        let gap_ms = lo.saturating_sub(current.1) as u64 * hop_ms;
        if gap_ms < merge_gap_ms {
            current.1 = hi;
        } else {
            merged.push(current);
            current = (lo, hi);
        }
    }
    merged.push(current);
    merged
}

fn split_cluster_at_valley(
    smoothed: &[f32],
    lo: usize,
    hi: usize,
    gate: SpeechGateLevels,
) -> Option<(usize, usize)> {
    if hi <= lo.saturating_add(6) {
        return None;
    }

    let mut valley_frame = lo + 1;
    let mut valley_value = smoothed[lo + 1];
    for frame in lo + 2..hi.saturating_sub(1) {
        if smoothed[frame] < valley_value {
            valley_value = smoothed[frame];
            valley_frame = frame;
        }
    }

    if valley_value >= gate.low {
        return None;
    }

    let left_last = last_speech_frame(smoothed, lo, valley_frame, gate.high)?;
    let right_first = first_speech_frame(smoothed, valley_frame, hi, gate.high)?;
    if left_last >= right_first {
        return None;
    }

    Some((left_last, right_first))
}

fn split_oversized_clusters(
    smoothed: &[f32],
    gate_ratio: f32,
    hop_ms: u64,
    max_cluster_ms: u64,
) -> Vec<(usize, usize)> {
    let gate = speech_gate_levels(smoothed, gate_ratio);
    let min_silence = min_silence_frames(hop_ms);
    let clusters = speech_frame_clusters(smoothed, gate, min_silence);
    let mut refined = Vec::new();

    for (lo, hi) in clusters {
        let duration_ms = (hi.saturating_sub(lo) as u64 + 1) * hop_ms;
        if duration_ms <= max_cluster_ms {
            refined.push((lo, hi));
            continue;
        }

        let sub = &smoothed[lo..=hi.min(smoothed.len().saturating_sub(1))];
        let inner = speech_frame_clusters(sub, gate, min_silence);
        if inner.len() > 1 {
            for (sub_lo, sub_hi) in inner {
                refined.push((lo + sub_lo, lo + sub_hi));
            }
            continue;
        }

        if let Some((left_last, right_first)) = split_cluster_at_valley(smoothed, lo, hi, gate) {
            refined.push((lo, left_last));
            refined.push((right_first, hi));
        } else {
            refined.push((lo, hi));
        }
    }

    refined.sort_by_key(|(lo, _)| *lo);
    refined
}

fn speech_clusters_to_runs(
    smoothed: &[f32],
    gate_ratio: f32,
    origin_ms: u64,
    hop_ms: u64,
) -> Vec<SpeechRun> {
    let gate = speech_gate_levels(smoothed, gate_ratio);
    let clusters = speech_frame_clusters(smoothed, gate, min_silence_frames(hop_ms));
    clusters
        .into_iter()
        .map(|(start, end)| SpeechRun {
            start_ms: frame_to_ms(origin_ms, start, hop_ms),
            end_ms: frame_to_ms(origin_ms, end, hop_ms).saturating_add(FRAME_MS),
        })
        .collect()
}

fn word_timing_from_cluster_frames(
    word: &str,
    segment_start_ms: u64,
    start_frame: usize,
    end_frame: usize,
    hop_ms: u64,
) -> WordTiming {
    let start_ms = frame_to_ms(segment_start_ms, start_frame, hop_ms);
    let end_ms = frame_to_ms(segment_start_ms, end_frame, hop_ms).saturating_add(FRAME_MS);
    WordTiming {
        text: word.to_string(),
        start_ms,
        end_ms: end_ms.max(start_ms),
    }
}

fn last_speech_frame(smoothed: &[f32], lo: usize, hi: usize, threshold: f32) -> Option<usize> {
    if smoothed.is_empty() {
        return None;
    }
    let hi = hi.min(smoothed.len() - 1);
    if lo > hi {
        return None;
    }
    (lo..=hi).rfind(|&index| smoothed[index] >= threshold)
}

fn align_single_word_to_envelope(
    word: &str,
    segment_start_ms: u64,
    smoothed: &[f32],
    hop_ms: u64,
    gate_ratio: f32,
) -> WordTiming {
    let levels = speech_gate_levels(smoothed, gate_ratio);
    let clusters = speech_frame_clusters(smoothed, levels, min_silence_frames(hop_ms));
    if let Some((start, end)) = clusters.first() {
        return word_timing_from_cluster_frames(word, segment_start_ms, *start, *end, hop_ms);
    }

    let last_frame = smoothed.len().saturating_sub(1);
    if let (Some(first), Some(last)) = (
        first_speech_frame(smoothed, 0, last_frame, levels.high),
        last_speech_frame(smoothed, 0, last_frame, levels.high),
    ) {
        return word_timing_from_cluster_frames(word, segment_start_ms, first, last, hop_ms);
    }

    WordTiming {
        text: word.to_string(),
        start_ms: segment_start_ms,
        end_ms: segment_start_ms.saturating_add(FRAME_MS),
    }
}

#[derive(Clone, Copy)]
struct Valley {
    index: usize,
    prominence: f32,
}

fn boundary_frame_indices(smoothed: &[f32], word_count: usize) -> Vec<usize> {
    let gaps_needed = word_count.saturating_sub(1);
    if gaps_needed == 0 {
        return Vec::new();
    }

    let min_val = smoothed.iter().copied().fold(f32::INFINITY, f32::min);
    let max_val = smoothed.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let dynamic_range = (max_val - min_val).max(1e-6);
    let min_prominence = dynamic_range * 0.04;

    let mut valleys = Vec::new();
    for index in 1..smoothed.len().saturating_sub(1) {
        let value = smoothed[index];
        if value <= smoothed[index - 1] && value <= smoothed[index + 1] {
            let left_peak = smoothed[..index].iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let right_peak = smoothed[index + 1..]
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            let prominence = (left_peak.min(right_peak) - value).max(0.0);
            if prominence >= min_prominence {
                valleys.push(Valley { index, prominence });
            }
        }
    }

    if valleys.len() >= gaps_needed {
        valleys.sort_by(|left, right| {
            right
                .prominence
                .partial_cmp(&left.prominence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut chosen: Vec<usize> = valleys.into_iter().take(gaps_needed).map(|v| v.index).collect();
        chosen.sort_unstable();
        return enforce_increasing_boundaries(chosen, smoothed.len());
    }

    boundaries_at_energy_minima(smoothed, word_count)
}

fn boundaries_at_energy_minima(smoothed: &[f32], word_count: usize) -> Vec<usize> {
    let gaps = word_count.saturating_sub(1);
    if gaps == 0 || smoothed.is_empty() {
        return Vec::new();
    }

    let len = smoothed.len();
    let mut boundaries = Vec::with_capacity(gaps);
    for gap_index in 1..=gaps {
        let center = gap_index * len / (gaps + 1);
        let radius = (len / (gaps + 1) / 2).max(2);
        let lo = center.saturating_sub(radius);
        let hi = (center + radius).min(len.saturating_sub(1));

        let mut best = lo;
        let mut best_value = smoothed[lo];
        for frame in lo..=hi {
            if smoothed[frame] < best_value {
                best_value = smoothed[frame];
                best = frame;
            }
        }
        boundaries.push(best);
    }

    enforce_increasing_boundaries(boundaries, len)
}

fn enforce_increasing_boundaries(mut boundaries: Vec<usize>, len: usize) -> Vec<usize> {
    if boundaries.is_empty() {
        return boundaries;
    }
    let last = len.saturating_sub(1);
    for index in 1..boundaries.len() {
        if boundaries[index] <= boundaries[index - 1] {
            boundaries[index] = (boundaries[index - 1] + 1).min(last);
        }
    }
    boundaries
}

fn assign_word_timings(
    words: &[String],
    segment_start_ms: u64,
    boundary_frames: &[usize],
    smoothed: &[f32],
    hop_ms: u64,
    gate_ratio: f32,
) -> Vec<WordTiming> {
    let word_count = words.len();
    let gap_count = word_count.saturating_sub(1);
    let last_frame = smoothed.len().saturating_sub(1);
    let threshold = speech_threshold(smoothed, gate_ratio);

    let boundary_frames = if boundary_frames.len() == gap_count {
        boundary_frames.to_vec()
    } else {
        boundaries_at_energy_minima(smoothed, word_count)
    };

    let mut out = Vec::with_capacity(word_count);

    for (index, word) in words.iter().enumerate() {
        let frame_lo = if index == 0 {
            0
        } else {
            boundary_frames[index - 1].saturating_add(1).min(last_frame)
        };
        let frame_hi = if index + 1 == word_count {
            last_frame
        } else {
            boundary_frames[index].min(last_frame)
        };

        let (start_ms, end_ms) = if let (Some(first), Some(last)) = (
            first_speech_frame(smoothed, frame_lo, frame_hi, threshold),
            last_speech_frame(smoothed, frame_lo, frame_hi, threshold),
        ) {
            (
                frame_to_ms(segment_start_ms, first, hop_ms),
                frame_to_ms(segment_start_ms, last, hop_ms).saturating_add(FRAME_MS),
            )
        } else {
            (
                frame_to_ms(segment_start_ms, frame_lo, hop_ms),
                frame_to_ms(segment_start_ms, frame_hi, hop_ms).saturating_add(FRAME_MS),
            )
        };

        out.push(WordTiming {
            text: word.clone(),
            start_ms,
            end_ms: end_ms.max(start_ms),
        });
    }

    out
}

fn split_words_at_boundaries(
    words: &mut [WordTiming],
    segment_start_ms: u64,
    boundary_frames: &[usize],
    smoothed: &[f32],
    hop_ms: u64,
    gate_ratio: f32,
) {
    if words.len() < 2 || boundary_frames.is_empty() {
        return;
    }

    let threshold = speech_threshold(smoothed, gate_ratio);
    let last_frame = smoothed.len().saturating_sub(1);

    for (gap_index, boundary) in boundary_frames.iter().enumerate() {
        if gap_index + 1 >= words.len() {
            break;
        }

        let b = (*boundary).min(last_frame);
        let search_lo = b.saturating_sub(8);
        let search_hi = (b.saturating_add(8)).min(last_frame);

        if let (Some(last_before), Some(first_after)) = (
            last_speech_frame(smoothed, search_lo, b, threshold),
            first_speech_frame(smoothed, b.saturating_add(1), search_hi, threshold),
        ) {
            let end_ms = frame_to_ms(segment_start_ms, last_before, hop_ms).saturating_add(FRAME_MS);
            let start_ms = frame_to_ms(segment_start_ms, first_after, hop_ms);
            if start_ms > end_ms {
                words[gap_index].end_ms = end_ms.max(words[gap_index].start_ms);
                words[gap_index + 1].start_ms = start_ms;
            }
        }
    }
}

fn fallback_word_timings(words: &[String], start_ms: u64, end_ms: u64) -> Vec<WordTiming> {
    if words.len() == 1 {
        return vec![WordTiming {
            text: words[0].clone(),
            start_ms,
            end_ms: start_ms.saturating_add(FRAME_MS),
        }];
    }

    let duration = end_ms.saturating_sub(start_ms).max(1);
    let slot = duration / words.len() as u64;
    words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let slot_start = start_ms + slot * index as u64;
            let slot_end = if index + 1 == words.len() {
                slot_start.saturating_add(FRAME_MS)
            } else {
                slot_start + slot.saturating_sub(MIN_ACOUSTIC_PAUSE_MS.max(1))
            };
            WordTiming {
                text: word.clone(),
                start_ms: slot_start,
                end_ms: slot_end.max(slot_start),
            }
        })
        .collect()
}

fn open_gaps_between_words(words: &mut [WordTiming], track: &EnvelopeTrack) {
    if words.len() < 2 {
        return;
    }

    for index in 0..words.len().saturating_sub(1) {
        let gap = words[index + 1]
            .start_ms
            .saturating_sub(words[index].end_ms);
        let glued = gap < MIN_ACOUSTIC_PAUSE_MS;

        let search_lo = words[index].end_ms.saturating_sub(20);
        let search_hi = if glued {
            words[index]
                .end_ms
                .saturating_add(GLUE_SEARCH_AHEAD_MS)
                .max(words[index + 1].start_ms.saturating_add(50))
        } else {
            words[index + 1].start_ms.saturating_add(20)
        };

        if let Some((silence_start, silence_end)) =
            track.longest_silence_between(search_lo, search_hi, MIN_ACOUSTIC_PAUSE_MS)
        {
            words[index].end_ms = words[index]
                .end_ms
                .min(silence_start)
                .max(words[index].start_ms);
            words[index + 1].start_ms = words[index + 1].start_ms.max(silence_end);
            continue;
        }

        if glued {
            if let Some(onset) = track.next_speech_onset_after(
                words[index].end_ms.saturating_add(10),
                words[index].end_ms.saturating_add(GLUE_SEARCH_AHEAD_MS),
            ) {
                if let Some(offset) = track.prev_speech_offset_before(
                    onset.saturating_sub(5),
                    words[index].start_ms,
                ) {
                    words[index].end_ms = offset.max(words[index].start_ms);
                }
                words[index + 1].start_ms = onset;
            }
        }
    }
}

fn whisper_timeline_mismatch(
    segments: &[CoarseSegment],
    runs: &[SpeechRun],
    audio_duration_ms: u64,
) -> bool {
    if segments.is_empty() || runs.is_empty() {
        return false;
    }

    let whisper_start = segments.first().map(|s| s.start_ms).unwrap_or(0);
    let whisper_end = segments.last().map(|s| s.end_ms).unwrap_or(0);
    let whisper_span = whisper_end.saturating_sub(whisper_start);
    let first_run = runs.first().map(|r| r.start_ms).unwrap_or(0);
    let last_run = runs.last().map(|r| r.end_ms).unwrap_or(0);
    let acoustic_span = last_run.saturating_sub(first_run);

    if last_run > whisper_end.saturating_add(600)
        && whisper_end.saturating_add(600) < audio_duration_ms.saturating_mul(65) / 100
    {
        return true;
    }

    acoustic_span > whisper_span.saturating_mul(2)
        && whisper_span < audio_duration_ms.saturating_mul(55) / 100
        && runs.len() >= 2
}

fn partition_words_by_run_duration(words: &[String], runs: &[SpeechRun]) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = vec![Vec::new(); runs.len()];
    if words.is_empty() || runs.is_empty() {
        return groups;
    }

    let weights: Vec<u64> = runs
        .iter()
        .map(|run| run.end_ms.saturating_sub(run.start_ms).max(1))
        .collect();
    let total_weight: u64 = weights.iter().sum::<u64>().max(1);
    let mut word_index = 0usize;

    for (run_index, weight) in weights.iter().enumerate() {
        let remaining_runs = runs.len().saturating_sub(run_index);
        let remaining_words = words.len().saturating_sub(word_index);
        if remaining_words == 0 {
            break;
        }

        let take = if remaining_runs <= 1 {
            remaining_words
        } else {
            let max_take = remaining_words.saturating_sub(remaining_runs - 1);
            if max_take == 0 {
                // Fewer words than remaining runs — assign on later passes (take 0 here).
                0
            } else {
                let share =
                    (*weight as f64 / total_weight as f64 * words.len() as f64).round() as usize;
                share.clamp(1, max_take)
            }
        };

        if take > 0 {
            groups[run_index].extend(words[word_index..word_index + take].iter().cloned());
            word_index += take;
        }
    }

    if word_index < words.len() {
        groups
            .last_mut()
            .expect("runs non-empty")
            .extend(words[word_index..].iter().cloned());
    }

    groups
}

fn anchor_segments_to_speech_runs(
    segments: &[CoarseSegment],
    runs: &[SpeechRun],
    audio_duration_ms: u64,
) -> Vec<CoarseSegment> {
    if segments.is_empty() || runs.is_empty() {
        return segments.to_vec();
    }

    let single_segment_many_runs =
        segments.len() == 1 && runs.len() >= 2 && segment_words_for_alignment(&segments[0].text).len() >= runs.len();

    if !single_segment_many_runs && !whisper_timeline_mismatch(segments, runs, audio_duration_ms) {
        return segments.to_vec();
    }

    if segments.len() == 1 {
        let groups = partition_words_by_run_duration(
            &segment_words_for_alignment(&segments[0].text),
            runs,
        );
        return runs
            .iter()
            .zip(groups)
            .filter_map(|(run, words)| {
                if words.is_empty() {
                    return None;
                }
                Some(CoarseSegment {
                    text: words.join(" "),
                    start_ms: run.start_ms,
                    end_ms: run.end_ms,
                })
            })
            .collect();
    }

    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let run = runs[index.min(runs.len().saturating_sub(1))];
            CoarseSegment {
                text: segment.text.clone(),
                start_ms: run.start_ms,
                end_ms: run.end_ms,
            }
        })
        .collect()
}

fn enforce_coarse_segment_gaps(words: &mut [WordTiming], segments: &[CoarseSegment]) {
    if segments.len() < 2 || words.is_empty() {
        return;
    }

    let mut word_cursor = 0usize;
    for segment_index in 0..segments.len().saturating_sub(1) {
        let current = &segments[segment_index];
        let next = &segments[segment_index + 1];
        let words_in_segment = segment_words_for_alignment(&current.text).len();
        if words_in_segment == 0 {
            continue;
        }

        let last_index = word_cursor.saturating_add(words_in_segment).saturating_sub(1);
        let first_next_index = last_index.saturating_add(1);
        if first_next_index >= words.len() {
            break;
        }

        let coarse_gap = next.start_ms.saturating_sub(current.end_ms);
        if coarse_gap >= MIN_ACOUSTIC_PAUSE_MS {
            if words[last_index].end_ms > current.end_ms {
                words[last_index].end_ms = current.end_ms.max(words[last_index].start_ms);
            }
            if words[first_next_index].start_ms < next.start_ms {
                words[first_next_index].start_ms = next.start_ms;
            }
        }

        word_cursor = first_next_index;
    }
}

fn trim_each_word_to_local_speech(words: &mut [WordTiming], track: &EnvelopeTrack) {
    let gate = track.gate_levels();
    for index in 0..words.len() {
        let frame_lo = track
            .ms_to_frame(words[index].start_ms)
            .min(track.smoothed.len().saturating_sub(1));
        let frame_hi = if index + 1 < words.len() {
            track
                .ms_to_frame(words[index + 1].start_ms)
                .saturating_sub(1)
        } else {
            track.smoothed.len().saturating_sub(1)
        };
        let frame_hi = frame_hi.max(frame_lo);

        if let Some(first) = first_speech_frame(&track.smoothed, frame_lo, frame_hi, gate.high) {
            words[index].start_ms = words[index].start_ms.max(track.frame_to_ms(first));
        }
        if let Some(last) = last_speech_frame(&track.smoothed, frame_lo, frame_hi, gate.high) {
            words[index].end_ms = words[index]
                .end_ms
                .min(track.frame_to_ms(last).saturating_add(FRAME_MS));
        }
        if words[index].end_ms <= words[index].start_ms {
            words[index].end_ms = words[index].start_ms.saturating_add(FRAME_MS);
        }
    }
}

fn enforce_inter_word_silence_gaps(words: &mut [WordTiming], track: &EnvelopeTrack) {
    let gate = track.gate_levels();
    for index in 0..words.len().saturating_sub(1) {
        let frame_lo = track
            .ms_to_frame(words[index].end_ms.saturating_sub(20))
            .min(track.smoothed.len().saturating_sub(1));
        let frame_hi = track
            .ms_to_frame(
                words[index + 1]
                    .start_ms
                    .saturating_add(track.hop_ms.saturating_mul(8)),
            )
            .min(track.smoothed.len().saturating_sub(1));
        if frame_lo >= frame_hi {
            continue;
        }

        let mut run_start: Option<usize> = None;
        let mut best: Option<(usize, usize)> = None;
        for frame in frame_lo..=frame_hi {
            if track.smoothed[frame] < gate.low {
                if run_start.is_none() {
                    run_start = Some(frame);
                }
            } else if let Some(start) = run_start {
                let end = frame.saturating_sub(1);
                if best
                    .map(|(b_lo, b_hi)| end.saturating_sub(start) > b_hi.saturating_sub(b_lo))
                    .unwrap_or(true)
                {
                    best = Some((start, end));
                }
                run_start = None;
            }
        }

        if let Some((start, end)) = best {
            let pause_ms = (end.saturating_sub(start) as u64 + 1) * track.hop_ms;
            if pause_ms >= 200 {
                let silence_start = track.frame_to_ms(start);
                let silence_end = track.frame_to_ms(end.saturating_add(1));
                words[index].end_ms = words[index]
                    .end_ms
                    .min(silence_start)
                    .max(words[index].start_ms);
                words[index + 1].start_ms = words[index + 1].start_ms.max(silence_end);
            }
        }
    }
}

fn spread_glued_word_timings(words: &mut [WordTiming]) {
    let mut index = 0usize;
    while index < words.len() {
        let mut run_end = index + 1;
        while run_end < words.len() {
            let gap = words[run_end]
                .start_ms
                .saturating_sub(words[run_end - 1].end_ms);
            if gap >= MIN_WORD_DURATION_MS {
                break;
            }
            run_end += 1;
        }

        if run_end > index + 1 {
            let span_start = words[index].start_ms;
            let span_end = if run_end < words.len() {
                words[run_end].start_ms.saturating_sub(1)
            } else {
                words[run_end - 1]
                    .end_ms
                    .max(span_start.saturating_add(MIN_WORD_DURATION_MS))
            };
            let count = run_end - index;
            let total = span_end.saturating_sub(span_start).max(count as u64 * MIN_WORD_DURATION_MS);
            let slot = total / count as u64;

            for offset in 0..count {
                let word_index = index + offset;
                let start = span_start.saturating_add(slot * offset as u64);
                let end = if offset + 1 == count {
                    span_end.max(start.saturating_add(MIN_WORD_DURATION_MS))
                } else {
                    start.saturating_add(slot.saturating_sub(1).max(MIN_WORD_DURATION_MS))
                };
                words[word_index].start_ms = start;
                words[word_index].end_ms = end.max(start.saturating_add(MIN_WORD_DURATION_MS));
            }
        }

        index = run_end;
    }
}

fn enforce_minimum_word_duration(words: &mut [WordTiming]) {
    for word in words.iter_mut() {
        if word.end_ms < word.start_ms.saturating_add(MIN_WORD_DURATION_MS) {
            word.end_ms = word.start_ms.saturating_add(MIN_WORD_DURATION_MS);
        }
    }
}

fn finalize_word_monotonicity(words: &mut [WordTiming]) {
    enforce_minimum_word_duration(words);
    for index in 0..words.len().saturating_sub(1) {
        if words[index].end_ms >= words[index + 1].start_ms {
            if words[index + 1].start_ms > words[index].start_ms.saturating_add(1) {
                words[index].end_ms = words[index + 1].start_ms.saturating_sub(1);
            }
        }
        if words[index].end_ms <= words[index].start_ms {
            words[index].end_ms = words[index].start_ms.saturating_add(MIN_WORD_DURATION_MS);
        }
    }
}

fn assign_words_to_acoustic_clusters(
    audio: &AudioSegment,
    words: &[String],
    clusters: &[(usize, usize)],
    origin_ms: u64,
    hop_ms: u64,
    gate_ratio: f32,
) -> Vec<WordTiming> {
    if words.is_empty() || clusters.is_empty() {
        return Vec::new();
    }

    let runs: Vec<SpeechRun> = clusters
        .iter()
        .map(|(lo, hi)| SpeechRun {
            start_ms: frame_to_ms(origin_ms, *lo, hop_ms),
            end_ms: frame_to_ms(origin_ms, *hi, hop_ms).saturating_add(FRAME_MS),
        })
        .collect();
    let groups = partition_words_by_run_duration(words, &runs);
    let mut out = Vec::new();
    for ((lo, hi), group) in clusters.iter().zip(groups) {
        if group.is_empty() {
            continue;
        }
        let start_ms = frame_to_ms(origin_ms, *lo, hop_ms);
        let end_ms = frame_to_ms(origin_ms, *hi, hop_ms).saturating_add(FRAME_MS);
        let segment = CoarseSegment {
            text: group.join(" "),
            start_ms,
            end_ms,
        };
        out.extend(refine_via_energy(audio, &segment, &group, gate_ratio));
    }
    out
}

fn align_smart_speech_first(
    audio: &AudioSegment,
    segments: &[CoarseSegment],
    config: EnergyAlignConfig,
) -> Vec<WordTiming> {
    let gate_ratio = config.speech_gate_ratio.clamp(0.05, 0.95);
    let track = EnvelopeTrack::from_full_audio(audio, gate_ratio);
    let raw_clusters = split_oversized_clusters(
        &track.smoothed,
        gate_ratio,
        track.hop_ms,
        MAX_CLUSTER_MS_FOR_WORD_ASSIGN,
    );
    let clusters = merge_nearby_clusters(raw_clusters, track.hop_ms, config.acoustic_pause_ms);

    let all_words: Vec<String> = segments
        .iter()
        .flat_map(|segment| segment_words_for_alignment(&segment.text))
        .collect();
    if all_words.is_empty() {
        return Vec::new();
    }

    if clusters.is_empty() {
        return align_whisper_segments(audio, segments, gate_ratio, false);
    }

    let mut words = assign_words_to_acoustic_clusters(
        audio,
        &all_words,
        &clusters,
        track.origin_ms,
        track.hop_ms,
        gate_ratio,
    );

    spread_glued_word_timings(&mut words);
    trim_each_word_to_local_speech(&mut words, &track);
    enforce_inter_word_silence_gaps(&mut words, &track);
    open_gaps_between_words(&mut words, &track);
    finalize_word_monotonicity(&mut words);
    words
}

fn align_whisper_segments(
    audio: &AudioSegment,
    segments: &[CoarseSegment],
    gate_ratio: f32,
    anchor_on_mismatch: bool,
) -> Vec<WordTiming> {
    let track = EnvelopeTrack::from_full_audio(audio, gate_ratio);
    let speech_runs = track.detect_speech_runs();
    let anchored = if anchor_on_mismatch {
        anchor_segments_to_speech_runs(segments, &speech_runs, audio.duration_ms.max(1))
    } else {
        segments.to_vec()
    };
    let mut words: Vec<WordTiming> = Vec::new();

    for segment in &anchored {
        let segment_words = segment_words_for_alignment(&segment.text);
        words.extend(refine_via_energy(
            audio,
            segment,
            &segment_words,
            gate_ratio,
        ));
    }

    enforce_coarse_segment_gaps(&mut words, &anchored);
    trim_each_word_to_local_speech(&mut words, &track);
    enforce_inter_word_silence_gaps(&mut words, &track);
    open_gaps_between_words(&mut words, &track);
    finalize_word_monotonicity(&mut words);
    words
}

pub fn align_segments_via_energy(
    audio: &AudioSegment,
    segments: &[CoarseSegment],
    config: EnergyAlignConfig,
) -> Vec<WordTiming> {
    let gate_ratio = config.speech_gate_ratio.clamp(0.05, 0.95);
    if config.smart_speech_first {
        return align_smart_speech_first(audio, segments, config);
    }
    align_whisper_segments(audio, segments, gate_ratio, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::segment::AudioSegment;

    fn tone_segment(duration_ms: u64, gap_ms: u64) -> AudioSegment {
        let sample_rate = 16_000u32;
        let speech_samples = (duration_ms * sample_rate as u64 / 1000) as usize;
        let gap_samples = (gap_ms * sample_rate as u64 / 1000) as usize;
        let mut samples = vec![0.0f32; speech_samples + gap_samples + speech_samples];
        for sample in &mut samples[..speech_samples] {
            *sample = 0.25;
        }
        let second_start = speech_samples + gap_samples;
        for sample in &mut samples[second_start..second_start + speech_samples] {
            *sample = 0.25;
        }
        AudioSegment::new(samples, sample_rate, 1)
    }

    #[test]
    fn energy_finds_pause_between_two_words() {
        let audio = tone_segment(200, 120);
        let segment = CoarseSegment {
            text: "alpha beta".to_string(),
            start_ms: 0,
            end_ms: 520,
        };
        let timings = refine_via_energy(
            &audio,
            &segment,
            &["alpha".to_string(), "beta".to_string()],
            0.25,
        );
        assert_eq!(timings.len(), 2);
        assert!(
            timings[1].start_ms.saturating_sub(timings[0].end_ms) >= MIN_ACOUSTIC_PAUSE_MS,
            "gap {} -> {}",
            timings[0].end_ms,
            timings[1].start_ms
        );
    }

    #[test]
    fn gate_hysteresis_splits_pause_with_room_noise() {
        let mut smoothed = vec![-2.0f32; 12];
        smoothed.extend(vec![-5.0; 35]);
        smoothed.extend(vec![-2.0; 12]);
        let gate = speech_gate_levels(&smoothed, 0.25);
        let clusters = speech_frame_clusters(&smoothed, gate, 8);
        assert!(
            clusters.len() >= 2,
            "expected two speech blobs, got {} (high={}, low={})",
            clusters.len(),
            gate.high,
            gate.low
        );
    }

    #[test]
    fn single_word_does_not_span_trailing_silence_in_clip() {
        let sample_rate = 16_000u32;
        let speech_samples = (150 * sample_rate as u64 / 1000) as usize;
        let gap_samples = (600 * sample_rate as u64 / 1000) as usize;
        let mut samples = vec![0.0f32; speech_samples + gap_samples];
        for sample in &mut samples[..speech_samples] {
            *sample = 0.32;
        }
        let audio = AudioSegment::new(samples, sample_rate, 1);
        let segment = CoarseSegment {
            text: "name".to_string(),
            start_ms: 0,
            end_ms: 750,
        };
        let timings = refine_via_energy(&audio, &segment, &["name".to_string()], 0.25);
        assert!(
            timings[0].end_ms <= 250,
            "word end should not include 600ms silence tail, got {}",
            timings[0].end_ms
        );
    }

    #[test]
    fn single_word_starts_after_leading_silence() {
        let sample_rate = 16_000u32;
        let silence_samples = (300 * sample_rate as u64 / 1000) as usize;
        let speech_samples = (180 * sample_rate as u64 / 1000) as usize;
        let mut samples = vec![0.0f32; silence_samples + speech_samples];
        for sample in &mut samples[silence_samples..] {
            *sample = 0.28;
        }
        let audio = AudioSegment::new(samples, sample_rate, 1);
        let segment = CoarseSegment {
            text: "hello".to_string(),
            start_ms: 0,
            end_ms: 500,
        };
        let timings = refine_via_energy(&audio, &segment, &["hello".to_string()], 0.25);
        assert!(
            timings[0].start_ms >= 250,
            "expected late start, got {}",
            timings[0].start_ms
        );
        assert!(timings[0].end_ms < 500, "should not fill coarse segment tail");
    }

    fn two_blobs_wrong_whisper() -> (AudioSegment, Vec<CoarseSegment>) {
        let sample_rate = 16_000u32;
        let speech_ms = 180u64;
        let gap_ms = 450u64;
        let speech_samples = (speech_ms * sample_rate as u64 / 1000) as usize;
        let gap_samples = (gap_ms * sample_rate as u64 / 1000) as usize;
        let mut samples = vec![0.0f32; speech_samples + gap_samples + speech_samples];
        for sample in &mut samples[..speech_samples] {
            *sample = 0.3;
        }
        let second = speech_samples + gap_samples;
        for sample in &mut samples[second..second + speech_samples] {
            *sample = 0.3;
        }
        let audio = AudioSegment::new(samples, sample_rate, 1);
        let segments = vec![
            CoarseSegment {
                text: "one".into(),
                start_ms: 0,
                end_ms: speech_ms,
            },
            CoarseSegment {
                text: "two".into(),
                start_ms: speech_ms,
                end_ms: speech_ms * 2,
            },
        ];
        (audio, segments)
    }

    #[test]
    fn anchors_whisper_segments_to_later_speech_runs() {
        let (audio, segments) = two_blobs_wrong_whisper();
        let words = align_segments_via_energy(
            &audio,
            &segments,
            EnergyAlignConfig {
                speech_gate_ratio: 0.25,
                smart_speech_first: true,
                acoustic_pause_ms: 400,
            },
        );
        assert_eq!(words.len(), 2);
        assert!(
            words[1].start_ms >= 180 + 450 - 80,
            "second word should land on second blob, got {}",
            words[1].start_ms
        );
        assert!(
            words[1].start_ms.saturating_sub(words[0].end_ms) >= MIN_ACOUSTIC_PAUSE_MS,
            "gap {} -> {}",
            words[0].end_ms,
            words[1].start_ms
        );
    }

    #[test]
    fn partition_words_does_not_panic_when_words_fewer_than_runs() {
        let words = vec!["only".to_string()];
        let runs = vec![
            SpeechRun {
                start_ms: 0,
                end_ms: 500,
            },
            SpeechRun {
                start_ms: 600,
                end_ms: 1_000,
            },
        ];
        let groups = super::partition_words_by_run_duration(&words, &runs);
        assert_eq!(groups.iter().map(|g| g.len()).sum::<usize>(), 1);
    }

    #[test]
    fn global_pass_opens_glued_words() {
        let audio = tone_segment(150, 200);
        let mut glued = vec![
            WordTiming {
                text: "one".into(),
                start_ms: 0,
                end_ms: 200,
            },
            WordTiming {
                text: "two".into(),
                start_ms: 200,
                end_ms: 350,
            },
        ];
        let track = EnvelopeTrack::from_full_audio(&audio, 0.25);
        open_gaps_between_words(&mut glued, &track);
        assert!(
            glued[1].start_ms.saturating_sub(glued[0].end_ms) >= MIN_ACOUSTIC_PAUSE_MS,
            "gap {} -> {}",
            glued[0].end_ms,
            glued[1].start_ms
        );
    }
}
