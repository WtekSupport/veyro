//! Optional speaker diarization for Audio → SRT (`todo/ТЗ_ диаризация в SRT-инструменте Veyro.md`).

mod assign;
mod engine;
pub mod model_store;

pub use assign::{
    assign_speakers_by_max_overlap, overlap_ms, speaker_by_max_overlap, SpeakerInterval,
};
pub use engine::{diarize_segment, DiarizeOptions, SpeakerCountMode};
pub use model_store::{ensure_models, model_ready, status, DiarizationModelStatus};
