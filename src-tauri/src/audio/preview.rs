use crate::audio::segment::AudioSegment;

/// How often the capture pipeline asks Whisper for a live preview.
pub const PREVIEW_POLL_INTERVAL_MS: u64 = 350;
/// Minimum captured audio before the first preview attempt.
pub const PREVIEW_MIN_AUDIO_MS: u64 = 450;
/// Only decode the latest slice so UI previews stay fast during long utterances.
pub const PREVIEW_MAX_AUDIO_MS: u64 = 3_000;
/// Shorter window for live dictation previews — faster Whisper passes while recording.
pub const PREVIEW_LIVE_MAX_AUDIO_MS: u64 = 2_000;
/// Full capture window for live dictation (incremental injection while recording).
pub const LIVE_DICTATION_MAX_AUDIO_MS: u64 = 45_000;

pub fn preview_min_samples(sample_rate: u32) -> usize {
    ((sample_rate as u64 * PREVIEW_MIN_AUDIO_MS) / 1000) as usize
}

pub fn trim_for_preview(segment: &AudioSegment) -> Option<AudioSegment> {
    if segment.duration_ms < PREVIEW_MIN_AUDIO_MS {
        return None;
    }

    let max_samples = ((segment.sample_rate as u64 * PREVIEW_MAX_AUDIO_MS) / 1000) as usize;
    if segment.samples.len() <= max_samples {
        return Some(segment.clone());
    }

    let tail = segment
        .samples
        .len()
        .saturating_sub(max_samples);
    Some(AudioSegment::new(
        segment.samples[tail..].to_vec(),
        segment.sample_rate,
        segment.channels,
    ))
}

pub fn trim_for_live_dictation(segment: &AudioSegment) -> Option<AudioSegment> {
    trim_for_live_dictation_window(segment, LIVE_DICTATION_MAX_AUDIO_MS)
}

pub fn trim_for_live_preview(segment: &AudioSegment) -> Option<AudioSegment> {
    trim_for_live_dictation_window(segment, PREVIEW_LIVE_MAX_AUDIO_MS)
}

fn trim_for_live_dictation_window(segment: &AudioSegment, max_ms: u64) -> Option<AudioSegment> {
    if segment.duration_ms < PREVIEW_MIN_AUDIO_MS {
        return None;
    }

    let max_samples = ((segment.sample_rate as u64 * max_ms) / 1000) as usize;
    if segment.samples.len() <= max_samples {
        return Some(segment.clone());
    }

    let tail = segment.samples.len().saturating_sub(max_samples);
    Some(AudioSegment::new(
        segment.samples[tail..].to_vec(),
        segment.sample_rate,
        segment.channels,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::resampler::TARGET_SAMPLE_RATE;

    #[test]
    fn rejects_audio_shorter_than_preview_minimum() {
        let segment = AudioSegment::new(vec![0.0; 6_000], TARGET_SAMPLE_RATE, 1);
        assert!(trim_for_preview(&segment).is_none());
    }

    #[test]
    fn keeps_short_capture_whole() {
        let segment = AudioSegment::new(vec![0.0; 20_000], TARGET_SAMPLE_RATE, 1);
        let preview = trim_for_preview(&segment).expect("preview");
        assert_eq!(preview.samples.len(), 20_000);
    }

    #[test]
    fn trims_to_latest_preview_window() {
        let segment = AudioSegment::new(vec![0.0; 120_000], TARGET_SAMPLE_RATE, 1);
        let preview = trim_for_preview(&segment).expect("preview");
        assert_eq!(preview.duration_ms, PREVIEW_MAX_AUDIO_MS);
    }
}
