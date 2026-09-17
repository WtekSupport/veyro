use std::fs;
use std::path::PathBuf;

use tracing::warn;

use crate::audio::encode::encode_wav;
use crate::audio::segment::AudioSegment;

pub fn save_last_ptt_wav(segment: &AudioSegment) -> Option<PathBuf> {
    let base = dirs::config_dir()?.join("Veyro").join("debug");
    if fs::create_dir_all(&base).is_err() {
        return None;
    }

    let path = base.join("last-ptt-failure.wav");
    let bytes = encode_wav(segment).ok()?;
    if fs::write(&path, bytes).is_err() {
        warn!(path = %path.display(), "failed to write debug wav");
        return None;
    }

    Some(path)
}
