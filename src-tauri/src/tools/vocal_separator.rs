#![cfg(feature = "local-separation")]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tracing::warn;

use crate::app::context::AppContext;
use crate::app::events::{
    emit_vocal_separator_progress, VocalSeparatorProgressPayload, VocalSeparatorProgressPhase,
};
use crate::audio::encode::{encode_flac, encode_wav};
use crate::audio::segment::AudioSegment;
use crate::ml::onnx_provider::separation_execution_provider;
use crate::separation::model_store::{self, bundle_ready_for_settings};
use crate::settings::vocal_separator::{VocalSeparatorOutputFormat, VocalSeparatorProfile};
use crate::settings::AppSettings;
use veyro_separation::{
    load_model, separate, separate_multi_stem, AudioBuffer, SeparationError, SeparationOptions,
    SeparationProfile, SeparationWarning,
};

use super::shared::{
    decode_for_separation, throttled_percent_callback, ToolsTranscriptionGuard,
    validate_tool_file_path,
};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VocalSeparatorInvokeOptions {
    #[serde(default)]
    pub profile: Option<VocalSeparatorProfile>,
    #[serde(default)]
    pub output_format: Option<VocalSeparatorOutputFormat>,
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub normalize: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocalSeparatorResult {
    pub file_name: String,
    pub vocals_path: String,
    pub instrumental_path: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentStemResult {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitInstrumentalResult {
    pub file_name: String,
    pub stems: Vec<InstrumentStemResult>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

fn emit_phase(app: &AppHandle, path: &str, phase: VocalSeparatorProgressPhase) {
    emit_progress(app, path, phase, None, None, None, Vec::new());
}

fn emit_progress(
    app: &AppHandle,
    path: &str,
    phase: VocalSeparatorProgressPhase,
    percent: Option<u8>,
    vocals_path: Option<String>,
    instrumental_path: Option<String>,
    instrument_stems: Vec<crate::app::events::VocalSeparatorInstrumentStem>,
) {
    emit_vocal_separator_progress(
        app,
        VocalSeparatorProgressPayload {
            path: path.to_string(),
            phase,
            percent,
            vocals_path,
            instrumental_path,
            instrument_stems,
        },
    );
}

fn warning_key(warning: SeparationWarning) -> &'static str {
    match warning {
        SeparationWarning::ExecutionProviderFallback => {
            "tools.vocalSeparator.warning.executionProviderFallback"
        }
        SeparationWarning::ProfileFallbackToFast => "tools.vocalSeparator.warning.profileFallback",
    }
}

fn resolve_output_dir(
    settings: &AppSettings,
    source: &Path,
    override_dir: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(raw) = override_dir.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(raw));
    }
    if let Some(raw) = settings
        .vocal_separator_output_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(raw));
    }
    source
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "tools.vocalSeparator.invalidPath".to_string())
}

fn output_extension(source: &Path, format: VocalSeparatorOutputFormat) -> &'static str {
    match format {
        VocalSeparatorOutputFormat::Wav => "wav",
        VocalSeparatorOutputFormat::Flac => "flac",
        VocalSeparatorOutputFormat::MatchSource => {
            let ext = source
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "flac" => "flac",
                _ => "wav",
            }
        }
    }
}

fn write_stem(
    segment: &AudioSegment,
    path: &Path,
    format: VocalSeparatorOutputFormat,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = match format {
        VocalSeparatorOutputFormat::Flac => encode_flac(segment)?,
        VocalSeparatorOutputFormat::Wav | VocalSeparatorOutputFormat::MatchSource => {
            encode_wav(segment)?
        }
    };
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn run_separation(
    settings: &AppSettings,
    segment: AudioSegment,
    profile: VocalSeparatorProfile,
    normalize: bool,
    progress: Arc<dyn Fn(u8) + Send + Sync>,
) -> Result<(AudioSegment, AudioSegment, Vec<SeparationWarning>), String> {
    if !profile.is_base_profile() {
        return Err("tools.vocalSeparator.modelIncompatible".to_string());
    }
    if !bundle_ready_for_settings(settings, profile) {
        return Err("tools.vocalSeparator.modelMissing".to_string());
    }

    let input = AudioBuffer::new(segment.samples, segment.sample_rate, segment.channels);
    let mix = input
        .to_stereo_44100()
        .map_err(|error| format!("tools.vocalSeparator.separationFailed|{error}"))?;
    let expected_len = mix.samples.len();

    let run_profile = |profile: VocalSeparatorProfile| -> Result<
        (veyro_separation::SeparatedAudio, Vec<SeparationWarning>),
        String,
    > {
        let model_path = model_store::model_path(settings, profile).map_err(|e| e.to_string())?;
        if !bundle_ready_for_settings(settings, profile) {
            return Err("tools.vocalSeparator.modelMissing".to_string());
        }
        let model = load_model(&model_path, SeparationProfile::from(profile)).map_err(|error| {
            match error {
                SeparationError::IncompatibleModel => {
                    "tools.vocalSeparator.modelIncompatible".to_string()
                }
                other => format!("tools.vocalSeparator.modelLoadFailed|{other}"),
            }
        })?;
        let options = SeparationOptions {
            profile: SeparationProfile::from(profile),
            normalize_output: false,
            execution_provider: separation_execution_provider(settings),
        };
        separate(&input, &model, &options, progress.clone()).map_err(|error| match error {
            SeparationError::IncompatibleModel => {
                "tools.vocalSeparator.modelIncompatible".to_string()
            }
            other if other.to_string().contains("stft_features") => {
                "tools.vocalSeparator.modelIncompatible".to_string()
            }
            other => format!("tools.vocalSeparator.separationFailed|{other}"),
        })
    };

    let (separated, warnings) = match run_profile(profile) {
        Ok(result) => result,
        Err(error) if error.contains("OutOfMemory") || error.contains("out of memory") => {
            if profile == VocalSeparatorProfile::Fast {
                return Err("tools.vocalSeparator.outOfMemory".to_string());
            }
            warn!("separation OOM on {profile:?}, retrying fast profile");
            let mut result = run_profile(VocalSeparatorProfile::Fast)?;
            result.1.push(SeparationWarning::ProfileFallbackToFast);
            result
        }
        Err(error) => return Err(error),
    };

    let mut vocals = AudioSegment::new(
        separated.vocals.samples,
        separated.vocals.sample_rate,
        separated.vocals.channels,
    );
    align_stereo_segment(&mut vocals, expected_len);

    // Instrumental is always mix − vocals at the model rate (44.1 kHz stereo).
    let instrumental_samples: Vec<f32> = mix
        .samples
        .iter()
        .zip(vocals.samples.iter())
        .map(|(m, v)| m - v)
        .collect();
    let mut instrumental = AudioSegment::new(instrumental_samples, 44_100, 2);

    if normalize {
        normalize_peak(&mut vocals);
        normalize_peak(&mut instrumental);
    }

    Ok((vocals, instrumental, warnings))
}

fn align_stereo_segment(segment: &mut AudioSegment, expected_interleaved_len: usize) {
    if segment.samples.len() > expected_interleaved_len {
        segment.samples.truncate(expected_interleaved_len);
    } else if segment.samples.len() < expected_interleaved_len {
        segment.samples.resize(expected_interleaved_len, 0.0);
    }
    segment.sample_rate = 44_100;
    segment.channels = 2;
}

fn normalize_peak(segment: &mut AudioSegment) {
    let peak = segment
        .samples
        .iter()
        .copied()
        .fold(0.0f32, |acc, v| acc.max(v.abs()));
    if peak <= 1e-6 {
        return;
    }
    let scale = 0.99 / peak;
    for sample in &mut segment.samples {
        *sample *= scale;
    }
}

pub async fn separate_vocal_file(
    app: AppHandle,
    ctx: Arc<AppContext>,
    path: String,
    options: VocalSeparatorInvokeOptions,
) -> Result<VocalSeparatorResult, String> {
    let _guard = ToolsTranscriptionGuard::try_begin(&ctx)?;

    let (path_key, path_buf, file_name) = validate_tool_file_path(&path, "tools.vocalSeparator")?;

    let settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();

    let profile = options.profile.unwrap_or(settings.vocal_separator_profile);
    let output_format = options
        .output_format
        .unwrap_or(settings.vocal_separator_output_format);
    let normalize = options
        .normalize
        .unwrap_or(settings.vocal_separator_normalize);
    let output_dir_override = options.output_dir.clone();

    let app_for_progress = app.clone();
    let path_key_for_progress = path_key.clone();
    let path_buf_for_work = path_buf.clone();
    let settings_for_work = settings.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        emit_phase(
            &app_for_progress,
            &path_key_for_progress,
            VocalSeparatorProgressPhase::Decoding,
        );
        let decode_progress = throttled_percent_callback(Arc::new({
            let app = app_for_progress.clone();
            let path_key = path_key_for_progress.clone();
            move |percent: u8| {
                emit_progress(
                    &app,
                    &path_key,
                    VocalSeparatorProgressPhase::Decoding,
                    Some(percent),
                    None,
                    None,
                    Vec::new(),
                );
            }
        }));
        let (segment, _source_rate) = decode_for_separation(
            &path_buf_for_work,
            Some(decode_progress),
            "tools.vocalSeparator",
        )?;

        emit_phase(
            &app_for_progress,
            &path_key_for_progress,
            VocalSeparatorProgressPhase::Separating,
        );
        let separate_progress = throttled_percent_callback(Arc::new({
            let app = app_for_progress.clone();
            let path_key = path_key_for_progress.clone();
            move |percent: u8| {
                emit_progress(
                    &app,
                    &path_key,
                    VocalSeparatorProgressPhase::Separating,
                    Some(percent),
                    None,
                    None,
                    Vec::new(),
                );
            }
        }));

        let (vocals, instrumental, warnings) = run_separation(
            &settings_for_work,
            segment,
            profile,
            normalize,
            separate_progress,
        )?;

        emit_progress(
            &app_for_progress,
            &path_key_for_progress,
            VocalSeparatorProgressPhase::Writing,
            Some(100),
            None,
            None,
            Vec::new(),
        );
        let output_dir = resolve_output_dir(
            &settings_for_work,
            &path_buf_for_work,
            output_dir_override.as_deref(),
        )?;
        let stem = path_buf_for_work
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("output");
        let ext = output_extension(&path_buf_for_work, output_format);
        let encode_format = match output_format {
            VocalSeparatorOutputFormat::MatchSource if ext == "flac" => {
                VocalSeparatorOutputFormat::Flac
            }
            _ => VocalSeparatorOutputFormat::Wav,
        };

        let vocals_path = output_dir.join(format!("{stem}_vocals.{ext}"));
        let instrumental_path = output_dir.join(format!("{stem}_instrumental.{ext}"));
        write_stem(&vocals, &vocals_path, encode_format)?;
        write_stem(&instrumental, &instrumental_path, encode_format)?;

        let vocals_path_str = vocals_path.display().to_string();
        let instrumental_path_str = instrumental_path.display().to_string();

        emit_progress(
            &app_for_progress,
            &path_key_for_progress,
            VocalSeparatorProgressPhase::Done,
            Some(100),
            Some(vocals_path_str.clone()),
            Some(instrumental_path_str.clone()),
            Vec::new(),
        );

        Ok::<VocalSeparatorResult, String>(VocalSeparatorResult {
            file_name: path_buf_for_work
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&file_name)
                .to_string(),
            vocals_path: vocals_path_str,
            instrumental_path: instrumental_path_str,
            warnings: warnings
                .into_iter()
                .map(|warning| warning_key(warning).to_string())
                .collect(),
        })
    })
    .await
    .map_err(|error| format!("tools.vocalSeparator.separationFailed|{error}"))?;

    result
}

/// Second independent inference on the **original mix** (not the instrumental file).
/// Keeps base `_instrumental` untouched; discards `htdemucs_6s` vocals stem.
pub async fn split_instrumental_further(
    app: AppHandle,
    ctx: Arc<AppContext>,
    path: String,
    options: VocalSeparatorInvokeOptions,
) -> Result<SplitInstrumentalResult, String> {
    let _guard = ToolsTranscriptionGuard::try_begin(&ctx)?;

    let (path_key, path_buf, file_name) = validate_tool_file_path(&path, "tools.vocalSeparator")?;

    let settings = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?
        .settings()
        .clone();

    let output_format = options
        .output_format
        .unwrap_or(settings.vocal_separator_output_format);
    let normalize = options
        .normalize
        .unwrap_or(settings.vocal_separator_normalize);
    let output_dir_override = options.output_dir.clone();

    if !bundle_ready_for_settings(&settings, VocalSeparatorProfile::MultiStem) {
        return Err("tools.vocalSeparator.multiStemModelMissing".to_string());
    }

    let app_for_progress = app.clone();
    let path_key_for_progress = path_key.clone();
    let path_buf_for_work = path_buf.clone();
    let settings_for_work = settings.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        emit_phase(
            &app_for_progress,
            &path_key_for_progress,
            VocalSeparatorProgressPhase::Decoding,
        );
        let decode_progress = throttled_percent_callback(Arc::new({
            let app = app_for_progress.clone();
            let path_key = path_key_for_progress.clone();
            move |percent: u8| {
                emit_progress(
                    &app,
                    &path_key,
                    VocalSeparatorProgressPhase::Decoding,
                    Some(percent),
                    None,
                    None,
                    Vec::new(),
                );
            }
        }));
        let (segment, _source_rate) = decode_for_separation(
            &path_buf_for_work,
            Some(decode_progress),
            "tools.vocalSeparator",
        )?;

        emit_phase(
            &app_for_progress,
            &path_key_for_progress,
            VocalSeparatorProgressPhase::SplittingInstruments,
        );
        let separate_progress = throttled_percent_callback(Arc::new({
            let app = app_for_progress.clone();
            let path_key = path_key_for_progress.clone();
            move |percent: u8| {
                emit_progress(
                    &app,
                    &path_key,
                    VocalSeparatorProgressPhase::SplittingInstruments,
                    Some(percent),
                    None,
                    None,
                    Vec::new(),
                );
            }
        }));

        let input = AudioBuffer::new(segment.samples, segment.sample_rate, segment.channels);
        let model_path = model_store::model_path(&settings_for_work, VocalSeparatorProfile::MultiStem)
            .map_err(|e| e.to_string())?;
        let model = load_model(&model_path, SeparationProfile::MultiStem).map_err(|error| {
            match error {
                SeparationError::IncompatibleModel => {
                    "tools.vocalSeparator.modelIncompatible".to_string()
                }
                other => format!("tools.vocalSeparator.modelLoadFailed|{other}"),
            }
        })?;
        let sep_options = SeparationOptions {
            profile: SeparationProfile::MultiStem,
            normalize_output: false,
            execution_provider: separation_execution_provider(&settings_for_work),
        };
        let (multi, warnings) =
            separate_multi_stem(&input, &model, &sep_options, separate_progress).map_err(
                |error| match error {
                    SeparationError::IncompatibleModel => {
                        "tools.vocalSeparator.modelIncompatible".to_string()
                    }
                    other => format!("tools.vocalSeparator.separationFailed|{other}"),
                },
            )?;

        let output_dir = resolve_output_dir(
            &settings_for_work,
            &path_buf_for_work,
            output_dir_override.as_deref(),
        )?;
        let stem = path_buf_for_work
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("output");
        let ext = output_extension(&path_buf_for_work, output_format);
        let encode_format = match output_format {
            VocalSeparatorOutputFormat::MatchSource if ext == "flac" => {
                VocalSeparatorOutputFormat::Flac
            }
            _ => VocalSeparatorOutputFormat::Wav,
        };

        let mut stem_results = Vec::with_capacity(multi.stems.len());
        for (name, buffer) in multi.stems {
            let mut segment =
                AudioSegment::new(buffer.samples, buffer.sample_rate, buffer.channels);
            if normalize {
                normalize_peak(&mut segment);
            }
            let out_path = output_dir.join(format!("{stem}_{name}.{ext}"));
            write_stem(&segment, &out_path, encode_format)?;
            stem_results.push(InstrumentStemResult {
                name,
                path: out_path.display().to_string(),
            });
        }

        let instrument_stems: Vec<_> = stem_results
            .iter()
            .map(|s| crate::app::events::VocalSeparatorInstrumentStem {
                name: s.name.clone(),
                path: s.path.clone(),
            })
            .collect();

        emit_progress(
            &app_for_progress,
            &path_key_for_progress,
            VocalSeparatorProgressPhase::Done,
            Some(100),
            None,
            None,
            instrument_stems,
        );

        Ok::<SplitInstrumentalResult, String>(SplitInstrumentalResult {
            file_name: path_buf_for_work
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&file_name)
                .to_string(),
            stems: stem_results,
            warnings: warnings
                .into_iter()
                .map(|warning| warning_key(warning).to_string())
                .collect(),
        })
    })
    .await
    .map_err(|error| format!("tools.vocalSeparator.separationFailed|{error}"))?;

    result
}
