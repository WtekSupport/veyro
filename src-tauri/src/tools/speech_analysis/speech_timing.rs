//! Unified “clean speech” duration: VAD spans minus energy pauses, edges trimmed.

use super::pause_energy::PauseInterval;
use super::qc::SpeechSpan;

/// Drop leading/trailing silence inside the outer VAD spans (≈250 ms margin).
pub fn trim_speech_span_edges(spans: &[SpeechSpan], total_duration_ms: u64) -> Vec<SpeechSpan> {
    if spans.is_empty() {
        return Vec::new();
    }
    const EDGE_TRIM_MS: u64 = 250;
    let mut sorted = spans.to_vec();
    sorted.sort_by_key(|(s, _)| *s);
    let mut out = Vec::new();
    for (i, (start, end)) in sorted.iter().enumerate() {
        let mut s = *start;
        let mut e = *end;
        if i == 0 {
            s = s.saturating_add(EDGE_TRIM_MS);
        }
        if i + 1 == sorted.len() {
            e = e.saturating_sub(EDGE_TRIM_MS);
        }
        if e > s.saturating_add(80) {
            out.push((s, e.min(total_duration_ms)));
        }
    }
    out
}

/// Pauses fully inside speech (exclude file-head/tail silence).
pub fn pauses_inside_speech(
    pauses: &[PauseInterval],
    spans: &[SpeechSpan],
    total_duration_ms: u64,
) -> Vec<PauseInterval> {
    if spans.is_empty() {
        return Vec::new();
    }
    let speech_start = spans.iter().map(|(s, _)| *s).min().unwrap_or(0);
    let speech_end = spans
        .iter()
        .map(|(_, e)| *e)
        .max()
        .unwrap_or(total_duration_ms);
    pauses
        .iter()
        .copied()
        .filter(|p| {
            let end = p.start_ms.saturating_add(p.duration_ms);
            p.start_ms >= speech_start.saturating_add(100)
                && end <= speech_end.saturating_sub(100)
        })
        .collect()
}

pub fn gross_speech_duration_ms(spans: &[SpeechSpan]) -> u64 {
    spans
        .iter()
        .map(|(start, end)| end.saturating_sub(*start))
        .sum::<u64>()
        .max(1)
}

/// Active speech ≈ time on voice (VAD) minus internal pauses detected from energy.
pub fn net_speech_duration_ms(gross_speech_ms: u64, internal_pause_ms: u64) -> u64 {
    gross_speech_ms
        .saturating_sub(internal_pause_ms)
        .max(800)
}

#[allow(dead_code)]
pub fn speech_minutes(net_speech_ms: u64) -> f32 {
    net_speech_ms as f32 / 60_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_speech_subtracts_pauses() {
        assert_eq!(net_speech_duration_ms(48_000, 23_000), 25_000);
    }

    #[test]
    fn trims_span_edges() {
        let spans = vec![(0, 10_000)];
        let trimmed = trim_speech_span_edges(&spans, 10_000);
        assert_eq!(trimmed[0].0, 250);
        assert_eq!(trimmed[0].1, 9750);
    }
}
