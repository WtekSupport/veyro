pub mod analyzer;
pub mod config;
pub mod detector;
pub mod threshold;
#[cfg(feature = "vad-silero")]
pub mod silero_model;

pub use analyzer::{silero_compiled, silero_runtime_available};
#[cfg(feature = "vad-silero")]
pub use analyzer::SILERO_RUNTIME_AVAILABLE;
pub use config::VadConfig;
pub use detector::{VadDetector, VadEvent};
