use async_trait::async_trait;
use veyro_lib::audio::segment::AudioSegment;
use veyro_lib::injection::{MockInjector, TextInjector};
use veyro_lib::settings::{AppSettings, InjectionMode, TextProcessingMode};
use veyro_lib::text::process_transcription;
use veyro_lib::transcription::models::{TranscriptionOptions, TranscriptionResult};
use veyro_lib::transcription::provider::{TranscriptionError, TranscriptionProvider};

struct MockTranscriber;

#[async_trait]
impl TranscriptionProvider for MockTranscriber {
    async fn transcribe(
        &self,
        _audio: AudioSegment,
        _options: TranscriptionOptions,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        Ok(TranscriptionResult {
            text: "hello world".to_string(),
            confidence: None,
            whisper_segments: None,
            timed_segments: None,
            audio_peak: None,
            audio_rms: None,
            detected_language: None,
        })
    }
}

#[tokio::test]
async fn mock_pipeline_processes_and_injects() {
    let transcriber = MockTranscriber;
    let injector = MockInjector::new();
    let http = reqwest::Client::new();

    let segment = AudioSegment::new(vec![0.0; 1600], 16_000, 1);
    let result = transcriber
        .transcribe(
            segment,
            TranscriptionOptions {
                language: None,
                prompt: None,
                model: "whisper-1".to_string(),
                whisper_decoding: None,
                dictionary_path: None,
            },
        )
        .await
        .unwrap();

    let settings = AppSettings {
        text_processing_mode: TextProcessingMode::Basic,
        ..Default::default()
    };
    let llm = veyro_lib::llm::LlmEngine::unloaded(None);
    let processed = process_transcription(&result.text, None, &settings, &http, &llm, None)
        .await
        .unwrap();

    injector
        .insert_text(&processed.text, InjectionMode::Auto)
        .await
        .unwrap();

    let injected = injector.last_text.lock().unwrap();
    assert_eq!(injected.as_deref(), Some("Hello world"));
}
