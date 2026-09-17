use async_trait::async_trait;
use reqwest::multipart;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::audio::encode;
use crate::audio::segment::AudioSegment;
use crate::network::openai_error::OpenAiError;
use crate::network::retry::RetryConfig;
use crate::settings::secrets;
use crate::transcription::models::{TranscriptionOptions, TranscriptionResult};
use crate::transcription::provider::{TranscriptionError, TranscriptionProvider};

const OPENAI_TRANSCRIPTIONS_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

pub struct OpenAITranscriptionProvider {
    client: reqwest::Client,
    cancel: CancellationToken,
}

impl OpenAITranscriptionProvider {
    pub fn new(client: reqwest::Client, cancel: CancellationToken) -> Self {
        Self { client, cancel }
    }

    fn build_form(
        audio_bytes: Vec<u8>,
        options: &TranscriptionOptions,
    ) -> Result<multipart::Form, TranscriptionError> {
        let part = multipart::Part::bytes(audio_bytes)
            .file_name("audio.flac")
            .mime_str("audio/flac")
            .map_err(|error| TranscriptionError::InvalidAudio(error.to_string()))?;

        let mut form = multipart::Form::new()
            .text("model", options.model.clone())
            .text("temperature", "0")
            .part("file", part);

        if let Some(language) = &options.language {
            form = form.text("language", language.clone());
        }
        if let Some(prompt) = &options.prompt {
            form = form.text("prompt", prompt.clone());
        }

        Ok(form)
    }
}

#[async_trait]
impl TranscriptionProvider for OpenAITranscriptionProvider {
    async fn transcribe(
        &self,
        audio: AudioSegment,
        options: TranscriptionOptions,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        if self.cancel.is_cancelled() {
            return Err(TranscriptionError::Cancelled);
        }

        let api_key = secrets::load_api_key().map_err(|_| OpenAiError::api_key_missing())?;
        let audio_bytes = encode::encode_flac(&audio)
            .or_else(|_| encode::encode_wav(&audio))
            .map_err(TranscriptionError::InvalidAudio)?;
        let config = RetryConfig::default();
        let mut delay = config.initial_delay_ms;

        for attempt in 0..config.max_attempts {
            if self.cancel.is_cancelled() {
                return Err(TranscriptionError::Cancelled);
            }

            let form = Self::build_form(audio_bytes.clone(), &options)?;
            let response = self
                .client
                .post(OPENAI_TRANSCRIPTIONS_URL)
                .bearer_auth(&api_key)
                .multipart(form)
                .send()
                .await;

            match response {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if response.status().is_success() {
                        let payload: serde_json::Value = response.json().await.map_err(|error| {
                            TranscriptionError::OpenAi(OpenAiError {
                                kind: crate::network::openai_error::OpenAiErrorKind::Network,
                                status: None,
                                api_message: Some(error.to_string()),
                            })
                        })?;

                        let text = payload
                            .get("text")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .trim()
                            .to_string();

                        debug!(chars = text.len(), "transcription completed");

                        return Ok(TranscriptionResult {
                            timed_segments: None,
                            text,
                            confidence: None,
                            whisper_segments: None,
                            audio_peak: None,
                            audio_rms: None,
                            detected_language: None,
                        });
                    }

                    let body = response.text().await.unwrap_or_default();
                    let parsed = OpenAiError::from_http(status, &body);
                    if !parsed.retryable() || attempt + 1 >= config.max_attempts {
                        return Err(TranscriptionError::OpenAi(parsed));
                    }

                    warn!(
                        "OpenAI STT attempt {} received retryable HTTP {status}: {}",
                        attempt + 1,
                        parsed.api_message.as_deref().unwrap_or("")
                    );
                }
                Err(error) => {
                    warn!(
                        "OpenAI STT attempt {} failed to reach API: {error}",
                        attempt + 1
                    );
                    if attempt + 1 >= config.max_attempts {
                        return Err(TranscriptionError::OpenAi(
                            OpenAiError::from_network_exhausted(&error, config.max_attempts),
                        ));
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            delay = (delay * 2).min(config.max_delay_ms);
        }

        Err(TranscriptionError::OpenAi(OpenAiError {
            kind: crate::network::openai_error::OpenAiErrorKind::Network,
            status: None,
            api_message: Some(format!("failed after {} attempts", config.max_attempts)),
        }))
    }
}
