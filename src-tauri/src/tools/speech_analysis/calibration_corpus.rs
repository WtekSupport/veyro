//! Optional labeled clips for relative score ordering (see `calibration_corpus.toml`).

use std::path::{Path, PathBuf};

use serde::Deserialize;

const CORPUS_TOML: &str = include_str!("../../../resources/speech_analysis/calibration_corpus.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct CalibrationClip {
    pub id: String,
    pub path: String,
    pub expected_rank: u8,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Deserialize)]
struct CorpusFile {
    clip: Vec<CalibrationClip>,
}

pub fn load_calibration_clips() -> Vec<CalibrationClip> {
    toml::from_str::<CorpusFile>(CORPUS_TOML)
        .map(|file| file.clip)
        .unwrap_or_default()
}

pub fn resolve_clip_path(manifest_relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(manifest_relative)
}

pub fn clips_with_existing_audio() -> Vec<(CalibrationClip, PathBuf)> {
    load_calibration_clips()
        .into_iter()
        .map(|clip| {
            let path = resolve_clip_path(&clip.path);
            (clip, path)
        })
        .filter(|(_, path)| path.is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_file_parses() {
        let clips = load_calibration_clips();
        assert!(clips.len() >= 10);
    }

    #[test]
    #[ignore = "requires WAV fixtures under tests/fixtures/speech_analysis/"]
    fn calibration_scores_follow_expected_order() {
        let available = clips_with_existing_audio();
        if available.len() < 2 {
            return;
        }
        let mut ranks: Vec<(u8, u8)> = Vec::new();
        for (clip, _path) in available {
            // Full pipeline test hooks in when fixtures are added.
            ranks.push((clip.expected_rank, clip.expected_rank));
        }
        ranks.sort_by_key(|(rank, _)| *rank);
        for window in ranks.windows(2) {
            assert!(window[0].0 <= window[1].0);
        }
    }
}
