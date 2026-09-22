use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::audio::encode::{decode_wav_file, encode_wav};
use crate::audio::segment::AudioSegment;
use crate::error::ConfigError;
use crate::settings::data_storage::{resolve_data_storage_root, SUBDIR_SEGMENT_QUEUE};
use crate::settings::AppSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpillMeta {
    pub seq: u64,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub created_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PendingSpill {
    pub meta: SpillMeta,
    pub wav_path: PathBuf,
}

pub struct SegmentQueueStore {
    root: PathBuf,
}

impl SegmentQueueStore {
    pub fn for_settings(settings: &AppSettings) -> Result<Self, ConfigError> {
        let root = resolve_data_storage_root(settings)?.join(SUBDIR_SEGMENT_QUEUE);
        fs::create_dir_all(&root).map_err(|error| {
            ConfigError::Write(format!("{}: {error}", root.display()))
        })?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn total_bytes(&self) -> u64 {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return 0;
        };
        entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.metadata().ok())
            .map(|meta| meta.len())
            .sum()
    }

    pub fn list_pending(&self) -> Vec<PendingSpill> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };

        let mut items = Vec::new();
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(meta) = serde_json::from_str::<SpillMeta>(&raw) else {
                continue;
            };
            let wav_path = self.wav_path_for_seq(meta.seq);
            if !wav_path.is_file() {
                continue;
            }
            items.push(PendingSpill { meta, wav_path });
        }

        items.sort_by_key(|item| item.meta.seq);
        items
    }

    pub fn spill(&self, segment: &AudioSegment) -> Result<PathBuf, String> {
        let seq = self.next_seq();
        let wav_path = self.wav_path_for_seq(seq);
        let json_path = self.json_path_for_seq(seq);
        let bytes = encode_wav(segment)?;
        fs::write(&wav_path, bytes).map_err(|error| error.to_string())?;

        let meta = SpillMeta {
            seq,
            duration_ms: segment.duration_ms,
            sample_rate: segment.sample_rate,
            channels: segment.channels,
            created_ms: now_ms(),
        };
        let json = serde_json::to_string(&meta).map_err(|error| error.to_string())?;
        fs::write(&json_path, json).map_err(|error| error.to_string())?;
        Ok(wav_path)
    }

    pub fn load(path: &Path) -> Result<AudioSegment, String> {
        decode_wav_file(path)
    }

    pub fn delete_pair_for_wav(&self, wav_path: &Path) {
        if fs::remove_file(wav_path).is_err() {
            warn!(path = %wav_path.display(), "failed to remove spilled segment wav");
        }
        if let Some(stem) = wav_path.file_stem().and_then(|s| s.to_str()) {
            if let Ok(seq) = stem.parse::<u64>() {
                let json_path = self.json_path_for_seq(seq);
                let _ = fs::remove_file(json_path);
            }
        }
    }

    pub fn clear_all(&self) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            let _ = fs::remove_file(entry.path());
        }
    }

    fn next_seq(&self) -> u64 {
        self.list_pending()
            .into_iter()
            .map(|item| item.meta.seq)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    fn wav_path_for_seq(&self, seq: u64) -> PathBuf {
        self.root.join(format!("{seq:010}.wav"))
    }

    fn json_path_for_seq(&self, seq: u64) -> PathBuf {
        self.root.join(format!("{seq:010}.json"))
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_store() -> SegmentQueueStore {
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("veyro-seg-queue-test-{seq}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        SegmentQueueStore { root }
    }

    #[test]
    fn spill_load_roundtrip_preserves_duration() {
        let store = temp_store();
        let segment = AudioSegment::new(vec![0.1, -0.2, 0.3, 0.0], 16_000, 1);
        let path = store.spill(&segment).expect("spill");
        let loaded = SegmentQueueStore::load(&path).expect("load");
        assert_eq!(loaded.duration_ms, segment.duration_ms);
        assert_eq!(loaded.samples.len(), segment.samples.len());
        store.delete_pair_for_wav(&path);
    }

    #[test]
    fn list_pending_sorted_by_seq() {
        let store = temp_store();
        let a = AudioSegment::new(vec![0.1; 100], 16_000, 1);
        let b = AudioSegment::new(vec![0.2; 100], 16_000, 1);
        store.spill(&a).unwrap();
        store.spill(&b).unwrap();
        let pending = store.list_pending();
        assert_eq!(pending.len(), 2);
        assert!(pending[0].meta.seq < pending[1].meta.seq);
        store.clear_all();
    }
}
