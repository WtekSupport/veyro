use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::AppHandle;
use tracing::warn;

use crate::app::events;
use crate::error::ConfigError;

#[derive(Debug, Clone, Serialize)]
pub struct AiSkillInfo {
    pub filename: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
struct CachedSkill {
    mtime: SystemTime,
    body: String,
}

static SKILL_CACHE: Mutex<Option<(String, CachedSkill)>> = Mutex::new(None);

pub fn skills_dir() -> Result<PathBuf, ConfigError> {
    let base = dirs::config_dir().ok_or_else(|| {
        ConfigError::Read("unable to resolve OS config directory".to_string())
    })?;
    Ok(base.join("Veyro").join("skills"))
}

pub fn ensure_skills_dir() -> Result<PathBuf, ConfigError> {
    let dir = skills_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|error| ConfigError::Write(format!("{}: {error}", dir.display())))?;
    Ok(dir)
}

pub fn seed_skills_from_resources(resource_dir: &Path) -> Result<(), ConfigError> {
    let bundled = resource_dir.join("skills");
    if !bundled.is_dir() {
        return Ok(());
    }

    let user_dir = ensure_skills_dir()?;
    for entry in fs::read_dir(&bundled)
        .map_err(|error| ConfigError::Read(format!("{}: {error}", bundled.display())))?
    {
        let entry = entry.map_err(|error| ConfigError::Read(error.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }

        let target = user_dir.join(entry.file_name());
        if target.is_file() {
            continue;
        }

        fs::copy(&path, &target).map_err(|error| {
            ConfigError::Write(format!("{}: {error}", target.display()))
        })?;
    }

    Ok(())
}

pub fn start_skills_watcher(app: AppHandle) {
    let Ok(dir) = ensure_skills_dir() else {
        return;
    };

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(
            move |result| {
                if let Ok(event) = result {
                    let _ = tx.send(event);
                }
            },
            notify::Config::default(),
        ) {
            Ok(watcher) => watcher,
            Err(error) => {
                warn!("skills watcher init failed: {error}");
                return;
            }
        };

        if let Err(error) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
            warn!("skills watcher failed for {}: {error}", dir.display());
            return;
        }

        let mut pending = false;
        let mut last_change = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);

        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(event) => {
                    if is_skill_relevant_event(&event) {
                        pending = true;
                        last_change = Instant::now();
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if pending && last_change.elapsed() >= Duration::from_millis(400) {
                        if let Ok(mut guard) = SKILL_CACHE.lock() {
                            *guard = None;
                        }
                        events::emit_skills_changed(&app);
                        pending = false;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

fn is_skill_relevant_event(event: &notify::Event) -> bool {
    match &event.kind {
        EventKind::Create(_)
        | EventKind::Modify(_)
        | EventKind::Remove(_)
        | EventKind::Any => event
            .paths
            .iter()
            .any(|path| path.extension().and_then(|value| value.to_str()) == Some("md")),
        _ => false,
    }
}

pub fn list_skills() -> Result<Vec<AiSkillInfo>, ConfigError> {
    let dir = ensure_skills_dir()?;
    let mut skills = Vec::new();

    let mut entries: Vec<_> = fs::read_dir(&dir)
        .map_err(|error| ConfigError::Read(format!("{}: {error}", dir.display())))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }

        let filename = entry.file_name().to_string_lossy().into_owned();
        let contents = fs::read_to_string(&path)
            .map_err(|error| ConfigError::Read(format!("{}: {error}", path.display())))?;
        let (name, description, _) = parse_skill_markdown(&contents, &filename);
        skills.push(AiSkillInfo {
            filename,
            name,
            description,
        });
    }

    Ok(skills)
}

pub fn load_skill_body(filename: Option<&str>) -> Result<Option<String>, ConfigError> {
    let Some(filename) = filename.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let path = skills_dir()?.join(filename);
    if !path.is_file() {
        return Ok(None);
    }

    let mtime = fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    if let Ok(guard) = SKILL_CACHE.lock() {
        if let Some((cached_name, cached)) = guard.as_ref() {
            if cached_name == filename && cached.mtime == mtime {
                return Ok(Some(cached.body.clone()));
            }
        }
    }

    let contents = fs::read_to_string(&path)
        .map_err(|error| ConfigError::Read(format!("{}: {error}", path.display())))?;
    let (_, _, body) = parse_skill_markdown(&contents, filename);
    let body = body.trim().to_string();
    if body.is_empty() {
        return Ok(None);
    }

    if let Ok(mut guard) = SKILL_CACHE.lock() {
        *guard = Some((
            filename.to_string(),
            CachedSkill {
                mtime,
                body: body.clone(),
            },
        ));
    }

    Ok(Some(body))
}

pub fn import_skill(from_path: &str) -> Result<AiSkillInfo, ConfigError> {
    let source = Path::new(from_path.trim());
    if !source.is_file() {
        return Err(ConfigError::Invalid("skill_file_not_found".to_string()));
    }

    if source.extension().and_then(|value| value.to_str()) != Some("md") {
        return Err(ConfigError::Invalid("skill_file_not_markdown".to_string()));
    }

    let filename = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ConfigError::Invalid("skill_file_invalid_name".to_string()))?
        .to_string();

    let dir = ensure_skills_dir()?;
    let target = dir.join(&filename);
    fs::copy(source, &target).map_err(|error| {
        ConfigError::Write(format!("{}: {error}", target.display()))
    })?;

    let contents = fs::read_to_string(&target)
        .map_err(|error| ConfigError::Read(format!("{}: {error}", target.display())))?;
    let (name, description, _) = parse_skill_markdown(&contents, &filename);
    Ok(AiSkillInfo {
        filename,
        name,
        description,
    })
}

pub fn open_skills_folder() -> Result<(), ConfigError> {
    let dir = ensure_skills_dir()?;
    open_path_in_file_manager(&dir)
}

fn open_path_in_file_manager(path: &Path) -> Result<(), ConfigError> {
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

fn parse_skill_markdown(contents: &str, fallback_name: &str) -> (String, String, String) {
    let trimmed = contents.trim_start();
    if !trimmed.starts_with("---") {
        return (
            fallback_name.to_string(),
            String::new(),
            contents.to_string(),
        );
    }

    let Some(rest) = trimmed.strip_prefix("---") else {
        return (
            fallback_name.to_string(),
            String::new(),
            contents.to_string(),
        );
    };

    let Some((frontmatter, body)) = rest.split_once("---") else {
        return (
            fallback_name.to_string(),
            String::new(),
            contents.to_string(),
        );
    };

    let mut name = fallback_name.to_string();
    let mut description = String::new();
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("name:") {
            name = value.trim().trim_matches('"').to_string();
        } else if let Some(value) = line.strip_prefix("description:") {
            description = value.trim().trim_matches('"').to_string();
        }
    }

    (name, description, body.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter_and_body() {
        let markdown = "---\nname: Business\n description: Formal tone\n---\nUse short paragraphs.";
        let (name, description, body) = parse_skill_markdown(markdown, "fallback.md");
        assert_eq!(name, "Business");
        assert_eq!(description, "Formal tone");
        assert_eq!(body, "Use short paragraphs.");
    }

    #[test]
    fn uses_full_body_without_frontmatter() {
        let markdown = "Keep a friendly conversational tone.";
        let (name, description, body) = parse_skill_markdown(markdown, "custom.md");
        assert_eq!(name, "custom.md");
        assert!(description.is_empty());
        assert_eq!(body, markdown);
    }
}
