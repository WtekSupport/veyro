use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::audio::device::list_devices;
use crate::error::ConfigError;
use crate::llm::ai_rewrite_available;
use crate::settings::config::{AppSettings, LlmModelKind, TextProcessingMode, UiLocale};
use crate::settings::homemaker::normalize_homemaker_settings;
use crate::setup::apply_homemaker_local_recommendations;
use crate::settings::{local_llm_gpu_compiled, whisper_gpu_compiled};
use crate::settings::encryption::{decrypt_json, encrypt_json, EncryptedEnvelope};
use crate::settings::secrets::has_api_key;

const CONFIG_FILE: &str = "config.json";

pub fn config_path() -> Result<PathBuf, ConfigError> {
    let base = dirs::config_dir().ok_or_else(|| {
        ConfigError::Read("unable to resolve OS config directory".to_string())
    })?;
    Ok(base.join("Veyro").join(CONFIG_FILE))
}

pub fn load_settings() -> Result<AppSettings, ConfigError> {
    let path = config_path()?;
    if !path.exists() {
        let mut settings = AppSettings::default();
        normalize_api_key_dependent_settings(&mut settings);
        normalize_custom_skill_settings(&mut settings);
        normalize_gpu_settings(&mut settings);
        normalize_local_llm_gpu_settings(&mut settings);
        normalize_homemaker_settings(&mut settings);
        normalize_locale_dependent_settings(&mut settings);
        if settings.is_homemaker() && settings.transcription_provider == "local" {
            apply_homemaker_local_recommendations(&mut settings);
            normalize_homemaker_settings(&mut settings);
        }
        settings.validate()?;
        return Ok(settings);
    }

    let contents = fs::read_to_string(&path)
        .map_err(|error| ConfigError::Read(format!("{}: {error}", path.display())))?;

    let (mut settings, migrated_from_plaintext) = parse_settings_contents(&contents)?;
    let sanitized = sanitize_microphone_device(&mut settings);
    let normalized = normalize_api_key_dependent_settings(&mut settings);
    let skill_normalized = normalize_custom_skill_settings(&mut settings);
    let gpu_normalized = normalize_gpu_settings(&mut settings);
    let llm_gpu_normalized = normalize_local_llm_gpu_settings(&mut settings);
    let homemaker_normalized = normalize_homemaker_settings(&mut settings);
    let locale_normalized = normalize_locale_dependent_settings(&mut settings);
    settings.validate()?;

    if migrated_from_plaintext
        || sanitized
        || normalized
        || skill_normalized
        || gpu_normalized
        || llm_gpu_normalized
        || homemaker_normalized
        || locale_normalized
    {
        save_settings(&settings)?;
    }

    Ok(settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<(), ConfigError> {
    settings.validate()?;
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ConfigError::Write(format!("{}: {error}", parent.display())))?;
    }

    let plaintext = serde_json::to_string(settings)
        .map_err(|error| ConfigError::Write(error.to_string()))?;
    let envelope = encrypt_json(&plaintext)?;
    let contents = serde_json::to_string_pretty(&envelope)
        .map_err(|error| ConfigError::Write(error.to_string()))?;
    fs::write(&path, contents)
        .map_err(|error| ConfigError::Write(format!("{}: {error}", path.display())))
}

fn parse_settings_contents(contents: &str) -> Result<(AppSettings, bool), ConfigError> {
    if let Ok(envelope) = serde_json::from_str::<EncryptedEnvelope>(contents) {
        if envelope.v > 0 {
            let plaintext = decrypt_json(&envelope)?;
            let settings = parse_settings_json(&plaintext)?;
            return Ok((settings, false));
        }
    }

    let settings = parse_settings_json(contents)?;
    Ok((settings, true))
}

fn parse_settings_json(contents: &str) -> Result<AppSettings, ConfigError> {
    let mut value: Value = serde_json::from_str(contents)
        .map_err(|error| ConfigError::Invalid(error.to_string()))?;

    migrate_legacy_fields(&mut value);

    serde_json::from_value(value).map_err(|error| ConfigError::Invalid(error.to_string()))
}

fn normalize_gpu_settings(settings: &mut AppSettings) -> bool {
    if !whisper_gpu_compiled() {
        if settings.local_whisper_use_gpu {
            settings.local_whisper_use_gpu = false;
            return true;
        }
        return false;
    }

    if settings.local_whisper_gpu_autodetected {
        return false;
    }

    settings.local_whisper_gpu_autodetected = true;
    if !settings.local_whisper_use_gpu {
        settings.local_whisper_use_gpu = true;
        return true;
    }

    true
}

fn normalize_custom_skill_settings(settings: &mut AppSettings) -> bool {
    if settings.text_processing_mode.requires_custom_skill()
        && settings
            .ai_rewrite_skill
            .as_deref()
            .is_none_or(str::is_empty)
    {
        settings.text_processing_mode = TextProcessingMode::Optimization;
        return true;
    }

    false
}

pub fn normalize_locale_dependent_settings(settings: &mut AppSettings) -> bool {
    if settings.ui_locale != UiLocale::Ru
        && settings.local_llm_model == LlmModelKind::TLiteIt21
    {
        settings.local_llm_model = LlmModelKind::Qwen3_4B;
        return true;
    }

    false
}

fn normalize_api_key_dependent_settings(settings: &mut AppSettings) -> bool {
    let mut changed = false;

    if !has_api_key() && settings.uses_openai_transcription() {
        settings.transcription_provider = "local".to_string();
        changed = true;
    }

    if settings.text_processing_mode.uses_ai() && !ai_rewrite_available(settings) {
        settings.text_processing_mode = settings.canonical_light_cleanup_mode();
        changed = true;
    }

    changed
}

fn normalize_local_llm_gpu_settings(settings: &mut AppSettings) -> bool {
    if !local_llm_gpu_compiled() {
        if settings.local_llm_use_gpu {
            settings.local_llm_use_gpu = false;
            return true;
        }
        return false;
    }

    if settings.local_llm_gpu_autodetected {
        return false;
    }

    settings.local_llm_gpu_autodetected = true;
    if !settings.local_llm_use_gpu {
        settings.local_llm_use_gpu = true;
        return true;
    }

    true
}

fn sanitize_microphone_device(settings: &mut AppSettings) -> bool {
    let before = settings.microphone_device.clone();

    if settings
        .microphone_device
        .as_deref()
        .is_some_and(str::is_empty)
    {
        settings.microphone_device = None;
    }

    if let Some(name) = settings.microphone_device.clone() {
        match list_devices() {
            Ok(devices) if devices.iter().any(|device| device.name == name) => {}
            Ok(_) => settings.microphone_device = None,
            Err(_) => {}
        }
    }

    before != settings.microphone_device
}

fn migrate_legacy_fields(value: &mut Value) {
    if value.get("text_processing_mode").is_none() {
        let mode = value
            .get("text_cleanup")
            .and_then(|value| value.as_bool())
            .map(|enabled| {
                if enabled {
                    "basic"
                } else {
                    "original"
                }
            })
            .unwrap_or("basic");
        value["text_processing_mode"] = Value::String(mode.to_string());
    }

    if let Some(device) = value.get("microphone_device") {
        if device.is_null() || device.as_str().is_some_and(str::is_empty) {
            value["microphone_device"] = Value::Null;
        }
    }

    if value.get("local_whisper_model").is_none() {
        value["local_whisper_model"] = Value::String("base".to_string());
    }

    if value.get("text_rewrite_provider").is_none() {
        value["text_rewrite_provider"] = Value::String("openai".to_string());
    }

    if value.get("ui_mode").is_none() {
        value["ui_mode"] = Value::String("expert".to_string());
    }

    if value.get("local_llm_model").is_none() {
        value["local_llm_model"] = Value::String("qwen3_4b".to_string());
    } else if let Some(model) = value.get("local_llm_model").and_then(|value| value.as_str()) {
        let normalized = match model {
            "qwen3_4_b" => "qwen3_4b",
            "qwen25_7_b" => "qwen25_7b",
            "gec08_b" => "gec08b",
            other => other,
        };
        if normalized != model {
            value["local_llm_model"] = Value::String(normalized.to_string());
        }
    }

    if value.get("local_whisper_models_dir").is_none() {
        if let Some(old) = value.get("local_whisper_model_path") {
            match old {
                Value::String(path) if path.ends_with(".bin") => {
                    if let Some(parent) = Path::new(path).parent() {
                        value["local_whisper_models_dir"] =
                            Value::String(parent.display().to_string());
                    }
                }
                Value::String(path) if !path.trim().is_empty() => {
                    value["local_whisper_models_dir"] = Value::String(path.clone());
                }
                Value::Null => {
                    value["local_whisper_models_dir"] = Value::Null;
                }
                _ => {}
            }
        }
    }

    if let Some(object) = value.as_object_mut() {
        object.remove("local_whisper_model_path");
    }

    if let Some(mode) = value.get("text_processing_mode").and_then(|value| value.as_str()) {
        let migrated = match mode {
            "structural" | "md_structural" => Some("optimization"),
            "custom_skill" => None,
            _ => None,
        };
        if let Some(next) = migrated {
            value["text_processing_mode"] = Value::String(next.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{TextProcessingMode, UiMode};

    #[test]
    fn config_path_is_under_veyro_namespace() {
        let path = config_path().expect("config path");
        let rendered = path.to_string_lossy();
        assert!(rendered.contains("Veyro"));
        assert!(rendered.ends_with(CONFIG_FILE));
    }

    #[test]
    fn migrates_legacy_text_cleanup_flag() {
        let legacy = serde_json::json!({
            "text_cleanup": false,
            "global_hotkey": "Ctrl+Shift+Space",
            "transcription_provider": "openai"
        });
        let mut value: Value = legacy;
        migrate_legacy_fields(&mut value);
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.text_processing_mode, TextProcessingMode::Original);
    }

    #[test]
    fn normalizes_openai_provider_without_api_key_to_local() {
        crate::settings::secrets::reset_api_key_cache_for_tests();
        let mut settings = AppSettings {
            transcription_provider: "openai".to_string(),
            text_processing_mode: TextProcessingMode::Optimization,
            ..Default::default()
        };
        assert!(normalize_api_key_dependent_settings(&mut settings));
        assert_eq!(settings.transcription_provider, "local");
        assert_eq!(settings.text_processing_mode, TextProcessingMode::Original);
    }

    #[test]
    fn normalizes_empty_microphone_to_none() {
        let mut settings = AppSettings {
            microphone_device: Some(String::new()),
            ..Default::default()
        };
        assert!(sanitize_microphone_device(&mut settings));
        assert_eq!(settings.microphone_device, None);
    }

    #[test]
    fn migrates_removed_structural_modes_to_optimization() {
        let legacy = serde_json::json!({
            "text_processing_mode": "md_structural",
            "global_hotkey": "Shift+F1",
            "transcription_provider": "local"
        });
        let mut value: Value = legacy;
        migrate_legacy_fields(&mut value);
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.text_processing_mode, TextProcessingMode::Optimization);
    }

    #[test]
    fn default_settings_use_homemaker_ui_mode() {
        let settings = AppSettings::default();
        assert_eq!(settings.ui_mode, UiMode::Homemaker);
    }

    #[test]
    fn english_ui_falls_back_from_russian_llm_model() {
        let mut settings = AppSettings {
            ui_locale: UiLocale::En,
            local_llm_model: LlmModelKind::TLiteIt21,
            ..Default::default()
        };
        assert!(normalize_locale_dependent_settings(&mut settings));
        assert_eq!(settings.local_llm_model, LlmModelKind::Qwen3_4B);
    }

    #[test]
    fn migrates_missing_ui_mode_to_expert() {
        let legacy = serde_json::json!({
            "global_hotkey": "Shift+F1",
            "transcription_provider": "local"
        });
        let mut value: Value = legacy;
        migrate_legacy_fields(&mut value);
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.ui_mode, UiMode::Expert);
    }

    #[test]
    fn detects_encrypted_envelope_shape() {
        let envelope = EncryptedEnvelope {
            v: 1,
            nonce: "abc".to_string(),
            data: "def".to_string(),
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: EncryptedEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.v, 1);
    }
}
