use thiserror::Error;

#[derive(Debug, Error)]
pub enum SeparationError {
    #[error("invalid audio buffer: {0}")]
    InvalidAudio(String),
    #[error("model load failed: {0}")]
    ModelLoad(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("out of memory")]
    OutOfMemory,
    #[error("incompatible model file for this build (wrong ONNX export); re-download from the tool")]
    IncompatibleModel,
    #[error("unsupported model kind")]
    UnsupportedModel,
}

impl SeparationError {
    pub fn is_oom(&self) -> bool {
        matches!(self, Self::OutOfMemory)
            || self
                .to_string()
                .to_ascii_lowercase()
                .contains("out of memory")
    }
}
