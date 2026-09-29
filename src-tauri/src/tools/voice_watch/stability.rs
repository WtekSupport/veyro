use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StabilityOutcome {
    Stable { size: u64, mtime: SystemTime },
    Disappeared,
    TimedOut,
}

/// Wait until size and mtime are unchanged for `stable_ms`, and the file can be opened for read.
pub fn wait_until_stable(path: &Path, stable_ms: u64) -> StabilityOutcome {
    let stable_for = Duration::from_millis(stable_ms.max(100));
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_size: Option<u64> = None;
    let mut last_mtime: Option<SystemTime> = None;
    let mut stable_since: Option<Instant> = None;
    let mut backoff_ms = 50_u64;

    while Instant::now() < deadline {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return StabilityOutcome::Disappeared;
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(backoff_ms));
                backoff_ms = (backoff_ms * 2).min(1000);
                continue;
            }
        };
        if !meta.is_file() {
            return StabilityOutcome::Disappeared;
        }
        let size = meta.len();
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        if last_size == Some(size) && last_mtime == Some(mtime) {
            if stable_since.is_none() {
                stable_since = Some(Instant::now());
            }
            if stable_since.is_some_and(|t| t.elapsed() >= stable_for) {
                match try_open_read(path) {
                    Ok(()) => {
                        return StabilityOutcome::Stable { size, mtime };
                    }
                    Err(_) => {
                        stable_since = None;
                        thread::sleep(Duration::from_millis(backoff_ms));
                        backoff_ms = (backoff_ms * 2).min(1000);
                        continue;
                    }
                }
            }
        } else {
            last_size = Some(size);
            last_mtime = Some(mtime);
            stable_since = None;
            backoff_ms = 50;
        }
        thread::sleep(Duration::from_millis(100));
    }
    StabilityOutcome::TimedOut
}

fn try_open_read(path: &Path) -> std::io::Result<()> {
    let file = File::open(path)?;
    drop(file);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn waits_for_stable_file() {
        let dir = std::env::temp_dir().join(format!(
            "veyro-stable-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("voice.ogg");
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(b"abc").unwrap();
        }
        let outcome = wait_until_stable(&path, 200);
        let _ = fs::remove_dir_all(&dir);
        assert!(matches!(outcome, StabilityOutcome::Stable { size: 3, .. }));
    }

    #[test]
    fn disappeared_file() {
        let dir = std::env::temp_dir().join(format!(
            "veyro-stable-miss-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("missing.ogg");
        let outcome = wait_until_stable(&path, 200);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(outcome, StabilityOutcome::Disappeared);
    }
}
