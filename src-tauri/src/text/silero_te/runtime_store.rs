#![cfg(feature = "silero-te")]

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;
use zip::ZipArchive;

use crate::error::ConfigError;
use crate::settings::AppSettings;
use crate::transcription::model_store::{download_client, DownloadProgress, resolve_models_dir};

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
const DOWNLOAD_MAX_ATTEMPTS: u32 = 5;
const DOWNLOAD_RETRY_DELAY: Duration = Duration::from_secs(5);

pub const RELEASE_TAG: &str = "silero-te-runtime-v1";
pub const RUNTIME_ZIP: &str = "silero-te-runtime-win-x64.zip";
pub const MANIFEST_FILE: &str = "runtime-manifest.json";
pub const APPROX_RUNTIME_MB: u32 = 100;

const GITHUB_RELEASE_BASE: &str = "https://github.com/WtekSupport/veyro/releases/download";
const PROBE_DLL: &str = "torch_cpu.dll";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeManifest {
    files: Vec<RuntimeManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeManifestEntry {
    name: String,
    sha256: String,
}

pub fn runtime_dir(settings: &AppSettings) -> Result<PathBuf, ConfigError> {
    Ok(resolve_models_dir(settings)?.join("silero-te-runtime"))
}

pub fn runtime_ready(settings: &AppSettings) -> bool {
    if dev_exe_runtime_ready() {
        return true;
    }
    runtime_dir(settings)
        .ok()
        .is_some_and(|dir| manifest_on_disk_valid(&dir))
}

fn dev_exe_runtime_ready() -> bool {
    #[cfg(not(debug_assertions))]
    {
        return false;
    }
    #[cfg(debug_assertions)]
    {
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let Some(parent) = exe.parent() else {
            return false;
        };
        parent.join(PROBE_DLL).is_file()
    }
}

fn manifest_on_disk_valid(dir: &Path) -> bool {
    let manifest_path = dir.join(MANIFEST_FILE);
    let Ok(text) = fs::read_to_string(&manifest_path) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<RuntimeManifest>(&text) else {
        return false;
    };
    manifest.files.iter().all(|entry| {
        let path = dir.join(&entry.name);
        path.is_file() && file_sha256_hex(&path).is_ok_and(|hash| hash == entry.sha256)
    })
}

fn file_sha256_hex(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn download_urls(file_name: &str) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(base) = std::env::var("VEYRO_SILERO_TE_RUNTIME_BASE_URL") {
        let base = base.trim_end_matches('/');
        if !base.is_empty() {
            urls.push(format!("{base}/{file_name}"));
        }
    }
    urls.push(format!("{GITHUB_RELEASE_BASE}/{RELEASE_TAG}/{file_name}"));
    urls
}

pub fn configure_dll_search(settings: &AppSettings) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::System::LibraryLoader::SetDllDirectoryW;

        if dev_exe_runtime_ready() {
            let Ok(exe) = std::env::current_exe() else {
                return Ok(());
            };
            let Some(parent) = exe.parent() else {
                return Ok(());
            };
            let mut wide: Vec<u16> = parent.as_os_str().encode_wide().collect();
            wide.push(0);
            unsafe {
                let _ = SetDllDirectoryW(windows::core::PCWSTR(wide.as_ptr()));
            }
            return Ok(());
        }

        let dir = runtime_dir(settings).map_err(|error| error.to_string())?;
        if !dir.join(PROBE_DLL).is_file() {
            return Err(format!(
                "Silero TE runtime not found at {} — download required",
                dir.display()
            ));
        }
        let mut wide: Vec<u16> = dir.as_os_str().encode_wide().collect();
        wide.push(0);
        unsafe {
            let _ = SetDllDirectoryW(windows::core::PCWSTR(wide.as_ptr()));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = settings;
        Ok(())
    }
}

pub async fn download_runtime<F>(
    http: &Client,
    settings: &AppSettings,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress),
{
    if runtime_ready(settings) {
        if dev_exe_runtime_ready() {
            return std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf))
                .ok_or_else(|| "Silero TE runtime path unavailable".to_string());
        }
        return runtime_dir(settings).map_err(|error| error.to_string());
    }

    let dir = runtime_dir(settings).map_err(|error| error.to_string())?;
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;

    on_progress(DownloadProgress::new(0, None));
    tokio::time::sleep(Duration::from_millis(50)).await;

    let manifest = download_manifest(http).await?;
    let zip_bytes = download_bytes(http, RUNTIME_ZIP, &mut on_progress).await?;

    let staging = dir.with_extension("staging");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;

    extract_zip(&zip_bytes, &staging)?;

    for entry in &manifest.files {
        let extracted = staging.join(&entry.name);
        if !extracted.is_file() {
            return Err(format!(
                "runtime archive missing {} after extract",
                entry.name
            ));
        }
        let hash = file_sha256_hex(&extracted)?;
        if hash != entry.sha256 {
            return Err(format!(
                "runtime file {} checksum mismatch (expected {}, got {})",
                entry.name, entry.sha256, hash
            ));
        }
        let dest = dir.join(&entry.name);
        if dest.exists() {
            fs::remove_file(&dest).map_err(|error| error.to_string())?;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::rename(&extracted, &dest).map_err(|error| error.to_string())?;
    }

    fs::write(
        dir.join(MANIFEST_FILE),
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let _ = fs::remove_dir_all(&staging);

    if !manifest_on_disk_valid(&dir) {
        return Err("Silero TE runtime install finished but verification failed".to_string());
    }

    info!("Silero TE runtime ready at {}", dir.display());
    Ok(dir)
}

async fn download_manifest(http: &Client) -> Result<RuntimeManifest, String> {
    let bytes = download_bytes(http, MANIFEST_FILE, &mut |_| {}).await?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

async fn download_bytes<F>(
    http: &Client,
    file_name: &str,
    on_progress: &mut F,
) -> Result<Vec<u8>, String>
where
    F: FnMut(DownloadProgress),
{
    let urls = download_urls(file_name);
    let mut last_error = String::new();

    for url in urls {
        for attempt in 1..=DOWNLOAD_MAX_ATTEMPTS {
            match download_bytes_attempt(http, &url, on_progress).await {
                Ok(bytes) => return Ok(bytes),
                Err(error) => {
                    last_error = error;
                    if last_error.contains("404") {
                        break;
                    }
                    if attempt < DOWNLOAD_MAX_ATTEMPTS {
                        tokio::time::sleep(DOWNLOAD_RETRY_DELAY).await;
                    }
                }
            }
        }
    }

    Err(format!(
        "{last_error}. GitHub release `{RELEASE_TAG}` is missing `{file_name}` — run \
         `scripts/package-silero-te-runtime.ps1 -RepoRoot .` (requires staged libtorch DLLs)."
    ))
}

async fn download_bytes_attempt<F>(
    http: &Client,
    url: &str,
    on_progress: &mut F,
) -> Result<Vec<u8>, String>
where
    F: FnMut(DownloadProgress),
{
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "HTTP {} while downloading Silero TE runtime from {url}",
            response.status()
        ));
    }

    let total = response.content_length();
    let mut downloaded = 0_u64;
    let mut last_emit = Instant::now();
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();

    let mut emit = |downloaded: u64, total: Option<u64>, force: bool| {
        if force || last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
            on_progress(DownloadProgress::new(downloaded, total));
            last_emit = Instant::now();
        }
    };

    emit(0, total, true);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        buffer.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        emit(downloaded, total, false);
    }

    emit(downloaded, total, true);
    Ok(buffer)
}

fn extract_zip(bytes: &[u8], dest_dir: &Path) -> Result<(), String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|error| error.to_string())?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(name) = file.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let out_path = dest_dir.join(name);
        if file.is_dir() {
            fs::create_dir_all(&out_path).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut out = File::create(&out_path).map_err(|error| error.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn download_http_client() -> Result<Client, String> {
    download_client()
}
