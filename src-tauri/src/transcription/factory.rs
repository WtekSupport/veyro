use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::audio::segment::AudioSegment;
use crate::settings::{AppSettings, LocalSttEngine};
#[cfg(feature = "local-sherpa-stt")]
use crate::transcription::local_stt_model_store;
use crate::transcription::models::{TranscriptionOptions, TranscriptionResult};
use crate::transcription::openai::OpenAITranscriptionProvider;
use crate::transcription::provider::{TranscriptionError, TranscriptionProvider};

#[cfg(feature = "local-sherpa-stt")]
use crate::transcription::local_sherpa::LocalSherpaProvider;
#[cfg(feature = "local-whisper")]
use crate::transcription::local::{LocalWhisperConfig, LocalWhisperProvider};
#[cfg(feature = "local-whisper")]
use crate::transcription::model_store;

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
    match settings.local_stt_variant().engine() {
        LocalSttEngine::Sherpa => create_sherpa_transcriber(settings, cancel),
        LocalSttEngine::Whisper => create_whisper_transcriber(settings, cancel),
    }
}

fn create_whisper_transcriber(
    settings: &AppSettings,
    cancel: CancellationToken,
) -> Arc<dyn TranscriptionProvider> {
    #[cfg(feature = "local-whisper")]
    {
        match model_store::resolve_model_path(settings) {
            Ok(path) => {
                info_whisper_provider(&path);
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

fn create_sherpa_transcriber(
    settings: &AppSettings,
    cancel: CancellationToken,
) -> Arc<dyn TranscriptionProvider> {
    #[cfg(feature = "local-sherpa-stt")]
    {
        let variant = settings.local_stt_variant();
        match local_stt_model_store::effective_sherpa_bundle_dir(settings, variant) {
            Ok(path) => {
                if local_stt_model_store::bundle_ready_for_settings(settings, variant) {
                    tracing::info!("using sherpa STT bundle at {}", path.display());
                } else {
                    tracing::warn!(
                        "sherpa STT bundle not found at {} — download required",
                        path.display()
                    );
                }
                Arc::new(LocalSherpaProvider::new_with_bundle(
                    settings.clone(),
                    path,
                    cancel,
                ))
            }
            Err(error) => {
                warn!("sherpa model path unavailable: {error}");
                Arc::new(LocalTranscriptionUnavailable {
                    reason: error.to_string(),
                })
            }
        }
    }

    #[cfg(not(feature = "local-sherpa-stt"))]
    {
        let _ = (settings, cancel);
        warn!("sherpa STT requested but `local-sherpa-stt` feature is disabled");
        Arc::new(LocalTranscriptionUnavailable {
            reason: "sherpa STT is not available in this build".to_string(),
        })
    }
}

#[cfg(feature = "local-whisper")]
fn info_whisper_provider(path: &std::path::Path) {
    if model_store::model_exists(path) {
        tracing::info!("using local whisper model at {}", path.display());
    } else {
        tracing::warn!(
            "local whisper model not found at {} — download required",
            path.display()
        );
    }
}
