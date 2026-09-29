//! Auto-watch messenger download folders and enqueue voice files for STT.

mod filter;
mod history;
mod presets;
mod queue;
mod registry;
mod service;
mod stability;
mod watcher;

pub use history::{
    clear_history, delete_history_entry, list_history, HistoryEntry, HistoryStatus,
};
pub use presets::{list_presets, VoiceWatchPreset};
pub use queue::{
    apply_text_mode_override, enqueue_paths, get_queue_snapshot, remove_from_index,
    retry_history_entry, VoiceJob, VoiceJobMeta, VoiceJobSource, VoiceQueueSnapshot,
};
pub use service::{
    apply_settings_to_voice_watch, is_cloud_provider_active, start_voice_watch_runtime,
    voice_watch_runtime, voice_watch_set_enabled, VoiceWatchStatus,
};
