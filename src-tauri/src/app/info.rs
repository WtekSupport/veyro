use serde::{Deserialize, Serialize};

use crate::i18n;
use crate::settings::UiLocale;

include!(concat!(env!("OUT_DIR"), "/app_version.rs"));

const THIRD_PARTY_LICENSES_JSON: &str =
    include_str!("../../resources/legal/third-party-licenses.json");

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub version_display: String,
    pub version_semver: String,
    pub hwid_hash: String,
    pub publisher: String,
    pub copyright: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyLicense {
    pub name: String,
    pub copyright: String,
    pub license: String,
    pub url: String,
}

pub fn app_info() -> AppInfo {
    AppInfo {
        version_display: VERSION_DISPLAY.to_string(),
        version_semver: VERSION_SEMVER.to_string(),
        hwid_hash: crate::hwid::hash(),
        publisher: "MKDee Tech Solutions".to_string(),
        copyright: "© 2026 MKDee Tech Solutions. All rights reserved.".to_string(),
    }
}

#[cfg(windows)]
fn is_running_elevated() -> bool {
    crate::game_input::elevation_win::is_process_elevated()
}

#[cfg(not(windows))]
fn is_running_elevated() -> bool {
    false
}

pub fn window_title_with_elevation(base: &str, locale: UiLocale) -> String {
    if is_running_elevated() {
        format!(
            "{base}{}",
            i18n::translate(locale, "app.title.admin_suffix", &[])
        )
    } else {
        base.to_string()
    }
}

pub fn main_window_title(app_name: &str, locale: UiLocale) -> String {
    window_title_with_elevation(&format!("{app_name} {VERSION_SEMVER}"), locale)
}

pub fn about_window_title(locale: UiLocale) -> String {
    window_title_with_elevation(&i18n::translate(locale, "app.about.title", &[]), locale)
}

pub fn third_party_licenses() -> Vec<ThirdPartyLicense> {
    serde_json::from_str(THIRD_PARTY_LICENSES_JSON).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_title_includes_version() {
        let title = main_window_title("Veyro", UiLocale::En);
        assert!(title.starts_with("Veyro "));
        assert!(title.contains(VERSION_SEMVER));
    }

    #[test]
    fn admin_suffix_is_localized() {
        let base = "Veyro 1.0.0";
        assert!(window_title_with_elevation(base, UiLocale::En).starts_with(base));
        assert!(window_title_with_elevation(base, UiLocale::Ru).starts_with(base));
    }
}
