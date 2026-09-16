use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Deserialize;

use crate::error::ConfigError;
use crate::text::corrections::sort_corrections;

/// Synced from `docs/dictionary/dictionary.default.toml` at build time (see `build.rs`).
const DEFAULT_DICTIONARY: &str = include_str!("../../resources/dictionary/dictionary.toml");

#[derive(Debug, Clone, Default, Deserialize)]
struct DictionaryToml {
    #[serde(default)]
    vocabulary: Vec<String>,
    #[serde(default)]
    corrections: HashMap<String, String>,
    #[serde(default)]
    whisper: WhisperOverridesToml,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct WhisperOverridesToml {
    logprob_thold: Option<f32>,
    entropy_thold: Option<f32>,
    temperature_inc: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct WhisperOverrides {
    pub logprob_thold: Option<f32>,
    pub entropy_thold: Option<f32>,
    pub temperature_inc: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    pub vocabulary: Vec<String>,
    pub corrections: Vec<(String, String)>,
    pub whisper: WhisperOverrides,
}

#[derive(Debug, Clone)]
struct CachedDictionary {
    mtime: SystemTime,
    dictionary: Dictionary,
}

static DICTIONARY_CACHE: Mutex<Option<(PathBuf, CachedDictionary)>> = Mutex::new(None);

pub fn default_dictionary_dir() -> Result<PathBuf, ConfigError> {
    let base = dirs::config_dir().ok_or_else(|| {
        ConfigError::Read("unable to resolve OS config directory".to_string())
    })?;
    Ok(base.join("Veyro").join("dictionary"))
}

pub fn default_dictionary_path() -> Result<PathBuf, ConfigError> {
    Ok(default_dictionary_dir()?.join("dictionary.toml"))
}

fn legacy_dictionary_path() -> Result<PathBuf, ConfigError> {
    let base = dirs::config_dir().ok_or_else(|| {
        ConfigError::Read("unable to resolve OS config directory".to_string())
    })?;
    Ok(base.join("Veyro").join("dictionary.toml"))
}

pub fn resolve_dictionary_path(custom_path: Option<&str>) -> Result<PathBuf, ConfigError> {
    let custom = custom_path.map(str::trim).filter(|value| !value.is_empty());
    if let Some(path) = custom {
        return Ok(PathBuf::from(path));
    }
    default_dictionary_path()
}

fn seed_default_dictionary(path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ConfigError::Write(format!("{}: {error}", parent.display())))?;
    }

    let legacy = legacy_dictionary_path()?;
    if legacy.is_file() {
        fs::copy(&legacy, path)
            .map_err(|error| ConfigError::Write(format!("{}: {error}", path.display())))?;
        return Ok(());
    }

    fs::write(path, DEFAULT_DICTIONARY)
        .map_err(|error| ConfigError::Write(format!("{}: {error}", path.display())))
}

pub fn ensure_dictionary_file() -> Result<PathBuf, ConfigError> {
    let path = default_dictionary_path()?;
    if !path.is_file() {
        seed_default_dictionary(&path)?;
    }
    Ok(path)
}

pub fn load_dictionary(custom_path: Option<&str>) -> Result<Dictionary, ConfigError> {
    let path = resolve_dictionary_path(custom_path)?;
    let mtime = fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    if let Ok(guard) = DICTIONARY_CACHE.lock() {
        if let Some((cached_path, cached)) = guard.as_ref() {
            if cached_path == &path && cached.mtime == mtime {
                return Ok(cached.dictionary.clone());
            }
        }
    }

    let contents = if path.is_file() {
        fs::read_to_string(&path)
            .map_err(|error| ConfigError::Read(format!("{}: {error}", path.display())))?
    } else {
        String::new()
    };

    let dictionary = parse_dictionary(&contents);

    if let Ok(mut guard) = DICTIONARY_CACHE.lock() {
        *guard = Some((
            path,
            CachedDictionary {
                mtime,
                dictionary: dictionary.clone(),
            },
        ));
    }

    Ok(dictionary)
}

fn parse_dictionary(contents: &str) -> Dictionary {
    let parsed: DictionaryToml = toml::from_str(contents).unwrap_or_default();
    let corrections = sort_corrections(
        parsed
            .corrections
            .into_iter()
            .map(|(from, to)| (from, to))
            .collect(),
    );

    Dictionary {
        vocabulary: parsed
            .vocabulary
            .into_iter()
            .map(|term| term.trim().to_string())
            .filter(|term| !term.is_empty())
            .collect(),
        corrections,
        whisper: WhisperOverrides {
            logprob_thold: parsed.whisper.logprob_thold,
            entropy_thold: parsed.whisper.entropy_thold,
            temperature_inc: parsed.whisper.temperature_inc,
        },
    }
}

/// Terms that AI rewrite must preserve: vocabulary plus correction targets.
pub fn protected_terms(dictionary: &Dictionary) -> Vec<String> {
    let mut terms: HashSet<String> = dictionary
        .vocabulary
        .iter()
        .filter(|term| !term.trim().is_empty())
        .cloned()
        .collect();

    for (_, to) in &dictionary.corrections {
        let trimmed = to.trim();
        if !trimmed.is_empty() {
            terms.insert(trimmed.to_string());
        }
    }

    let mut sorted: Vec<String> = terms.into_iter().collect();
    sorted.sort_by(|left, right| left.to_lowercase().cmp(&right.to_lowercase()));
    sorted
}

pub fn open_dictionary_folder(custom_path: Option<&str>) -> Result<(), ConfigError> {
    let path = resolve_dictionary_path(custom_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ConfigError::Write(format!("{}: {error}", parent.display())))?;
        open_path(parent)
    } else {
        open_path(&path)
    }
}

fn open_path(path: &Path) -> Result<(), ConfigError> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|error| ConfigError::Read(error.to_string()))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| ConfigError::Read(error.to_string()))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| ConfigError::Read(error.to_string()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_dictionary() {
        let dictionary = parse_dictionary(DEFAULT_DICTIONARY);
        assert!(dictionary.vocabulary.is_empty());
        assert!(dictionary.corrections.is_empty());
        assert!(dictionary.whisper.logprob_thold.is_none());
    }

    #[test]
    fn parses_example_dictionary() {
        let example = include_str!("../../../docs/dictionary/dictionary.example.toml");
        let dictionary = parse_dictionary(example);
        assert!(dictionary.vocabulary.iter().any(|term| term == "послать"));
        assert!(dictionary
            .corrections
            .iter()
            .any(|(from, _)| from == "с лать"));
    }
}
