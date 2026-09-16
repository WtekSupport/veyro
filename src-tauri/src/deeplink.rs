use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tracing::warn;
use url::Url;

use crate::app::context::AppContext;
use crate::app::events::{emit_skill_imported, SkillImportedPayload};
use crate::error::ConfigError;
use crate::i18n;
use crate::notify;
use crate::settings::{save_settings, SettingsPatch, TextProcessingMode};
use crate::text::skill::{import_skill, AiSkillInfo};
use crate::window;

const MAX_REMOTE_SKILL_BYTES: u64 = 1024 * 1024;

pub enum SkillSource {
    Local(PathBuf),
    Remote(Url),
}

pub fn handle_skill_import_urls(app: &AppHandle, urls: Vec<String>) {
    for url in urls {
        if let Err(error) = import_skill_from_url(app, &url) {
            warn!("deep link skill import failed for {url}: {error}");
            let locale = app_settings_locale(app);
            notify::notify(
                app,
                &i18n::translate(locale, "notify.error_title", &[]),
                &i18n::translate(
                    locale,
                    "notify.skill_import_failed",
                    &[("error", &error.to_string())],
                ),
            );
        }
    }
}

fn import_skill_from_url(app: &AppHandle, raw_url: &str) -> Result<AiSkillInfo, ConfigError> {
    let source = resolve_skill_source(app, raw_url)?;
    let skill = import_skill(source.to_string_lossy().as_ref())?;
    apply_skill_import(app, skill)
}

fn resolve_skill_source(app: &AppHandle, raw_url: &str) -> Result<PathBuf, ConfigError> {
    match parse_veyro_skill_source(raw_url)? {
        SkillSource::Local(path) => Ok(path),
        SkillSource::Remote(url) => {
            let client = app
                .try_state::<std::sync::Arc<AppContext>>()
                .map(|ctx| ctx.inner().http.clone())
                .unwrap_or_else(reqwest::Client::new);
            tauri::async_runtime::block_on(download_remote_skill(&client, &url))
        }
    }
}

fn apply_skill_import(app: &AppHandle, skill: AiSkillInfo) -> Result<AiSkillInfo, ConfigError> {
    if let Some(ctx) = app.try_state::<std::sync::Arc<AppContext>>() {
        let ctx = ctx.inner();
        if let Ok(mut controller) = ctx.controller.lock() {
            let patch = SettingsPatch {
                ai_rewrite_skill: Some(Some(skill.filename.clone())),
                text_processing_mode: Some(TextProcessingMode::CustomSkill),
                ..Default::default()
            };
            if controller.plan_settings_update(patch).is_ok() {
                if let Err(error) = save_settings(controller.settings()) {
                    warn!("failed to persist settings after skill import: {error}");
                }
            }
        }

        let locale = app_settings_locale(app);
        ctx.record_activity(
            Some(app),
            crate::app::activity_log::ActivityLevel::Info,
            "activity.skill_imported",
            serde_json::json!({
                "name": skill.name,
                "filename": skill.filename,
            }),
        );
        notify::notify(
            app,
            &i18n::translate(locale, "app.title", &[]),
            &i18n::translate(
                locale,
                "notify.skill_imported",
                &[("name", &skill.name)],
            ),
        );
    }

    emit_skill_imported(
        app,
        SkillImportedPayload {
            skill: skill.clone(),
        },
    );
    window::show_settings_window(app);

    Ok(skill)
}

pub fn parse_veyro_skill_source(raw_url: &str) -> Result<SkillSource, ConfigError> {
    let url = Url::parse(raw_url.trim()).map_err(|_| {
        ConfigError::Invalid("deeplink_invalid_url".to_string())
    })?;

    if url.scheme() != "veyro" {
        return Err(ConfigError::Invalid("deeplink_invalid_scheme".to_string()));
    }

    if let Some(remote) = query_remote_url(&url) {
        return Ok(SkillSource::Remote(remote));
    }

    if let Some(path) = query_path(&url) {
        return classify_path_source(&path);
    }

    let path = reconstruct_path_from_url(&url).ok_or_else(|| {
        ConfigError::Invalid("deeplink_invalid_path".to_string())
    })?;
    classify_path_source(&path)
}

fn classify_path_source(path: &Path) -> Result<SkillSource, ConfigError> {
    if let Some(remote) = extract_http_url(&path.to_string_lossy()) {
        return Ok(SkillSource::Remote(remote));
    }
    Ok(SkillSource::Local(validate_md_path(path)?))
}

fn query_remote_url(url: &Url) -> Option<Url> {
    for (key, value) in url.query_pairs() {
        if key == "url" || key == "href" {
            if let Some(remote) = extract_http_url(value.trim()) {
                return Some(remote);
            }
        }
    }
    None
}

fn query_path(url: &Url) -> Option<PathBuf> {
    for (key, value) in url.query_pairs() {
        if key == "path" || key == "file" {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
    }
    None
}

fn extract_http_url(value: &str) -> Option<Url> {
    let trimmed = value.trim().trim_start_matches('/');
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return None;
    }
    Url::parse(trimmed).ok()
}

fn reconstruct_path_from_url(url: &Url) -> Option<PathBuf> {
    if let Some(host) = url.host_str() {
        if host.len() == 1 && host.chars().next()?.is_ascii_alphabetic() {
            let drive = host.to_ascii_uppercase();
            let path = url.path();
            return Some(PathBuf::from(format!("{drive}:{path}")));
        }

        if matches!(host, "import" | "skill" | "open" | "localhost") {
            let path = url.path().trim_start_matches('/');
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    let path = url.path();
    if path.is_empty() {
        return None;
    }

    Some(PathBuf::from(path))
}

fn validate_md_path(path: &Path) -> Result<PathBuf, ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(ConfigError::Invalid("deeplink_invalid_path".to_string()));
    }

    if path.extension().and_then(|value| value.to_str()) != Some("md") {
        return Err(ConfigError::Invalid("skill_file_not_markdown".to_string()));
    }

    if !path.is_file() {
        return Err(ConfigError::Invalid("skill_file_not_found".to_string()));
    }

    Ok(path.to_path_buf())
}

async fn download_remote_skill(
    client: &reqwest::Client,
    url: &Url,
) -> Result<PathBuf, ConfigError> {
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(ConfigError::Invalid("deeplink_invalid_url".to_string()));
    }

    let filename = filename_from_remote_url(url)?;

    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| ConfigError::Read(format!("deeplink_download_failed: {error}")))?;

    if !response.status().is_success() {
        return Err(ConfigError::Read(format!(
            "deeplink_download_failed: HTTP {}",
            response.status()
        )));
    }

    if let Some(length) = response.content_length() {
        if length > MAX_REMOTE_SKILL_BYTES {
            return Err(ConfigError::Invalid("deeplink_file_too_large".to_string()));
        }
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| ConfigError::Read(format!("deeplink_download_failed: {error}")))?;

    if bytes.len() as u64 > MAX_REMOTE_SKILL_BYTES {
        return Err(ConfigError::Invalid("deeplink_file_too_large".to_string()));
    }

    if std::str::from_utf8(&bytes).is_err() {
        return Err(ConfigError::Invalid("skill_file_not_markdown".to_string()));
    }

    let temp_dir = std::env::temp_dir().join("veyro-skill-import");
    fs::create_dir_all(&temp_dir).map_err(|error| {
        ConfigError::Write(format!("{}: {error}", temp_dir.display()))
    })?;
    let temp_path = temp_dir.join(&filename);
    fs::write(&temp_path, &bytes).map_err(|error| {
        ConfigError::Write(format!("{}: {error}", temp_path.display()))
    })?;

    Ok(temp_path)
}

fn filename_from_remote_url(url: &Url) -> Result<String, ConfigError> {
    let filename = url
        .path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())
        .map(sanitize_filename)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ConfigError::Invalid("deeplink_invalid_path".to_string()))?;

    if filename.contains("..") {
        return Err(ConfigError::Invalid("skill_file_invalid_name".to_string()));
    }

    if Path::new(&filename)
        .extension()
        .and_then(|value| value.to_str())
        != Some("md")
    {
        return Err(ConfigError::Invalid("skill_file_not_markdown".to_string()));
    }

    Ok(filename)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn app_settings_locale(app: &AppHandle) -> crate::settings::UiLocale {
    app.try_state::<std::sync::Arc<AppContext>>()
        .and_then(|ctx| {
            ctx.inner()
                .controller
                .try_lock()
                .ok()
                .map(|controller| controller.settings().ui_locale)
        })
        .unwrap_or(crate::settings::UiLocale::En)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_drive_url() {
        let path = reconstruct_path_from_url(
            &Url::parse("veyro://C:/Users/test/skill.md").unwrap(),
        )
        .unwrap();
        assert_eq!(path.to_string_lossy(), "C:/Users/test/skill.md");
    }

    #[test]
    fn parses_absolute_unix_url() {
        let path = reconstruct_path_from_url(
            &Url::parse("veyro:///home/user/skill.md").unwrap(),
        )
        .unwrap();
        assert_eq!(path.to_string_lossy(), "/home/user/skill.md");
    }

    #[test]
    fn parses_query_path() {
        let url = Url::parse("veyro://import?path=C%3A%2Ftmp%2Fskill.md").unwrap();
        assert_eq!(
            query_path(&url).unwrap().to_string_lossy(),
            "C:/tmp/skill.md"
        );
    }

    #[test]
    fn parses_query_remote_url() {
        let url = Url::parse(
            "veyro://import?url=https%3A%2F%2Fexample.com%2Fskills%2Fbusiness.md",
        )
        .unwrap();
        let remote = query_remote_url(&url).unwrap();
        assert_eq!(remote.as_str(), "https://example.com/skills/business.md");
    }

    #[test]
    fn parses_remote_url_in_path() {
        let source = parse_veyro_skill_source("veyro:///https://example.com/skill.md").unwrap();
        match source {
            SkillSource::Remote(url) => {
                assert_eq!(url.as_str(), "https://example.com/skill.md");
            }
            SkillSource::Local(_) => panic!("expected remote source"),
        }
    }

    #[test]
    fn parses_remote_url_in_path_query() {
        let source = parse_veyro_skill_source(
            "veyro://import?path=https%3A%2F%2Fexample.com%2Fskill.md",
        )
        .unwrap();
        match source {
            SkillSource::Remote(url) => {
                assert_eq!(url.as_str(), "https://example.com/skill.md");
            }
            SkillSource::Local(_) => panic!("expected remote source"),
        }
    }

    #[test]
    fn rejects_non_veyro_scheme() {
        assert!(parse_veyro_skill_source("https://example.com/skill.md").is_err());
    }

    #[test]
    fn remote_filename_requires_md_extension() {
        let url = Url::parse("https://example.com/skills/readme.txt").unwrap();
        assert!(filename_from_remote_url(&url).is_err());
    }

    #[test]
    fn remote_filename_uses_last_path_segment() {
        let url = Url::parse("https://example.com/skills/business-ru.md").unwrap();
        assert_eq!(filename_from_remote_url(&url).unwrap(), "business-ru.md");
    }
}
