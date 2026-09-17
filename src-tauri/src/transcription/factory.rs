use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;
#[cfg(feature = "local-whisper")]
use crate::transcription::model_store;
use crate::transcription::openai::OpenAITranscriptionProvider;
use crate::transcription::provider::{TranscriptionError, TranscriptionProvider};
use crate::transcription::models::{TranscriptionOptions, TranscriptionResult};

#[cfg(feature = "local-whisper")]
use crate::transcription::local::{LocalWhisperConfig, LocalWhisperProvider};

struct LocalTranscriptionUnavailable {
    reason: String,
}

#[async_trait]
impl TranscriptionProvider for LocalTranscriptionUnavailable {
    async fn transcribe(
        &self,
        _audio: AudioSegment,
        _options: TranscriptionOptions,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        Err(TranscriptionError::ModelNotFound(self.reason.clone()))
    }
}

pub fn create_transcriber(
    settings: &AppSettings,
    http: Client,
    cancel: CancellationToken,
) -> Arc<dyn TranscriptionProvider> {
    if settings.transcription_provider == "local" {
        return create_local_transcriber(settings, cancel);
    }

    Arc::new(OpenAITranscriptionProvider::new(http, cancel))
}

fn create_local_transcriber(
    settings: &AppSettings,
    cancel: CancellationToken,
) -> Arc<dyn TranscriptionProvider> {
    #[cfg(feature = "local-whisper")]
    {
        match model_store::resolve_model_path(settings) {
            Ok(path) => {
                info_local_provider(&path);
                let config = LocalWhisperConfig {
                    use_gpu: settings.local_whisper_use_gpu,
                    beam_size: settings.local_whisper_beam_size,
                };
                return Arc::new(LocalWhisperProvider::new(path, config, cancel));
            }
            Err(error) => {
                warn!("local whisper model path unavailable: {error}");
                return Arc::new(LocalTranscriptionUnavailable {
                    reason: error.to_string(),
                });
            }
        }
    }

    #[cfg(not(feature = "local-whisper"))]
    {
        let _ = (settings, cancel);
        warn!("local whisper requested but `local-whisper` feature is disabled");
        Arc::new(LocalTranscriptionUnavailable {
            reason: "local whisper is not available in this build".to_string(),
        })
    }
}

#[cfg(feature = "local-whisper")]
fn info_local_provider(path: &std::path::Path) {
    if model_store::model_exists(path) {
        tracing::info!("using local whisper model at {}", path.display());
    } else {
        tracing::warn!(
            "local whisper model not found at {} — download required",
            path.display()
        );
    }
}
