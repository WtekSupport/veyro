use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, get_lang_str};

use crate::audio::preprocess::audio_peak_rms;
use crate::audio::resampler::TARGET_SAMPLE_RATE;
use crate::audio::segment::AudioSegment;
use crate::text::dictionary::Dictionary;
use crate::text::corrections::apply_corrections;
use crate::text::normalize::{clean_raw_transcription_with_dictionary, strip_prompt_echo};
use crate::timed_text::TimedTextSegment;
use crate::transcription::models::{TranscriptionOptions, TranscriptionResult, WhisperDecodingOptions};
use crate::transcription::provider::{TranscriptionError, TranscriptionProvider};

const RETRY_MIN_DURATION_MS: u64 = 400;

#[derive(Debug, Clone, Copy)]
enum DecodeProfile {
    Normal,
    /// Short/processed mic audio often gets discarded unless thresholds are relaxed.
    Permissive,
}

#[derive(Debug, Clone)]
pub struct LocalWhisperConfig {
    pub use_gpu: bool,
    pub beam_size: u8,
}

struct SharedModel {
    model_path: PathBuf,
    config: LocalWhisperConfig,
    context: Mutex<Option<WhisperContext>>,
}

impl SharedModel {
    fn unload(&self) {
        if let Ok(mut guard) = self.context.lock() {
            *guard = None;
        }
    }

    fn is_loaded(&self) -> bool {
        self.context
            .lock()
            .ok()
            .is_some_and(|guard| guard.is_some())
    }

    fn ensure_loaded(&self) -> Result<(), TranscriptionError> {
        let mut guard = self
            .context
            .lock()
            .map_err(|_| TranscriptionError::InferenceFailed("model lock poisoned".to_string()))?;

        if guard.is_some() {
            return Ok(());
        }

        if !self.model_path.is_file() {
            return Err(TranscriptionError::ModelNotFound(
                self.model_path.display().to_string(),
            ));
        }

        let path = self
            .model_path
            .to_str()
            .ok_or_else(|| TranscriptionError::ModelNotFound("invalid model path".to_string()))?;

        let mut params = WhisperContextParameters::default();
        if crate::settings::whisper_gpu_compiled() {
            params.use_gpu = self.config.use_gpu;
        }

        let context = WhisperContext::new_with_params(path, params)
            .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?;
        *guard = Some(context);
        Ok(())
    }

    fn prewarm(&self) -> Result<(), TranscriptionError> {
        self.ensure_loaded()?;

        // ~300 ms of silence — enough to warm GPU kernels without meaningful output.
        let sample_count = (TARGET_SAMPLE_RATE as usize * 300) / 1000;
        let segment = AudioSegment::new(vec![0.0; sample_count], TARGET_SAMPLE_RATE, 1);
        let _ = self.transcribe(&segment, &TranscriptionOptions::default(), &Dictionary::default())?;
        Ok(())
    }

    fn transcribe(
        &self,
        audio: &AudioSegment,
        options: &TranscriptionOptions,
        dictionary: &Dictionary,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        self.ensure_loaded()?;

        let guard = self
            .context
            .lock()
            .map_err(|_| TranscriptionError::InferenceFailed("model lock poisoned".to_string()))?;
        let context = guard
            .as_ref()
            .ok_or_else(|| TranscriptionError::InferenceFailed("model not loaded".to_string()))?;

        let decoding = merge_decoding_options(options.whisper_decoding, dictionary);
        let beam_size = self.config.beam_size;
        let (peak, rms) = audio_peak_rms(audio);
        let meta = |segments: i32, detected: &Option<String>, timed: Vec<TimedTextSegment>| {
            TranscriptionResult {
                text: String::new(),
                confidence: None,
                whisper_segments: Some(segments),
                timed_segments: if timed.is_empty() {
                    None
                } else {
                    Some(timed)
                },
                audio_peak: Some(peak),
                audio_rms: Some(rms),
                detected_language: detected.clone(),
            }
        };

        let (text, raw_segments, segment_count, detected_language, timed_segments) = self.decode_audio(
            context,
            audio,
            options,
            &decoding,
            beam_size,
            DecodeProfile::Normal,
            true,
            None,
        )?;
        let whisper_detected = detected_language;
        let text = finalize_local_text(&text, dictionary, options.prompt.as_deref());

        if !text.is_empty() {
            debug!(
                chars = text.len(),
                segments = segment_count,
                peak,
                rms,
                "local transcription completed"
            );
            return Ok(TranscriptionResult {
                text,
                confidence: None,
                whisper_segments: Some(segment_count),
                timed_segments: if timed_segments.is_empty() {
                    None
                } else {
                    Some(timed_segments)
                },
                audio_peak: Some(peak),
                audio_rms: Some(rms),
                detected_language: whisper_detected.clone(),
            });
        }

        let should_retry = audio.duration_ms >= RETRY_MIN_DURATION_MS;
        if should_retry {
            warn!(
                duration_ms = audio.duration_ms,
                segments = segment_count,
                peak,
                rms,
                raw = %raw_segments.join(" | "),
                "local transcription empty; retrying with permissive decode"
            );
            let permissive = WhisperDecodingOptions::permissive();
            let language = options
                .language
                .clone()
                .or(whisper_detected.clone());
            let (retry_text, retry_raw, retry_segments, retry_detected, retry_timed) = self.decode_audio(
                context,
                audio,
                options,
                &permissive,
                beam_size,
                DecodeProfile::Permissive,
                false,
                language.as_deref(),
            )?;
            let whisper_detected = retry_detected.or(whisper_detected);
            let retry_text = finalize_local_text(&retry_text, dictionary, None);
            if !retry_text.is_empty() {
                debug!(
                    chars = retry_text.len(),
                    segments = retry_segments,
                    peak,
                    rms,
                    "local transcription completed after permissive retry"
                );
                return Ok(TranscriptionResult {
                    text: retry_text,
                    confidence: None,
                    whisper_segments: Some(retry_segments),
                    timed_segments: if retry_timed.is_empty() {
                        None
                    } else {
                        Some(retry_timed)
                    },
                    audio_peak: Some(peak),
                    audio_rms: Some(rms),
                    detected_language: whisper_detected.clone(),
                });
            }
            warn!(
                duration_ms = audio.duration_ms,
                segments = retry_segments,
                peak,
                rms,
                raw = %retry_raw.join(" | "),
                "local transcription still empty after permissive retry"
            );
            return Ok(meta(retry_segments, &whisper_detected, retry_timed));
        }

        warn!(
            duration_ms = audio.duration_ms,
            segments = segment_count,
            peak,
            rms,
            raw = %raw_segments.join(" | "),
            "local transcription empty"
        );
        Ok(meta(segment_count, &whisper_detected, timed_segments))
    }

    fn transcribe_preview(
        &self,
        audio: &AudioSegment,
        options: &TranscriptionOptions,
    ) -> Result<String, TranscriptionError> {
        if audio.duration_ms < crate::audio::preview::PREVIEW_MIN_AUDIO_MS {
            return Ok(String::new());
        }

        self.ensure_loaded()?;

        let dictionary = crate::text::dictionary::load_dictionary(options.dictionary_path.as_deref())
            .unwrap_or_default();
        let (peak, rms) = audio_peak_rms(audio);

        let guard = self
            .context
            .lock()
            .map_err(|_| TranscriptionError::InferenceFailed("model lock poisoned".to_string()))?;
        let context = guard
            .as_ref()
            .ok_or_else(|| TranscriptionError::InferenceFailed("model not loaded".to_string()))?;

        let permissive = merge_decoding_options(
            Some(WhisperDecodingOptions::permissive()),
            &dictionary,
        );
        let (text, raw_segments, segment_count, detected_language, _timed) = self.decode_audio(
            context,
            audio,
            options,
            &permissive,
            1,
            DecodeProfile::Permissive,
            false,
            options.language.as_deref(),
        )?;
        let mut finalized = finalize_preview_text(&text, &dictionary, None);

        if finalized.is_empty() && audio.duration_ms >= RETRY_MIN_DURATION_MS {
            let language = options.language.clone().or(detected_language);
            let (retry_text, retry_raw, retry_segments, _, _retry_timed) = self.decode_audio(
                context,
                audio,
                options,
                &permissive,
                1,
                DecodeProfile::Permissive,
                false,
                language.as_deref(),
            )?;
            finalized = finalize_preview_text(&retry_text, &dictionary, None);
            if finalized.is_empty() {
                warn!(
                    duration_ms = audio.duration_ms,
                    peak,
                    rms,
                    segments = retry_segments,
                    raw = %retry_raw.join(" | "),
                    "live preview whisper returned empty after retry"
                );
            }
        } else if finalized.is_empty() {
            warn!(
                duration_ms = audio.duration_ms,
                peak,
                rms,
                segments = segment_count,
                raw = %raw_segments.join(" | "),
                "live preview whisper returned empty"
            );
        }

        Ok(finalized)
    }

    fn decode_audio(
        &self,
        context: &WhisperContext,
        audio: &AudioSegment,
        options: &TranscriptionOptions,
        decoding: &WhisperDecodingOptions,
        beam_size: u8,
        profile: DecodeProfile,
        use_initial_prompt: bool,
        language_override: Option<&str>,
    ) -> Result<(String, Vec<String>, i32, Option<String>, Vec<TimedTextSegment>), TranscriptionError>
    {
        let mut state = context
            .create_state()
            .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?;

        let mut params = FullParams::new(sampling_strategy(beam_size));
        match language_override.or(options.language.as_deref()) {
            Some(language) => params.set_language(Some(language)),
            None => params.set_detect_language(true),
        }
        if use_initial_prompt {
            if let Some(prompt) = options.prompt.as_deref() {
                params.set_initial_prompt(prompt);
            }
        }
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_entropy_thold(decoding.entropy_thold);
        params.set_logprob_thold(decoding.logprob_thold);
        params.set_temperature_inc(decoding.temperature_inc);
        params.set_suppress_nst(true);

        match profile {
            DecodeProfile::Normal => {
                params.set_no_speech_thold(0.6);
            }
            DecodeProfile::Permissive => {
                params.set_no_speech_thold(0.99);
                params.set_single_segment(true);
                params.set_suppress_blank(false);
            }
        }

        state
            .full(params, &audio.samples)
            .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?;

        let segment_count = state
            .full_n_segments()
            .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?;

        let detected_language = state
            .full_lang_id_from_state()
            .ok()
            .and_then(|lang_id| get_lang_str(lang_id).map(str::to_string));

        let mut raw_segments = Vec::new();
        let mut timed_segments = Vec::new();
        let mut text = String::new();
        for index in 0..segment_count {
            let segment = state
                .full_get_segment_text(index)
                .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?;
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                continue;
            }
            let start_ms = state
                .full_get_segment_t0(index)
                .map(whisper_timestamp_to_ms)
                .unwrap_or(0);
            let end_ms = state
                .full_get_segment_t1(index)
                .map(whisper_timestamp_to_ms)
                .unwrap_or(start_ms);
            timed_segments.push(TimedTextSegment {
                text: trimmed.to_string(),
                start_ms,
                end_ms,
            });
            raw_segments.push(trimmed.to_string());
            text.push_str(trimmed);
            text.push(' ');
        }

        Ok((
            text.trim().to_string(),
            raw_segments,
            segment_count,
            detected_language,
            timed_segments,
        ))
    }
}

fn whisper_timestamp_to_ms(timestamp: i64) -> u64 {
    timestamp.max(0) as u64 * 10
}

fn sampling_strategy(beam_size: u8) -> SamplingStrategy {
    if beam_size <= 1 {
        SamplingStrategy::Greedy { best_of: 1 }
    } else {
        SamplingStrategy::BeamSearch {
            beam_size: beam_size as i32,
            patience: -1.0,
        }
    }
}

fn finalize_local_text(text: &str, dictionary: &Dictionary, prompt: Option<&str>) -> String {
    let text = clean_raw_transcription_with_dictionary(text.trim(), dictionary);
    strip_prompt_echo(&text, prompt)
}

fn finalize_preview_text(text: &str, dictionary: &Dictionary, prompt: Option<&str>) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let text = apply_corrections(text, &dictionary.corrections);
    strip_prompt_echo(&text, prompt)
}

fn merge_decoding_options(
    options: Option<WhisperDecodingOptions>,
    dictionary: &Dictionary,
) -> WhisperDecodingOptions {
    let base = options.unwrap_or_default();
    WhisperDecodingOptions {
        logprob_thold: dictionary
            .whisper
            .logprob_thold
            .unwrap_or(base.logprob_thold),
        entropy_thold: dictionary
            .whisper
            .entropy_thold
            .unwrap_or(base.entropy_thold),
        temperature_inc: dictionary
            .whisper
            .temperature_inc
            .unwrap_or(base.temperature_inc),
    }
}

pub struct LocalWhisperProvider {
    model: Arc<SharedModel>,
    cancel: CancellationToken,
}

impl LocalWhisperProvider {
    pub fn new(model_path: PathBuf, config: LocalWhisperConfig, cancel: CancellationToken) -> Self {
        Self {
            model: Arc::new(SharedModel {
                model_path,
                config,
                context: Mutex::new(None),
            }),
            cancel,
        }
    }

    pub fn is_model_loaded(&self) -> bool {
        self.model.is_loaded()
    }
}

#[async_trait]
impl TranscriptionProvider for LocalWhisperProvider {
    async fn transcribe(
        &self,
        audio: AudioSegment,
        options: TranscriptionOptions,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        if self.cancel.is_cancelled() {
            return Err(TranscriptionError::Cancelled);
        }

        let model = Arc::clone(&self.model);
        let cancel = self.cancel.clone();
        let dictionary_path = options.dictionary_path.clone();

        tokio::task::spawn_blocking(move || {
            if cancel.is_cancelled() {
                return Err(TranscriptionError::Cancelled);
            }
            let dictionary = crate::text::dictionary::load_dictionary(dictionary_path.as_deref())
                .unwrap_or_default();
            model.transcribe(&audio, &options, &dictionary)
        })
        .await
        .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?
    }

    async fn prewarm(&self) -> Result<(), TranscriptionError> {
        if self.cancel.is_cancelled() {
            return Err(TranscriptionError::Cancelled);
        }

        let model = Arc::clone(&self.model);
        let cancel = self.cancel.clone();

        tokio::task::spawn_blocking(move || {
            if cancel.is_cancelled() {
                return Err(TranscriptionError::Cancelled);
            }
            model.prewarm()
        })
        .await
        .map_err(|error| TranscriptionError::InferenceFailed(error.to_string()))?
    }

    async fn transcribe_preview(
        &self,
        audio: AudioSegment,
        options: TranscriptionOptions,
    ) -> Result<Option<String>, TranscriptionError> {
        self.transcribe_preview_sync(audio, options)
    }

    fn transcribe_preview_sync(
        &self,
        audio: AudioSegment,
        options: TranscriptionOptions,
    ) -> Result<Option<String>, TranscriptionError> {
        if self.cancel.is_cancelled() {
            return Err(TranscriptionError::Cancelled);
        }
        let text = self.model.transcribe_preview(&audio, &options)?;
        Ok(if text.is_empty() { None } else { Some(text) })
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

#[cfg(all(test, feature = "local-whisper"))]
mod tests {
    use super::*;

    #[test]
    fn shared_model_unload_clears_context() {
        let model = SharedModel {
            model_path: PathBuf::from("missing-model.bin"),
            config: LocalWhisperConfig {
                use_gpu: false,
                beam_size: 1,
            },
            context: Mutex::new(None),
        };

        assert!(!model.is_loaded());
        model.unload();
        assert!(!model.is_loaded());
        assert!(model.context.lock().unwrap().is_none());
    }
}
