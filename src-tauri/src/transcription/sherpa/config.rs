use std::path::Path;

use sherpa_onnx::{
    OfflineQwen3ASRModelConfig, OfflineRecognizerConfig, OfflineTransducerModelConfig,
};

use crate::settings::{AppSettings, SherpaOnnxLayout};
use crate::transcription::local_stt_model_store::{effective_sherpa_bundle_dir, required_sherpa_files};

pub fn execution_provider(settings: &AppSettings) -> &'static str {
    if let Ok(raw) = std::env::var("VEYRO_SHERPA_PROVIDER") {
        return match raw.to_ascii_lowercase().as_str() {
            "cuda" => "cuda",
            "directml" => "directml",
            "coreml" => "coreml",
            _ => "cpu",
        };
    }

    if !settings.local_whisper_use_gpu {
        return "cpu";
    }

    let layout = crate::settings::variant_spec(settings.local_stt_variant()).sherpa_layout;
    // Qwen3 ASR + DirectML has caused full-process crashes during decode on Windows.
    #[cfg(windows)]
    if layout == Some(SherpaOnnxLayout::Qwen3Int8) {
        return "cpu";
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return "coreml";
    }
    #[cfg(all(windows, feature = "local-sherpa-directml"))]
    {
        return "directml";
    }
    #[cfg(feature = "local-sherpa-cuda")]
    {
        return "cuda";
    }
    "cpu"
}

pub fn build_offline_config(
    settings: &AppSettings,
    _bundle_dir: &Path,
) -> Result<OfflineRecognizerConfig, String> {
    let variant = settings.local_stt_variant();
    let bundle_dir = effective_sherpa_bundle_dir(settings, variant).map_err(|e| e.to_string())?;
    let layout = crate::settings::variant_spec(variant)
        .sherpa_layout
        .ok_or_else(|| "not a sherpa STT variant".to_string())?;
    let mut config = OfflineRecognizerConfig::default();
    config.feat_config.sample_rate = 16000;
    config.feat_config.feature_dim = 80;
    config.model_config.num_threads = settings.local_sherpa_num_threads.clamp(1, 16) as i32;
    config.model_config.provider = Some(execution_provider(settings).to_string());
    config.model_config.debug = false;

    for relative in required_sherpa_files(variant) {
        path_string(&bundle_dir, relative)?;
    }

    match layout {
        SherpaOnnxLayout::NemoInt8 => {
            config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: Some(path_string(&bundle_dir, "encoder.int8.onnx")?),
                decoder: Some(path_string(&bundle_dir, "decoder.int8.onnx")?),
                joiner: Some(path_string(&bundle_dir, "joiner.int8.onnx")?),
            };
            config.model_config.tokens = Some(path_string(&bundle_dir, "tokens.txt")?);
            config.model_config.model_type = Some("nemo_transducer".into());
        }
        SherpaOnnxLayout::NemoFpOnnx => {
            config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: Some(path_string(&bundle_dir, "encoder.onnx")?),
                decoder: Some(path_string(&bundle_dir, "decoder.onnx")?),
                joiner: Some(path_string(&bundle_dir, "joiner.onnx")?),
            };
            config.model_config.tokens = Some(path_string(&bundle_dir, "tokens.txt")?);
            config.model_config.model_type = Some("nemo_transducer".into());
        }
        SherpaOnnxLayout::Qwen3Int8 => {
            let max_new_tokens = 128;
            config.feat_config.feature_dim = 128;
            config.model_config.qwen3_asr = OfflineQwen3ASRModelConfig {
                conv_frontend: Some(path_string(&bundle_dir, "conv_frontend.onnx")?),
                encoder: Some(path_string(&bundle_dir, "encoder.int8.onnx")?),
                decoder: Some(path_string(&bundle_dir, "decoder.int8.onnx")?),
                tokenizer: Some(path_string(&bundle_dir, "tokenizer")?),
                max_total_len: 512,
                max_new_tokens,
                temperature: 1e-6,
                top_p: 0.8,
                seed: 42,
                hotwords: None,
            };
        }
    }

    Ok(config)
}

fn path_string(base: &Path, name: &str) -> Result<String, String> {
    let path = base.join(name);
    if !path.exists() {
        return Err(format!("missing model file: {}", path.display()));
    }
    Ok(path.display().to_string())
}
