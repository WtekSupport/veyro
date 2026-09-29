use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VoiceWatchTextModeOverride {
    #[default]
    Inherit,
    Original,
    Basic,
    /// Value stored as `skill:<filename>` in string form when needed; enum variant carries name.
    #[serde(alias = "skill")]
    Skill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VoiceWatchTextMode {
    Named(VoiceWatchTextModeOverride),
    /// Custom skill: `"skill:my-skill.md"` or plain skill filename via object.
    SkillName(String),
}

impl Default for VoiceWatchTextMode {
    fn default() -> Self {
        Self::Named(VoiceWatchTextModeOverride::Inherit)
    }
}

impl VoiceWatchTextMode {
    pub fn as_config_str(&self) -> String {
        match self {
            Self::Named(VoiceWatchTextModeOverride::Inherit) => "inherit".to_string(),
            Self::Named(VoiceWatchTextModeOverride::Original) => "original".to_string(),
            Self::Named(VoiceWatchTextModeOverride::Basic) => "basic".to_string(),
            Self::Named(VoiceWatchTextModeOverride::Skill) => "skill".to_string(),
            Self::SkillName(s) => {
                if s.starts_with("skill:") || s == "inherit" || s == "original" || s == "basic" {
                    s.clone()
                } else {
                    format!("skill:{s}")
                }
            }
        }
    }

    pub fn from_config_str(value: &str) -> Self {
        let trimmed = value.trim();
        match trimmed {
            "" | "inherit" => Self::Named(VoiceWatchTextModeOverride::Inherit),
            "original" => Self::Named(VoiceWatchTextModeOverride::Original),
            "basic" => Self::Named(VoiceWatchTextModeOverride::Basic),
            "skill" => Self::Named(VoiceWatchTextModeOverride::Skill),
            other if other.starts_with("skill:") => Self::SkillName(other.to_string()),
            other => Self::SkillName(format!("skill:{other}")),
        }
    }

    pub fn is_inherit(&self) -> bool {
        matches!(self, Self::Named(VoiceWatchTextModeOverride::Inherit))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VoiceWatchNotifyMode {
    Off,
    #[default]
    Result,
    ResultAndCopy,
}

impl VoiceWatchNotifyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Result => "result",
            Self::ResultAndCopy => "result_and_copy",
        }
    }
}

fn default_extensions() -> Vec<String> {
    vec![
        "ogg".to_string(),
        "oga".to_string(),
        "opus".to_string(),
    ]
}

fn default_stable_ms() -> u64 {
    1500
}

fn default_max_size_mb() -> u64 {
    50
}

fn default_max_duration_min() -> u64 {
    30
}

fn default_only_local() -> bool {
    true
}

fn default_history_days() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VoiceWatchSettings {
    pub enabled: bool,
    pub folders: Vec<String>,
    pub recursive: bool,
    pub extensions: Vec<String>,
    pub stable_ms: u64,
    pub max_size_mb: u64,
    pub max_duration_min: u64,
    pub name_filter: String,
    pub only_local_providers: bool,
    pub text_mode_override: String,
    pub notify: VoiceWatchNotifyMode,
    pub delete_source_after: bool,
    pub move_source_after: bool,
    pub history_retention_days: u32,
}

impl Default for VoiceWatchSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            folders: Vec::new(),
            recursive: false,
            extensions: default_extensions(),
            stable_ms: default_stable_ms(),
            max_size_mb: default_max_size_mb(),
            max_duration_min: default_max_duration_min(),
            name_filter: String::new(),
            only_local_providers: default_only_local(),
            text_mode_override: "inherit".to_string(),
            notify: VoiceWatchNotifyMode::default(),
            delete_source_after: false,
            move_source_after: false,
            history_retention_days: default_history_days(),
        }
    }
}

impl VoiceWatchSettings {
    pub fn text_mode(&self) -> VoiceWatchTextMode {
        VoiceWatchTextMode::from_config_str(&self.text_mode_override)
    }

    pub fn normalized_extensions(&self) -> Vec<String> {
        self.extensions
            .iter()
            .map(|ext| {
                ext.trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase()
            })
            .filter(|ext| !ext.is_empty())
            .collect()
    }

    pub fn max_size_bytes(&self) -> u64 {
        self.max_size_mb.saturating_mul(1024 * 1024)
    }

    pub fn max_duration_secs(&self) -> u64 {
        self.max_duration_min.saturating_mul(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_tz() {
        let s = VoiceWatchSettings::default();
        assert!(!s.enabled);
        assert_eq!(s.extensions, ["ogg", "oga", "opus"]);
        assert_eq!(s.stable_ms, 1500);
        assert_eq!(s.max_size_mb, 50);
        assert_eq!(s.max_duration_min, 30);
        assert!(s.only_local_providers);
        assert_eq!(s.text_mode_override, "inherit");
        assert_eq!(s.notify, VoiceWatchNotifyMode::Result);
        assert!(!s.delete_source_after);
        assert_eq!(s.history_retention_days, 30);
    }

    #[test]
    fn text_mode_roundtrip() {
        assert!(VoiceWatchTextMode::from_config_str("inherit").is_inherit());
        assert_eq!(
            VoiceWatchTextMode::from_config_str("skill:foo.md").as_config_str(),
            "skill:foo.md"
        );
    }
}
