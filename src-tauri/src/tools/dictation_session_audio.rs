//! Accumulates dictation PCM per session and exports `ses{started_at_ms}.wav` into the shared voice index.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use tauri::AppHandle;

use crate::audio::encode::encode_wav;
use crate::audio::segment::AudioSegment;
static BUFFERS: LazyLock<Mutex<HashMap<u64, AudioSegment>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn merge_segment(into: &mut AudioSegment, chunk: &AudioSegment) {
    if chunk.is_empty() {
        return;
    }
    if into.is_empty() {
        *into = chunk.clone();
        return;
    }
    if into.sample_rate != chunk.sample_rate || into.channels != chunk.channels {
        tracing::warn!(
            "dictation session audio: skip chunk with mismatched format ({}Hz {}ch vs {}Hz {}ch)",
            chunk.sample_rate,
            chunk.channels,
            into.sample_rate,
            into.channels
        );
        return;
    }
    into.samples.extend_from_slice(&chunk.samples);
    into.duration_ms = into.duration_ms.saturating_add(chunk.duration_ms);
}

pub fn append_session_segment(session_id: u64, segment: &AudioSegment) {
    if session_id == 0 || segment.is_empty() {
        return;
    }
    let Ok(mut map) = BUFFERS.lock() else {
        return;
    };
    let slot = map
        .entry(session_id)
        .or_insert_with(|| AudioSegment::new(Vec::new(), segment.sample_rate, segment.channels));
    merge_segment(slot, segment);
}

pub fn take_session_segment(session_id: u64) -> Option<AudioSegment> {
    let Ok(mut map) = BUFFERS.lock() else {
        return None;
    };
    map.remove(&session_id).filter(|s| !s.is_empty())
}

fn session_wav_display_name(started_at_ms: u64) -> String {
    format!("ses{started_at_ms}.wav")
}

/// Write accumulated audio and register it in `voice-history.json` (Files sidebar).
pub fn publish_session_recording(
    app: Option<&AppHandle>,
    session_id: u64,
    started_at_ms: u64,
    transcript_text: &str,
) {
    let Some(segment) = take_session_segment(session_id) else {
        return;
    };
    let Some(app) = app else {
        return;
    };

    let file_name = session_wav_display_name(started_at_ms);
    let duration_secs = segment.duration_ms as f64 / 1000.0;

    let wav_bytes = match encode_wav(&segment) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("dictation session wav encode failed: {err}");
            return;
        }
    };

    let tmp_path: PathBuf = std::env::temp_dir().join(&file_name);
    if let Err(err) = fs::write(&tmp_path, &wav_bytes) {
        tracing::warn!("dictation session wav temp write failed: {err}");
        return;
    }

    let result = crate::tools::voice_watch::register_dictation_session_recording(
        app,
        &tmp_path.to_string_lossy(),
        transcript_text,
        started_at_ms,
        duration_secs,
    );
    let _ = fs::remove_file(&tmp_path);

    if let Err(err) = result {
        tracing::warn!("dictation session voice index register failed: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_same_format_chunks() {
        let mut a = AudioSegment::new(vec![0.1; 1600], 16_000, 1);
        let b = AudioSegment::new(vec![0.2; 800], 16_000, 1);
        merge_segment(&mut a, &b);
        assert_eq!(a.samples.len(), 2400);
        assert_eq!(a.duration_ms, 150);
    }
}
