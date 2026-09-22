use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sherpa_onnx::OfflineRecognizer;
use tokio_util::sync::CancellationToken;

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
}

impl SharedSherpa {
    fn from_settings(settings: &AppSettings, bundle_dir: PathBuf) -> Self {
        Self {
            bundle_dir,
            settings_snapshot: SherpaSettingsSnapshot {
                variant: settings.local_stt_variant(),
                use_gpu: settings.local_whisper_use_gpu,
                num_threads: settings.local_sherpa_num_threads,
            },
            recognizer: Mutex::new(None),
        }
    }

    fn settings_match(&self, settings: &AppSettings) -> bool {
        self.settings_snapshot.variant == settings.local_stt_variant()
            && self.settings_snapshot.use_gpu == settings.local_whisper_use_gpu
            && self.settings_snapshot.num_threads == settings.local_sherpa_num_threads
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
        self.ensure_loaded(settings)?;

        let guard = self
            .recognizer
            .lock()
            .map_err(|_| TranscriptionError::InferenceFailed("sherpa lock poisoned".to_string()))?;
        let recognizer = guard.as_ref().ok_or_else(|| {
            TranscriptionError::InferenceFailed("sherpa recognizer not loaded".to_string())
        })?;

        let stream = recognizer.create_stream();
        if let Some(lang) = options.language.as_deref().filter(|v| !v.is_empty()) {
            stream.set_option("language", lang);
        }
        stream.accept_waveform(audio.sample_rate as i32, &audio.samples);
        recognizer.decode(&stream);
        let result = stream.get_result().ok_or_else(|| {
            TranscriptionError::InferenceFailed("sherpa decode returned no result".to_string())
        })?;
        Ok(result.text.trim().to_string())
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

        tokio::task::spawn_blocking(move || {
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
        .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?
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

        tokio::task::spawn_blocking(move || {
            if cancel.is_cancelled() {
                return Err(TranscriptionError::Cancelled);
            }
            let _ = model.decode_segment(&settings, &segment, &options)?;
            Ok(())
        })
        .await
        .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?
    }

    async fn unload(&self) -> Result<(), TranscriptionError> {
        let model = Arc::clone(&self.model);
        tokio::task::spawn_blocking(move || {
            model.unload();
            Ok(())
        })
        .await
        .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?
    }

    fn is_model_loaded(&self) -> bool {
        self.model.is_loaded()
    }
}
