use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceWatchPreset {
    pub id: String,
    pub label_key: String,
    pub path: Option<String>,
    pub exists: bool,
}

pub fn resolve_downloads_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

pub fn resolve_telegram_desktop_dir() -> Option<PathBuf> {
    let downloads = resolve_downloads_dir()?;
    let candidates = [
        downloads.join("Telegram Desktop"),
        downloads.join("TelegramDesktop"),
    ];
    for path in candidates {
        if path.is_dir() {
            return Some(path);
        }
    }
    // Prefer conventional path even if missing (user can create / enable later).
    Some(downloads.join("Telegram Desktop"))
}

pub fn list_presets() -> Vec<VoiceWatchPreset> {
    let mut presets = Vec::new();

    let downloads = resolve_downloads_dir();
    let downloads_exists = downloads.as_ref().is_some_and(|p| p.is_dir());
    presets.push(VoiceWatchPreset {
        id: "downloads".to_string(),
        label_key: "tools.voiceWatch.preset.downloads".to_string(),
        path: downloads.map(|p| p.to_string_lossy().to_string()),
        exists: downloads_exists,
    });

    let tg = resolve_telegram_desktop_dir();
    let tg_exists = tg.as_ref().is_some_and(|p| p.is_dir());
    presets.push(VoiceWatchPreset {
        id: "telegram".to_string(),
        label_key: "tools.voiceWatch.preset.telegram".to_string(),
        path: tg.map(|p| p.to_string_lossy().to_string()),
        exists: tg_exists,
    });

    presets
}

/// Heuristic messenger label from folder path / file name (optional metadata).
pub fn guess_messenger(path: &Path) -> Option<String> {
    let full = path.to_string_lossy().to_ascii_lowercase();
    if full.contains("telegram") {
        return Some("Telegram".to_string());
    }
    if full.contains("element") || full.contains("riot") {
        return Some("Element".to_string());
    }
    if full.contains("whatsapp") {
        return Some("WhatsApp".to_string());
    }
    if full.contains("\\max\\") || full.contains("/max/") || full.contains("max messenger") {
        return Some("Max".to_string());
    }
    None
}
