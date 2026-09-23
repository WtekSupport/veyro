use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use sherpa_onnx::OfflineRecognizer;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::audio::resampler::TARGET_SAMPLE_RATE;
use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;
use crate::settings::LocalSttVariant;
use crate::transcription::local_stt_model_store::{
    bundle_ready_for_settings, effective_sherpa_bundle_dir,
};
use crate::transcription::models::{TranscriptionOptions, TranscriptionResult};
use crate::transcription::provider::{TranscriptionError, TranscriptionProvider};
use crate::transcription::sherpa::build_offline_config;

/// Serialize all sherpa-onnx native calls (prewarm + transcribe share one recognizer).
fn sherpa_inference_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct SharedSherpa {
    bundle_dir: PathBuf,
    settings_snapshot: SherpaSettingsSnapshot,
    recognizer: Mutex<Option<OfflineRecognizer>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SherpaSettingsSnapshot {
    variant: LocalSttVariant,
    use_gpu: bool,
    num_threads: u32,
    provider: String,
}

impl SharedSherpa {
    fn from_settings(settings: &AppSettings, bundle_dir: PathBuf) -> Self {
        Self {
            bundle_dir,
            settings_snapshot: SherpaSettingsSnapshot {
                variant: settings.local_stt_variant(),
                use_gpu: settings.local_whisper_use_gpu,
                num_threads: settings.local_sherpa_num_threads,
                provider: crate::transcription::sherpa::execution_provider(settings).to_string(),
            },
            recognizer: Mutex::new(None),
        }
    }

    fn settings_match(&self, settings: &AppSettings) -> bool {
        self.settings_snapshot.variant == settings.local_stt_variant()
            && self.settings_snapshot.use_gpu == settings.local_whisper_use_gpu
            && self.settings_snapshot.num_threads == settings.local_sherpa_num_threads
            && self.settings_snapshot.provider
                == crate::transcription::sherpa::execution_provider(settings)
    }

    fn unload(&self) {
        if let Ok(mut guard) = self.recognizer.lock() {
            *guard = None;
        }
    }

    fn is_loaded(&self) -> bool {
        self.recognizer
            .lock()
            .ok()
            .is_some_and(|guard| guard.is_some())
    }

    fn ensure_loaded(&self, settings: &AppSettings) -> Result<(), TranscriptionError> {
        let mut guard = self
            .recognizer
            .lock()
            .map_err(|_| TranscriptionError::InferenceFailed("sherpa lock poisoned".to_string()))?;

        if guard.is_some() && self.settings_match(settings) {
            return Ok(());
        }

        if !bundle_ready_for_settings(settings, settings.local_stt_variant()) {
            return Err(TranscriptionError::ModelNotFound(
                self.bundle_dir.display().to_string(),
            ));
        }

        let config = build_offline_config(settings, &self.bundle_dir)
            .map_err(TranscriptionError::InferenceFailed)?;

        let provider = config.model_config.provider.clone().unwrap_or_default();
        info!(
            provider = %provider,
            variant = ?settings.local_stt_variant(),
            "loading sherpa STT recognizer"
        );

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            TranscriptionError::InferenceFailed("failed to create sherpa recognizer".to_string())
        })?;
        *guard = Some(recognizer);
        Ok(())
    }

    fn decode_segment(
        &self,
        settings: &AppSettings,
        audio: &AudioSegment,
        options: &TranscriptionOptions,
    ) -> Result<String, TranscriptionError> {
        let _infer = sherpa_inference_lock()
            .lock()
            .map_err(|_| TranscriptionError::InferenceFailed("sherpa inference lock poisoned".into()))?;

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.decode_segment_inner(settings, audio, options)
        }));
        match outcome {
            Ok(result) => result,
            Err(_) => {
                warn!("sherpa decode panicked; unloading recognizer");
                self.unload();
                Err(TranscriptionError::InferenceFailed(
                    "sherpa decode panicked".to_string(),
                ))
            }
        }
    }

    fn decode_segment_inner(
        &self,
        settings: &AppSettings,
        audio: &AudioSegment,
        options: &TranscriptionOptions,
    ) -> Result<String, TranscriptionError> {
        self.ensure_loaded(settings)?;

        let guard = self
            .recognizer
            .lock()
            .map_err(|_| TranscriptionError::InferenceFailed("sherpa lock poisoned".to_string()))?;
        let recognizer = guard.as_ref().ok_or_else(|| {
            TranscriptionError::InferenceFailed("sherpa recognizer not loaded".to_string())
        })?;

        if audio.sample_rate != TARGET_SAMPLE_RATE {
            return Err(TranscriptionError::InferenceFailed(format!(
                "sherpa expected {} Hz audio, got {} Hz",
                TARGET_SAMPLE_RATE, audio.sample_rate
            )));
        }

        info!(
            ms = audio.duration_ms,
            samples = audio.samples.len(),
            "sherpa decode start"
        );

        let stream = recognizer.create_stream();
        if let Some(lang) = options.language.as_deref().filter(|v| !v.is_empty()) {
            stream.set_option("language", lang);
        }
        stream.accept_waveform(audio.sample_rate as i32, &audio.samples);
        recognizer.decode(&stream);
        let result = stream.get_result().ok_or_else(|| {
            TranscriptionError::InferenceFailed("sherpa decode returned no result".to_string())
        })?;
        let text = result.text.trim().to_string();
        info!(chars = text.chars().count(), "sherpa decode done");
        Ok(text)
    }
}

pub struct LocalSherpaProvider {
    settings: Arc<Mutex<AppSettings>>,
    model: Arc<SharedSherpa>,
    _cancel: CancellationToken,
}

impl LocalSherpaProvider {
    pub fn new(settings: AppSettings, cancel: CancellationToken) -> Result<Self, TranscriptionError> {
        let variant = settings.local_stt_variant();
        let bundle_dir = effective_sherpa_bundle_dir(&settings, variant).map_err(|error| {
            TranscriptionError::ModelNotFound(error.to_string())
        })?;
        Ok(Self {
            settings: Arc::new(Mutex::new(settings.clone())),
            model: Arc::new(SharedSherpa::from_settings(&settings, bundle_dir)),
            _cancel: cancel,
        })
    }

    pub fn new_with_bundle(
        settings: AppSettings,
        bundle_dir: PathBuf,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            settings: Arc::new(Mutex::new(settings.clone())),
            model: Arc::new(SharedSherpa::from_settings(&settings, bundle_dir)),
            _cancel: cancel,
        }
    }

    fn current_settings(&self) -> Result<AppSettings, TranscriptionError> {
        self.settings
            .lock()
            .map(|guard| guard.clone())
            .map_err(|_| TranscriptionError::InferenceFailed("settings lock poisoned".to_string()))
    }
}

async fn run_sherpa_on_std_thread<F, T>(work: F) -> Result<T, TranscriptionError>
where
    F: FnOnce() -> Result<T, TranscriptionError> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("sherpa-stt".into())
        .spawn(move || {
            let _ = tx.send(work());
        })
        .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?;

    rx.await
        .map_err(|_| TranscriptionError::InferenceFailed("sherpa worker dropped".to_string()))?
}

#[async_trait]
impl TranscriptionProvider for LocalSherpaProvider {
    async fn transcribe(
        &self,
        audio: AudioSegment,
        options: TranscriptionOptions,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        if self._cancel.is_cancelled() {
            return Err(TranscriptionError::Cancelled);
        }

        let settings = self.current_settings()?;
        let model = Arc::clone(&self.model);
        let cancel = self._cancel.clone();
        let language = options.language.clone();

        run_sherpa_on_std_thread(move || {
            if cancel.is_cancelled() {
                return Err(TranscriptionError::Cancelled);
            }
            let text = model.decode_segment(&settings, &audio, &options)?;
            Ok(TranscriptionResult {
                text,
                confidence: None,
                whisper_segments: None,
                timed_segments: None,
                audio_peak: None,
                audio_rms: None,
                detected_language: language,
            })
        })
        .await
    }

    async fn prewarm(&self) -> Result<(), TranscriptionError> {
        if self._cancel.is_cancelled() {
            return Err(TranscriptionError::Cancelled);
        }

        let settings = self.current_settings()?;
        let model = Arc::clone(&self.model);
        let cancel = self._cancel.clone();
        let sample_count = (TARGET_SAMPLE_RATE as usize * 300) / 1000;
        let segment = AudioSegment::new(vec![0.0; sample_count], TARGET_SAMPLE_RATE, 1);
        let options = TranscriptionOptions::default();

        run_sherpa_on_std_thread(move || {
            if cancel.is_cancelled() {
                return Err(TranscriptionError::Cancelled);
            }
            let _ = model.decode_segment(&settings, &segment, &options)?;
            Ok(())
        })
        .await
    }

    async fn unload(&self) -> Result<(), TranscriptionError> {
        let model = Arc::clone(&self.model);
        run_sherpa_on_std_thread(move || {
            model.unload();
            Ok(())
        })
        .await
    }

    fn is_model_loaded(&self) -> bool {
        self.model.is_loaded()
    }
}
