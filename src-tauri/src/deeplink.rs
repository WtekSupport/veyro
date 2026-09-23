use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;
use tracing::{info, warn};
use url::Url;

use crate::app::context::AppContext;
use crate::app::events::{
    emit_skill_import_flow, emit_skill_imported, SkillImportFlowPayload, SkillImportPhase,
    SkillImportedPayload,
};
use crate::error::ConfigError;
use crate::i18n;
use crate::notify;
use crate::settings::{save_settings, SettingsPatch, TextProcessingMode};
use crate::text::skill::{
    import_skill_as, inspect_skill_file_as, is_skill_filename_installed, AiSkillInfo,
};
use crate::window;

const MAX_REMOTE_SKILL_BYTES: u64 = 1024 * 1024;
const VEYRO_SCHEME_PREFIX: &str = "veyro://";
const AISTRUCTEDIT_CATALOG_ITEM: &str = "https://aistructedit.com/api/catalog/";
const AISTRUCTEDIT_PARTNER_ID: &str = "veyro";

/// Windows/Linux pass the URL on the command line when opening `veyro://…`.
pub fn veyro_urls_from_cli_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|arg| arg.as_ref().trim().trim_matches('"').to_string())
        .filter(|arg| arg.starts_with(VEYRO_SCHEME_PREFIX))
        .collect()
}

/// Hide the extra console window Windows creates for protocol-handler launches (debug builds only).
pub fn prepare_windows_deeplink_launch() {
    #[cfg(all(windows, debug_assertions))]
    {
        let deeplink = !veyro_urls_from_cli_args(std::env::args().skip(1)).is_empty();
        if deeplink {
            use windows::Win32::System::Console::FreeConsole;
            unsafe {
                let _ = FreeConsole();
            }
        }
    }
}

#[derive(Debug)]
pub enum SkillSource {
    Local(PathBuf),
    Remote(Url),
    /// aiStructEdit community catalog (`partnerInstallPath`, e.g. `translate-en`).
    Catalog(String),
}

struct StagedSkill {
    path: PathBuf,
    install_filename: String,
    remove_after_flow: bool,
    source_label: String,
}

struct SkillImportGate {
    snapshot: Option<SkillImportFlowPayload>,
    decision_tx: Option<oneshot::Sender<bool>>,
}

static SKILL_IMPORT_GATE: OnceLock<Mutex<SkillImportGate>> = OnceLock::new();
static SKILL_IMPORT_TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
static SKILL_IMPORT_GENERATION: AtomicU64 = AtomicU64::new(0);

fn skill_import_gate() -> &'static Mutex<SkillImportGate> {
    SKILL_IMPORT_GATE.get_or_init(|| {
        Mutex::new(SkillImportGate {
            snapshot: None,
            decision_tx: None,
        })
    })
}

fn skill_import_session_stale(session: u64) -> bool {
    session != SKILL_IMPORT_GENERATION.load(Ordering::SeqCst)
}

/// Ends any in-flight import wait and starts a new deeplink session.
fn kick_skill_import_for_new_deeplink() -> u64 {
    if let Ok(mut gate) = skill_import_gate().lock() {
        if let Some(tx) = gate.decision_tx.take() {
            let _ = tx.send(false);
        }
    }
    SKILL_IMPORT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

fn invalidate_skill_import_sessions() {
    if let Ok(mut gate) = skill_import_gate().lock() {
        if let Some(tx) = gate.decision_tx.take() {
            let _ = tx.send(false);
        }
    }
    SKILL_IMPORT_GENERATION.fetch_add(1, Ordering::SeqCst);
}

pub fn handle_skill_import_urls(app: &AppHandle, urls: Vec<String>) {
    if urls.is_empty() {
        return;
    }

    let app_show = app.clone();
    let _ = app.run_on_main_thread(move || {
        window::show_skill_import_window(&app_show);
    });

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for raw_url in urls {
            let session = kick_skill_import_for_new_deeplink();
            if let Err(error) = run_interactive_skill_import(&app, &raw_url, session).await {
                warn!("deep link skill import failed for {raw_url}: {error}");
                let locale = app_settings_locale(&app);
                notify::notify(
                    &app,
                    &i18n::translate(locale, "notify.error_title", &[]),
                    &i18n::translate(
                        locale,
                        "notify.skill_import_failed",
                        &[("error", &error.to_string())],
                    ),
                );
            }
        }
    });
}

pub fn replay_skill_import_flow(app: &AppHandle) {
    if let Some(snapshot) = get_skill_import_flow_snapshot() {
        emit_skill_import_flow(app, snapshot);
    }
}

pub fn get_skill_import_flow_snapshot() -> Option<SkillImportFlowPayload> {
    skill_import_gate()
        .lock()
        .ok()
        .and_then(|gate| gate.snapshot.clone())
}

pub fn confirm_deeplink_skill_import() -> bool {
    if let Ok(mut gate) = skill_import_gate().lock() {
        if let Some(tx) = gate.decision_tx.take() {
            return tx.send(true).is_ok();
        }
    }
    false
}

pub fn cancel_deeplink_skill_import(app: &AppHandle) {
    invalidate_skill_import_sessions();
    clear_flow_snapshot();
    let _ = window::hide_skill_import_window(app);
}

fn clear_flow_snapshot() {
    if let Ok(mut gate) = skill_import_gate().lock() {
        gate.snapshot = None;
        if let Some(tx) = gate.decision_tx.take() {
            let _ = tx.send(false);
        }
    }
}

fn emit_flow(app: &AppHandle, payload: SkillImportFlowPayload) {
    if let Ok(mut gate) = skill_import_gate().lock() {
        gate.snapshot = Some(payload.clone());
    }
    emit_skill_import_flow(app, payload);
}

fn resize_skill_import_window(app: &AppHandle, preview: bool) {
    let Some(window) = app.get_webview_window(window::SKILL_IMPORT_WINDOW_LABEL) else {
        return;
    };
    window::resize_skill_import_window(&window, preview);
}

async fn run_interactive_skill_import(
    app: &AppHandle,
    raw_url: &str,
    session: u64,
) -> Result<(), ConfigError> {
    if skill_import_session_stale(session) {
        return Ok(());
    }

    let app_show = app.clone();
    let _ = window::await_on_main_thread(app, move || {
        window::show_skill_import_window(&app_show);
    })
    .await;

    if skill_import_session_stale(session) {
        return Ok(());
    }

    emit_flow(
        app,
        SkillImportFlowPayload {
            phase: SkillImportPhase::Preparing,
            skill: None,
            source: None,
            size_bytes: None,
            error: None,
        },
    );
    replay_skill_import_flow(app);

    let staged = match prepare_staged_skill(app, raw_url).await {
        Ok(staged) => staged,
        Err(error) => {
            if skill_import_session_stale(session) {
                clear_flow_snapshot();
                return Ok(());
            }
            emit_flow(
                app,
                SkillImportFlowPayload {
                    phase: SkillImportPhase::Error,
                    skill: None,
                    source: Some(raw_url.to_string()),
                    size_bytes: None,
                    error: Some(localized_import_error(app, &error)),
                },
            );
            resize_skill_import_window(app, false);
            return Err(error);
        }
    };

    if skill_import_session_stale(session) {
        cleanup_staged(&staged);
        clear_flow_snapshot();
        return Ok(());
    }

    let (skill, size_bytes) = match inspect_skill_file_as(
        staged.path.to_string_lossy().as_ref(),
        &staged.install_filename,
    ) {
        Ok(values) => values,
        Err(error) => {
            cleanup_staged(&staged);
            if skill_import_session_stale(session) {
                return Ok(());
            }
            emit_flow(
                app,
                SkillImportFlowPayload {
                    phase: SkillImportPhase::Error,
                    skill: None,
                    source: Some(staged.source_label.clone()),
                    size_bytes: None,
                    error: Some(localized_import_error(app, &error)),
                },
            );
            resize_skill_import_window(app, false);
            return Err(error);
        }
    };

    if skill_import_session_stale(session) {
        cleanup_staged(&staged);
        clear_flow_snapshot();
        return Ok(());
    }

    if is_skill_filename_installed(&skill.filename)? {
        let dismiss_rx = register_skill_import_decision_waiter();
        emit_flow(
            app,
            SkillImportFlowPayload {
                phase: SkillImportPhase::AlreadyInstalled,
                skill: Some(skill.clone()),
                source: Some(staged.source_label.clone()),
                size_bytes: Some(size_bytes),
                error: None,
            },
        );
        resize_skill_import_window(app, true);
        replay_skill_import_flow(app);
        let _ = dismiss_rx.await.unwrap_or(false);
        cleanup_staged(&staged);
        clear_flow_snapshot();
        if !skill_import_session_stale(session) {
            let app_hide = app.clone();
            let _ = window::await_on_main_thread(app, move || {
                let _ = window::hide_skill_import_window(&app_hide);
            })
            .await;
        }
        return Ok(());
    }

    let decision_rx = register_skill_import_decision_waiter();

    emit_flow(
        app,
        SkillImportFlowPayload {
            phase: SkillImportPhase::Preview,
            skill: Some(skill.clone()),
            source: Some(staged.source_label.clone()),
            size_bytes: Some(size_bytes),
            error: None,
        },
    );
    resize_skill_import_window(app, true);
    replay_skill_import_flow(app);

    let confirmed = decision_rx.await.unwrap_or(false);
    if skill_import_session_stale(session) || !confirmed {
        cleanup_staged(&staged);
        clear_flow_snapshot();
        let app_hide = app.clone();
        let _ = window::await_on_main_thread(app, move || {
            let _ = window::hide_skill_import_window(&app_hide);
        })
        .await;
        return Ok(());
    }

    emit_flow(
        app,
        SkillImportFlowPayload {
            phase: SkillImportPhase::Installing,
            skill: Some(skill.clone()),
            source: Some(staged.source_label.clone()),
            size_bytes: Some(size_bytes),
            error: None,
        },
    );

    tokio::task::yield_now().await;

    if skill_import_session_stale(session) {
        cleanup_staged(&staged);
        clear_flow_snapshot();
        return Ok(());
    }

    if staged.remove_after_flow && !staged.path.is_file() {
        cleanup_staged(&staged);
        return Err(ConfigError::Read(format!(
            "skill_file_not_found:{}",
            staged.path.display()
        )));
    }

    let installed = match import_skill_as(
        staged.path.to_string_lossy().as_ref(),
        &staged.install_filename,
    ) {
        Ok(skill) => skill,
        Err(error) => {
            cleanup_staged(&staged);
            emit_flow(
                app,
                SkillImportFlowPayload {
                    phase: SkillImportPhase::Error,
                    skill: None,
                    source: Some(staged.source_label.clone()),
                    size_bytes: None,
                    error: Some(localized_import_error(app, &error)),
                },
            );
            return Err(error);
        }
    };
    cleanup_staged(&staged);
    apply_skill_import(app, installed)?;

    if skill_import_session_stale(session) {
        return Ok(());
    }

    emit_flow(
        app,
        SkillImportFlowPayload {
            phase: SkillImportPhase::Done,
            skill: None,
            source: None,
            size_bytes: None,
            error: None,
        },
    );
    clear_flow_snapshot();

    let app_hide = app.clone();
    let _ = window::await_on_main_thread(app, move || {
        let _ = window::hide_skill_import_window(&app_hide);
    })
    .await;

    Ok(())
}

fn register_skill_import_decision_waiter() -> oneshot::Receiver<bool> {
    let (tx, rx) = oneshot::channel();
    if let Ok(mut gate) = skill_import_gate().lock() {
        if let Some(stale) = gate.decision_tx.take() {
            let _ = stale.send(false);
        }
        gate.decision_tx = Some(tx);
    }
    rx
}

async fn prepare_staged_skill(app: &AppHandle, raw_url: &str) -> Result<StagedSkill, ConfigError> {
    let trimmed = raw_url.trim().trim_matches('"');
    let client = app
        .try_state::<std::sync::Arc<AppContext>>()
        .map(|ctx| ctx.inner().http.clone())
        .unwrap_or_else(reqwest::Client::new);

    if trimmed.starts_with(VEYRO_SCHEME_PREFIX) {
        if let Some(slug) = catalog_slug_from_raw_deeplink(trimmed) {
            match stage_catalog_skill(&client, &slug).await {
                Ok(staged) => {
                    info!("skill import staged from aiStructEdit catalog slug={slug} url={trimmed}");
                    return Ok(staged);
                }
                Err(ConfigError::Invalid(code)) if code == "deeplink_catalog_not_found" => {
                    warn!("catalog slug {slug} not found for {trimmed}, falling back to parsed source");
                }
                Err(error) => return Err(error),
            }
        }
    }

    let source = parse_veyro_skill_source(trimmed)?;
    info!("skill import deeplink parsed source: {source:?} for {trimmed}");

    match source {
        SkillSource::Local(path) => {
            if path.is_file() {
                let validated = validate_md_path(&path)?;
                let install_filename = install_filename_from_path(&validated)?;
                let source_label = validated.display().to_string();
                return Ok(StagedSkill {
                    path: validated,
                    install_filename,
                    remove_after_flow: false,
                    source_label,
                });
            }
            if let Some(slug) = catalog_slug_from_path(&path)
                .or_else(|| catalog_slug_from_raw_deeplink(trimmed))
            {
                return stage_catalog_skill(&client, &slug).await;
            }
            let validated = validate_md_path(&path)?;
            let install_filename = install_filename_from_path(&validated)?;
            let source_label = validated.display().to_string();
            Ok(StagedSkill {
                path: validated,
                install_filename,
                remove_after_flow: false,
                source_label,
            })
        }
        SkillSource::Remote(url) => {
            let install_filename = filename_from_remote_url(&url)?;
            let temp_path = download_remote_skill(&client, &url).await?;
            Ok(StagedSkill {
                path: temp_path,
                install_filename,
                remove_after_flow: true,
                source_label: url.to_string(),
            })
        }
        SkillSource::Catalog(install_path) => stage_catalog_skill(&client, &install_path).await,
    }
}

async fn stage_catalog_skill(
    client: &reqwest::Client,
    install_path: &str,
) -> Result<StagedSkill, ConfigError> {
    let install_filename = catalog_install_filename(install_path)?;
    let temp_path = download_catalog_skill(client, install_path).await?;
    let source_label = format!("aistructedit.com/catalog/{install_path}");
    Ok(StagedSkill {
        path: temp_path,
        install_filename,
        remove_after_flow: true,
        source_label,
    })
}

fn cleanup_staged(staged: &StagedSkill) {
    if staged.remove_after_flow {
        let _ = fs::remove_file(&staged.path);
    }
}

fn localized_import_error(app: &AppHandle, error: &ConfigError) -> String {
    let locale = app_settings_locale(app);
    i18n::translate(locale, "notify.skill_import_failed", &[("error", &error.to_string())])
}

pub(crate) fn apply_skill_import(app: &AppHandle, skill: AiSkillInfo) -> Result<AiSkillInfo, ConfigError> {
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
    let trimmed = raw_url.trim().trim_matches('"');
    let url = Url::parse(trimmed).map_err(|_| {
        ConfigError::Invalid("deeplink_invalid_url".to_string())
    })?;

    if url.scheme() != "veyro" {
        return Err(ConfigError::Invalid("deeplink_invalid_scheme".to_string()));
    }

    if let Some(remote) = query_remote_url(&url) {
        return Ok(SkillSource::Remote(remote));
    }

    if let Some(path) = query_path(&url) {
        if let Some(slug) = catalog_slug_from_path(&path) {
            return Ok(SkillSource::Catalog(slug));
        }
        return classify_path_source(&path);
    }

    if let Some(slug) = catalog_slug_from_veyro_url(&url) {
        return Ok(SkillSource::Catalog(slug));
    }

    if let Some(slug) = catalog_slug_from_raw_deeplink(trimmed) {
        return Ok(SkillSource::Catalog(slug));
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
    if let Some(slug) = catalog_slug_from_path(path) {
        return Ok(SkillSource::Catalog(slug));
    }
    // Do not validate the filesystem here — `prepare_staged_skill` tries the aiStructEdit
    // catalog when the path is missing (e.g. `veyro://translate-en.md/` → `/` on disk).
    Ok(SkillSource::Local(path.to_path_buf()))
}

/// Single-segment relative slug from the catalog (`translate-en.md`), not a filesystem path.
fn catalog_slug_from_path(path: &Path) -> Option<String> {
    if path.is_absolute() {
        return None;
    }
    let lossy = path.to_string_lossy();
    if extract_http_url(&lossy).is_some() {
        return None;
    }
    let normalized = lossy.replace('\\', "/");
    let segments: Vec<&str> = normalized
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    if segments.len() != 1 {
        return None;
    }
    if !looks_like_catalog_slug(segments[0]) {
        return None;
    }
    Some(normalize_catalog_slug(segments[0]))
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
        return Err(ConfigError::Invalid(format!(
            "skill_file_not_found:{}",
            path.display()
        )));
    }

    Ok(path.to_path_buf())
}

fn install_filename_from_path(path: &Path) -> Result<String, ConfigError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ConfigError::Invalid("skill_file_invalid_name".to_string()))?;
    crate::text::skill::validate_skill_basename(name)?;
    Ok(name.to_string())
}

fn catalog_install_filename(install_path: &str) -> Result<String, ConfigError> {
    let slug = normalize_catalog_slug(install_path);
    if slug.is_empty() {
        return Err(ConfigError::Invalid("deeplink_invalid_path".to_string()));
    }
    let filename = format!("{slug}.md");
    crate::text::skill::validate_skill_basename(&filename)?;
    Ok(filename)
}

fn decode_deeplink_token(token: &str) -> String {
    let trimmed = token.trim();
    if !trimmed.contains('%') {
        return trimmed.to_string();
    }

    let mut out = String::with_capacity(trimmed.len());
    let bytes = trimmed.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&trimmed[index + 1..index + 3], 16) {
                out.push(byte as char);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out.trim().to_string()
}

fn normalize_catalog_slug(raw: &str) -> String {
    let decoded = decode_deeplink_token(raw);
    let trimmed = decoded.trim_matches('/').trim();
    let slug = trimmed
        .strip_suffix(".md")
        .unwrap_or(trimmed)
        .trim();
    slug.to_string()
}

fn looks_like_catalog_slug(value: &str) -> bool {
    let value = value.trim().trim_matches('/');
    if value.is_empty() || value.contains("://") || value.contains('\\') {
        return false;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return false;
    }
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
}

/// Fallback when `url` crate leaves host empty (common for `veyro://translate-en.md/`).
fn catalog_slug_from_raw_deeplink(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix(VEYRO_SCHEME_PREFIX)?.trim();
    if rest.is_empty() {
        return None;
    }
    if rest.starts_with("import") && rest.contains('?') {
        return None;
    }
    let path_part = rest.split(&['?', '#'][..]).next()?.trim();
    let path_part = path_part.trim_start_matches('/');
    let segments: Vec<&str> = path_part
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() != 1 {
        return None;
    }
    let token = segments[0];
    if token.contains(':') {
        return None;
    }
    if !looks_like_catalog_slug(token) {
        return None;
    }
    Some(normalize_catalog_slug(token))
}

fn catalog_slug_from_veyro_url(url: &Url) -> Option<String> {
    if let Some(host) = url.host_str() {
        if host.len() == 1 && host.chars().next()?.is_ascii_alphabetic() {
            return None;
        }
        if matches!(host, "import" | "skill" | "open" | "localhost") {
            let path = url.path().trim_start_matches('/').trim_end_matches('/');
            if looks_like_catalog_slug(path) {
                return Some(normalize_catalog_slug(path));
            }
            return None;
        }
        if looks_like_catalog_slug(host) {
            return Some(normalize_catalog_slug(host));
        }
    }

    let path = url.path().trim_start_matches('/').trim_end_matches('/');
    if looks_like_catalog_slug(path) {
        return Some(normalize_catalog_slug(path));
    }

    None
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogListItem {
    id: String,
    partner_id: Option<String>,
    partner_install_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CatalogListResponse {
    items: Vec<CatalogListItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogDetail {
    title: String,
    description: Option<String>,
    content: String,
}

fn format_catalog_skill_markdown(detail: &CatalogDetail) -> String {
    let description = detail.description.as_deref().unwrap_or("").trim();
    let mut frontmatter = format!("---\nname: {}\n", escape_yaml_scalar(&detail.title));
    if !description.is_empty() {
        frontmatter.push_str(&format!(
            "description: {}\n",
            escape_yaml_scalar(description)
        ));
    }
    frontmatter.push_str("---\n\n");
    format!("{frontmatter}{}", detail.content.trim())
}

fn escape_yaml_scalar(value: &str) -> String {
    if value.contains([':', '#', '\n', '"']) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn is_catalog_publication_id(value: &str) -> bool {
    value.len() == 36
        && value
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        && value.chars().filter(|ch| *ch == '-').count() == 4
}

async fn download_catalog_skill(
    client: &reqwest::Client,
    install_path: &str,
) -> Result<PathBuf, ConfigError> {
    let slug = normalize_catalog_slug(install_path);
    if slug.is_empty() {
        return Err(ConfigError::Invalid("deeplink_invalid_path".to_string()));
    }

    let publication_id = if is_catalog_publication_id(&slug) {
        slug.clone()
    } else {
        resolve_catalog_publication_id(client, &slug).await?
    };

    let detail = client
        .get(format!("{AISTRUCTEDIT_CATALOG_ITEM}{publication_id}"))
        .timeout(Duration::from_secs(45))
        .send()
        .await
        .map_err(|error| ConfigError::Read(format!("deeplink_download_failed: {error}")))?
        .error_for_status()
        .map_err(|error| ConfigError::Read(format!("deeplink_download_failed: {error}")))?
        .json::<CatalogDetail>()
        .await
        .map_err(|error| ConfigError::Read(format!("deeplink_download_failed: {error}")))?;

    let markdown = format_catalog_skill_markdown(&detail);
    if markdown.len() as u64 > MAX_REMOTE_SKILL_BYTES {
        return Err(ConfigError::Invalid("deeplink_file_too_large".to_string()));
    }

    let temp_path = allocate_skill_import_temp_path(&format!("{slug}.md"))?;
    fs::write(&temp_path, markdown).map_err(|error| {
        ConfigError::Write(format!("{}: {error}", temp_path.display()))
    })?;

    if !temp_path.is_file() {
        return Err(ConfigError::Read(format!(
            "deeplink_staged_missing:{}",
            temp_path.display()
        )));
    }

    Ok(temp_path)
}

async fn resolve_catalog_publication_id(
    client: &reqwest::Client,
    slug: &str,
) -> Result<String, ConfigError> {
    let mut list_url =
        Url::parse("https://aistructedit.com/api/catalog").map_err(|error| {
            ConfigError::Read(format!("deeplink_download_failed: {error}"))
        })?;
    {
        let mut pairs = list_url.query_pairs_mut();
        pairs.append_pair("partnerId", AISTRUCTEDIT_PARTNER_ID);
        pairs.append_pair("partnerInstallPath", slug);
        pairs.append_pair("perPage", "20");
    }
    let list = client
        .get(list_url)
        .timeout(Duration::from_secs(45))
        .send()
        .await
        .map_err(|error| ConfigError::Read(format!("deeplink_download_failed: {error}")))?
        .error_for_status()
        .map_err(|error| ConfigError::Read(format!("deeplink_download_failed: {error}")))?
        .json::<CatalogListResponse>()
        .await
        .map_err(|error| ConfigError::Read(format!("deeplink_download_failed: {error}")))?;

    list.items
        .into_iter()
        .find(|item| {
            item.partner_id.as_deref() == Some(AISTRUCTEDIT_PARTNER_ID)
                && item
                    .partner_install_path
                    .as_deref()
                    .map(normalize_catalog_slug)
                    == Some(slug.to_string())
        })
        .map(|item| item.id)
        .ok_or_else(|| ConfigError::Invalid("deeplink_catalog_not_found".to_string()))
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

    let temp_path = allocate_skill_import_temp_path(&filename)?;
    fs::write(&temp_path, &bytes).map_err(|error| {
        ConfigError::Write(format!("{}: {error}", temp_path.display()))
    })?;

    Ok(temp_path)
}

fn skill_import_temp_dir() -> PathBuf {
    std::env::temp_dir().join("veyro-skill-import")
}

/// Each import gets its own temp file so cleanup or a retry never races on `{slug}.md`.
fn allocate_skill_import_temp_path(basename: &str) -> Result<PathBuf, ConfigError> {
    let temp_dir = skill_import_temp_dir();
    fs::create_dir_all(&temp_dir).map_err(|error| {
        ConfigError::Write(format!("{}: {error}", temp_dir.display()))
    })?;
    let seq = SKILL_IMPORT_TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let safe = sanitize_filename(basename);
    let name = if safe.is_empty() {
        format!("{pid}-{seq}.md")
    } else {
        format!("{pid}-{seq}-{safe}")
    };
    Ok(temp_dir.join(name))
}

fn filename_from_remote_url(url: &Url) -> Result<String, ConfigError> {
    let filename = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
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
            SkillSource::Local(_) | SkillSource::Catalog(_) => panic!("expected remote source"),
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
            SkillSource::Local(_) | SkillSource::Catalog(_) => panic!("expected remote source"),
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

    #[test]
    fn skill_import_temp_paths_are_unique_per_allocation() {
        let a = allocate_skill_import_temp_path("translate-pl.md").unwrap();
        let b = allocate_skill_import_temp_path("translate-pl.md").unwrap();
        assert_ne!(a, b);
        assert!(a.file_name().unwrap().to_string_lossy().contains("translate-pl.md"));
    }

    #[test]
    fn collects_veyro_urls_from_argv() {
        let urls = veyro_urls_from_cli_args([
            "veyro://import?url=https%3A%2F%2Fexample.com%2Fa.md",
            "--background",
        ]);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].starts_with("veyro://"));
    }

    #[test]
    fn parses_catalog_slug_from_host_md() {
        let source =
            parse_veyro_skill_source("veyro://translate-en.md/").unwrap();
        match source {
            SkillSource::Catalog(slug) => assert_eq!(slug, "translate-en"),
            other => panic!("expected catalog source, got {other:?}"),
        }
    }

    #[test]
    fn parses_catalog_slug_from_query_path() {
        let source =
            parse_veyro_skill_source("veyro://import?path=translate-en.md").unwrap();
        match source {
            SkillSource::Catalog(slug) => assert_eq!(slug, "translate-en"),
            other => panic!("expected catalog source, got {other:?}"),
        }
    }

    #[test]
    fn parse_does_not_fail_as_missing_local_file_for_catalog_link() {
        let parsed = parse_veyro_skill_source("veyro://translate-en.md/");
        assert!(
            parsed.is_ok(),
            "expected Ok source, got {parsed:?}"
        );
        match parsed.unwrap() {
            SkillSource::Catalog(slug) => assert_eq!(slug, "translate-en"),
            SkillSource::Local(path) => {
                assert!(
                    catalog_slug_from_raw_deeplink("veyro://translate-en.md/").is_some(),
                    "local path {path:?} should fall back via raw slug in prepare"
                );
            }
            other => panic!("unexpected source {other:?}"),
        }
    }

    #[test]
    fn parses_catalog_slug_when_url_crate_leaves_opaque_path() {
        let url = Url::parse("veyro://translate-en.md/").unwrap();
        if catalog_slug_from_veyro_url(&url).is_some() {
            let source = parse_veyro_skill_source("veyro://translate-en.md/").unwrap();
            match source {
                SkillSource::Catalog(slug) => assert_eq!(slug, "translate-en"),
                other => panic!("expected catalog source, got {other:?}"),
            }
            return;
        }
        assert_eq!(
            catalog_slug_from_raw_deeplink("veyro://translate-en.md/").as_deref(),
            Some("translate-en")
        );
        let source = parse_veyro_skill_source("veyro://translate-en.md/").unwrap();
        match source {
            SkillSource::Catalog(slug) => assert_eq!(slug, "translate-en"),
            other => panic!("expected catalog source, got {other:?}"),
        }
    }

    #[test]
    fn normalizes_catalog_slug_strips_md_suffix() {
        assert_eq!(normalize_catalog_slug("translate-en.md"), "translate-en");
        assert_eq!(normalize_catalog_slug("translate-en"), "translate-en");
    }
}
