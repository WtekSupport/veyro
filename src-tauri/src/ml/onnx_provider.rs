use crate::settings::AppSettings;

/// ONNX Runtime execution provider for Veyro-local models (Sherpa STT, separation, etc.).
#[cfg_attr(
    not(any(feature = "local-separation", feature = "local-sherpa-stt")),
    allow(dead_code)
)]
pub fn execution_provider(settings: &AppSettings) -> &'static str {
    if let Ok(raw) = std::env::var("VEYRO_SHERPA_PROVIDER") {
        return match raw.to_ascii_lowercase().as_str() {
            "cuda" => "cuda",
            "directml" => "directml",
            "coreml" => "coreml",
            _ => "cpu",
        };
    }

    if let Ok(raw) = std::env::var("VEYRO_SEPARATION_PROVIDER") {
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

    #[cfg(windows)]
    {
        let layout = crate::settings::variant_spec(settings.local_stt_variant()).sherpa_layout;
        if layout == Some(crate::settings::SherpaOnnxLayout::Qwen3Int8) {
            return "cpu";
        }
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

#[cfg(feature = "local-separation")]
pub fn separation_execution_provider(
    settings: &AppSettings,
) -> veyro_separation::ExecutionProvider {
    use veyro_separation::ExecutionProvider;
    match execution_provider(settings) {
        "cuda" => ExecutionProvider::Cuda,
        "directml" => ExecutionProvider::DirectMl,
        "coreml" => ExecutionProvider::CoreMl,
        _ => ExecutionProvider::Cpu,
    }
}
