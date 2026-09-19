use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sherpa_onnx::OfflineRecognizer;
use tokio_util::sync::CancellationToken;

use crate::audio::resampler::TARGET_SAMPLE_RATE;
use crate::audio::segment::AudioSegment;
use crate::settings::AppSettings;
use crate::transcription::local_stt_model_store::{bundle_ready, sherpa_bundle_path};
use crate::transcription::models::{TranscriptionOptions, TranscriptionResult};
use crate::transcription::provider::{TranscriptionError, TranscriptionProvider};
use crate::transcription::sherpa::{build_offline_config, SherpaBuildOptions};

struct SharedSherpa {
    bundle_dir: PathBuf,
    settings_snapshot: SherpaSettingsSnapshot,
    recognizer: Mutex<Option<OfflineRecognizer>>,
    loaded_preview: Mutex<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SherpaSettingsSnapshot {
    model: crate::settings::LocalSttModelKind,
    use_gpu: bool,
    num_threads: u32,
}

impl SharedSherpa {
    fn from_settings(settings: &AppSettings, bundle_dir: PathBuf) -> Self {
        Self {
            bundle_dir,
            settings_snapshot: SherpaSettingsSnapshot {
                model: settings.local_stt_model,
                use_gpu: settings.local_whisper_use_gpu,
                num_threads: settings.local_sherpa_num_threads,
            },
            recognizer: Mutex::new(None),
            loaded_preview: Mutex::new(None),
        }
    }

    fn settings_match(&self, settings: &AppSettings) -> bool {
        self.settings_snapshot.model == settings.local_stt_model
            && self.settings_snapshot.use_gpu == settings.local_whisper_use_gpu
            && self.settings_snapshot.num_threads == settings.local_sherpa_num_threads
    }

    fn unload(&self) {
        if let Ok(mut guard) = self.recognizer.lock() {
            *guard = None;
        }
        if let Ok(mut preview) = self.loaded_preview.lock() {
            *preview = None;
        }
    }

    fn is_loaded(&self) -> bool {
        self.recognizer
            .lock()
            .ok()
            .is_some_and(|guard| guard.is_some())
    }

    fn ensure_loaded(
        &self,
        settings: &AppSettings,
        preview: bool,
    ) -> Result<(), TranscriptionError> {
        let mut guard = self
            .recognizer
            .lock()
            .map_err(|_| TranscriptionError::InferenceFailed("sherpa lock poisoned".to_string()))?;

        let same_preview = self
            .loaded_preview
            .lock()
            .ok()
            .and_then(|p| *p)
            .is_some_and(|loaded| loaded == preview);
        if guard.is_some() && self.settings_match(settings) && same_preview {
            return Ok(());
        }

        if !bundle_ready(&self.bundle_dir, settings.local_stt_model) {
            return Err(TranscriptionError::ModelNotFound(
                self.bundle_dir.display().to_string(),
            ));
        }

        let config = build_offline_config(
            settings,
            &self.bundle_dir,
            SherpaBuildOptions { preview },
        )
        .map_err(TranscriptionError::InferenceFailed)?;

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            TranscriptionError::InferenceFailed("failed to create sherpa recognizer".to_string())
        })?;
        *guard = Some(recognizer);
        if let Ok(mut loaded) = self.loaded_preview.lock() {
            *loaded = Some(preview);
        }
        Ok(())
    }

    fn decode_segment(
        &self,
        settings: &AppSettings,
        audio: &AudioSegment,
        options: &TranscriptionOptions,
        preview: bool,
    ) -> Result<String, TranscriptionError> {
        self.ensure_loaded(settings, preview)?;

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
        let kind = settings.local_stt_model;
        let bundle_dir = sherpa_bundle_path(&settings, kind).map_err(|error| {
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
        let settings = self.current_settings()?;
        let text = self
            .model
            .decode_segment(&settings, &audio, &options, false)?;
        Ok(TranscriptionResult {
            text,
            confidence: None,
            whisper_segments: None,
            timed_segments: None,
            audio_peak: None,
            audio_rms: None,
            detected_language: options.language.clone(),
        })
    }

    async fn prewarm(&self) -> Result<(), TranscriptionError> {
        let settings = self.current_settings()?;
        let sample_count = (TARGET_SAMPLE_RATE as usize * 300) / 1000;
        let segment = AudioSegment::new(vec![0.0; sample_count], TARGET_SAMPLE_RATE, 1);
        let _ = self.model.decode_segment(
            &settings,
            &segment,
            &TranscriptionOptions::default(),
            true,
        )?;
        Ok(())
    }

    fn transcribe_preview_sync(
        &self,
        audio: AudioSegment,
        options: TranscriptionOptions,
    ) -> Result<Option<String>, TranscriptionError> {
        let settings = self.current_settings()?;
        let text = self
            .model
            .decode_segment(&settings, &audio, &options, true)?;
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }

    async fn unload(&self) -> Result<(), TranscriptionError> {
        self.model.unload();
        Ok(())
    }

    fn is_model_loaded(&self) -> bool {
        self.model.is_loaded()
    }
}
