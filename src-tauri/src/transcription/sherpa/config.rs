use std::path::Path;

use sherpa_onnx::{
    OfflineQwen3ASRModelConfig, OfflineRecognizerConfig, OfflineTransducerModelConfig,
};

use crate::settings::{AppSettings, LocalSttModelKind};

#[derive(Debug, Clone, Copy)]
pub struct SherpaBuildOptions {
    pub preview: bool,
}

pub fn execution_provider(settings: &AppSettings) -> &'static str {
    if !settings.local_whisper_use_gpu {
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
    bundle_dir: &Path,
    options: SherpaBuildOptions,
) -> Result<OfflineRecognizerConfig, String> {
    let kind = settings.local_stt_model;
    let mut config = OfflineRecognizerConfig::default();
    config.feat_config.sample_rate = 16000;
    config.feat_config.feature_dim = 80;
    config.model_config.num_threads = settings.local_sherpa_num_threads.clamp(1, 16) as i32;
    config.model_config.provider = Some(execution_provider(settings).to_string());
    config.model_config.debug = false;

    match kind {
        LocalSttModelKind::ParakeetTdt06bV3 => {
            config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: Some(path_string(bundle_dir, "encoder.int8.onnx")?),
                decoder: Some(path_string(bundle_dir, "decoder.int8.onnx")?),
                joiner: Some(path_string(bundle_dir, "joiner.int8.onnx")?),
            };
            config.model_config.tokens = Some(path_string(bundle_dir, "tokens.txt")?);
            config.model_config.model_type = Some("nemo_transducer".into());
        }
        LocalSttModelKind::Qwen3Asr06b | LocalSttModelKind::Qwen3Asr17b => {
            let max_new_tokens = if options.preview { 64 } else { 128 };
            config.feat_config.feature_dim = 128;
            config.model_config.qwen3_asr = OfflineQwen3ASRModelConfig {
                conv_frontend: Some(path_string(bundle_dir, "conv_frontend.onnx")?),
                encoder: Some(path_string(bundle_dir, "encoder.int8.onnx")?),
                decoder: Some(path_string(bundle_dir, "decoder.int8.onnx")?),
                tokenizer: Some(path_string(bundle_dir, "tokenizer")?),
                max_total_len: 512,
                max_new_tokens,
                temperature: 1e-6,
                top_p: 0.8,
                seed: 42,
                hotwords: None,
            };
        }
        _ => return Err("not a sherpa STT model".to_string()),
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
