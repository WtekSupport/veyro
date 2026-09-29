use std::path::Path;

use crate::settings::VoiceWatchSettings;

const TEMP_SUFFIXES: &[&str] = &[
    ".part",
    ".crdownload",
    ".tmp",
    ".download",
    ".partial",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileAcceptDecision {
    Accept,
    RejectTemp,
    RejectHidden,
    RejectExtension,
    RejectSize,
    RejectNameFilter,
    RejectZeroSize,
}

pub fn should_ignore_temp_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    TEMP_SUFFIXES.iter().any(|suffix| lower.ends_with(suffix))
        || lower.ends_with(".part.ogg")
        || file_name.starts_with('.')
        || file_name.starts_with("~$")
}

/// Simple glob: `*` = any run, `?` = one char. Case-insensitive on the file name.
/// If `pattern` looks like `/.../` treat as regex-lite: only `.*`, `.`, and literals (no full regex crate).
pub fn matches_name_filter(file_name: &str, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return true;
    }
    if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() >= 2 {
        let inner = &pattern[1..pattern.len() - 1];
        return simple_regex_match(file_name, inner);
    }
    glob_match(&file_name.to_ascii_lowercase(), &pattern.to_ascii_lowercase())
}

fn glob_match(text: &str, pattern: &str) -> bool {
    fn rec(t: &[u8], p: &[u8]) -> bool {
        let mut ti = 0;
        let mut pi = 0;
        let mut star_p = None;
        let mut star_t = 0;
        while ti < t.len() {
            if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
                ti += 1;
                pi += 1;
            } else if pi < p.len() && p[pi] == b'*' {
                star_p = Some(pi);
                star_t = ti;
                pi += 1;
            } else if let Some(sp) = star_p {
                pi = sp + 1;
                star_t += 1;
                ti = star_t;
            } else {
                return false;
            }
        }
        while pi < p.len() && p[pi] == b'*' {
            pi += 1;
        }
        pi == p.len()
    }
    rec(text.as_bytes(), pattern.as_bytes())
}

fn simple_regex_match(text: &str, pattern: &str) -> bool {
    // Convert a tiny subset: .* -> *, . -> ?, escape nothing else special except \.
    let mut glob = String::new();
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            glob.push('*');
            i += 2;
        } else if bytes[i] == b'.' {
            glob.push('?');
            i += 1;
        } else if bytes[i] == b'\\' && i + 1 < bytes.len() {
            glob.push(bytes[i + 1] as char);
            i += 2;
        } else {
            glob.push(bytes[i] as char);
            i += 1;
        }
    }
    glob_match(&text.to_ascii_lowercase(), &glob.to_ascii_lowercase())
}

pub fn filter_candidate(
    path: &Path,
    settings: &VoiceWatchSettings,
    size_bytes: u64,
) -> FileAcceptDecision {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return FileAcceptDecision::RejectHidden;
    };
    if should_ignore_temp_name(file_name) {
        return FileAcceptDecision::RejectTemp;
    }
    if size_bytes == 0 {
        return FileAcceptDecision::RejectZeroSize;
    }
    if size_bytes > settings.max_size_bytes() {
        return FileAcceptDecision::RejectSize;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let allowed = settings.normalized_extensions();
    if !allowed.iter().any(|a| a == &ext) {
        return FileAcceptDecision::RejectExtension;
    }
    if !matches_name_filter(file_name, &settings.name_filter) {
        return FileAcceptDecision::RejectNameFilter;
    }
    FileAcceptDecision::Accept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn ignores_temp_and_hidden() {
        assert!(should_ignore_temp_name("voice.ogg.part"));
        assert!(should_ignore_temp_name("file.crdownload"));
        assert!(should_ignore_temp_name(".hidden.ogg"));
        assert!(!should_ignore_temp_name("voice.ogg"));
    }

    #[test]
    fn extension_and_size() {
        let settings = VoiceWatchSettings::default();
        let path = PathBuf::from("C:/Downloads/voice.ogg");
        assert_eq!(
            filter_candidate(&path, &settings, 1024),
            FileAcceptDecision::Accept
        );
        assert_eq!(
            filter_candidate(&path, &settings, 0),
            FileAcceptDecision::RejectZeroSize
        );
        let big = settings.max_size_bytes() + 1;
        assert_eq!(
            filter_candidate(&path, &settings, big),
            FileAcceptDecision::RejectSize
        );
        let mp3 = PathBuf::from("C:/Downloads/voice.mp3");
        assert_eq!(
            filter_candidate(&mp3, &settings, 1024),
            FileAcceptDecision::RejectExtension
        );
    }

    #[test]
    fn name_filter_glob() {
        assert!(matches_name_filter("audio_2024.ogg", "audio_*"));
        assert!(!matches_name_filter("voice.ogg", "audio_*"));
        assert!(matches_name_filter("x.ogg", ""));
        assert!(matches_name_filter("audio_1.ogg", "/audio_.*\\.ogg/"));
    }
}
