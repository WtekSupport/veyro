use crate::settings::config::{
    AppSettings, HomemakerDataStorage, TextProcessingMode, TextRewriteProvider, UiMode,
};
use crate::settings::secrets::has_api_key;
use crate::setup::apply_homemaker_local_recommendations;
use crate::setup::normalize_homemaker_hotkey;

/// Expert «Базовая очистка» ↔ standard «Оригинал» (same processing, different stored label).
pub fn map_text_processing_mode_for_ui_transition(
    mode: TextProcessingMode,
    from: UiMode,
    to: UiMode,
) -> TextProcessingMode {
    if from == to {
        return mode;
    }
    match (from, to) {
        (UiMode::Expert, UiMode::Homemaker) if mode == TextProcessingMode::Basic => {
            TextProcessingMode::Original
        }
        (UiMode::Homemaker, UiMode::Expert) if mode == TextProcessingMode::Original => {
            TextProcessingMode::Basic
        }
        _ => mode,
    }
}

pub fn normalize_homemaker_settings(settings: &mut AppSettings) -> bool {
    if !settings.is_homemaker() {
        return false;
    }

    let mut changed = false;

    match settings.text_processing_mode {
        TextProcessingMode::Original
        | TextProcessingMode::Optimization
        | TextProcessingMode::CustomSkill => {}
        TextProcessingMode::Basic => {
            settings.text_processing_mode = TextProcessingMode::Original;
            changed = true;
        }
    }

    let transcription_cloud = settings.transcription_provider != "local";
    let rewrite_cloud = matches!(
        settings.text_rewrite_provider,
        TextRewriteProvider::Openai
    );

    if transcription_cloud != rewrite_cloud {
        if transcription_cloud && has_api_key() {
            settings.text_rewrite_provider = TextRewriteProvider::Openai;
        } else if rewrite_cloud && has_api_key() {
            settings.transcription_provider = "openai".to_string();
        } else {
            settings.transcription_provider = "local".to_string();
            settings.text_rewrite_provider = TextRewriteProvider::Local;
        }
        changed = true;
    }

    if normalize_homemaker_hotkey(settings) {
        changed = true;
    }

    if settings.hotkey_game_mode {
        settings.hotkey_game_mode = false;
        changed = true;
    }

    // Standard UI: one press starts capture, the same key again stops (ScrollLock/CapsLock friendly).
    if settings.ptt_hold {
        settings.ptt_hold = false;
        changed = true;
    }

    if settings.uses_local_storage()
        && matches!(
            settings.text_processing_mode,
            TextProcessingMode::Original | TextProcessingMode::Basic
        )
        && !settings.silero_te
    {
        settings.silero_te = true;
        changed = true;
    }

    changed
}

pub fn apply_homemaker_patch_side_effects(
    settings: &mut AppSettings,
    previous_ui_mode: UiMode,
    homemaker_data_storage: Option<HomemakerDataStorage>,
    apply_local_setup: bool,
) -> bool {
    let mut changed = false;
    if previous_ui_mode != settings.ui_mode {
        let mapped = map_text_processing_mode_for_ui_transition(
            settings.text_processing_mode,
            previous_ui_mode,
            settings.ui_mode,
        );
        if mapped != settings.text_processing_mode {
            settings.text_processing_mode = mapped;
            changed = true;
        }
    }

    changed |= normalize_homemaker_settings(settings);

    let switched_to_homemaker =
        previous_ui_mode != UiMode::Homemaker && settings.ui_mode == UiMode::Homemaker;
    let selected_local = matches!(
        homemaker_data_storage,
        Some(HomemakerDataStorage::Local)
    );

    if apply_local_setup || selected_local || (switched_to_homemaker && settings.uses_local_storage())
    {
        changed |= apply_homemaker_local_recommendations(settings);
        changed |= normalize_homemaker_settings(settings);
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::TextProcessingMode;

    #[test]
    fn homemaker_maps_basic_to_original() {
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            text_processing_mode: TextProcessingMode::Basic,
            transcription_provider: "local".to_string(),
            text_rewrite_provider: TextRewriteProvider::Local,
            ..Default::default()
        };
        assert!(normalize_homemaker_settings(&mut settings));
        assert_eq!(
            settings.text_processing_mode,
            TextProcessingMode::Original
        );
    }

    #[test]
    fn ui_transition_maps_expert_basic_to_homemaker_original() {
        assert_eq!(
            map_text_processing_mode_for_ui_transition(
                TextProcessingMode::Basic,
                UiMode::Expert,
                UiMode::Homemaker,
            ),
            TextProcessingMode::Original
        );
    }

    #[test]
    fn ui_transition_maps_homemaker_original_to_expert_basic() {
        assert_eq!(
            map_text_processing_mode_for_ui_transition(
                TextProcessingMode::Original,
                UiMode::Homemaker,
                UiMode::Expert,
            ),
            TextProcessingMode::Basic
        );
    }

    #[test]
    fn ui_transition_keeps_expert_original_when_entering_homemaker() {
        assert_eq!(
            map_text_processing_mode_for_ui_transition(
                TextProcessingMode::Original,
                UiMode::Expert,
                UiMode::Homemaker,
            ),
            TextProcessingMode::Original
        );
    }

    #[test]
    fn homemaker_normalizes_custom_hotkey_to_preset() {
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            global_hotkey: "Ctrl+C".to_string(),
            transcription_provider: "local".to_string(),
            text_rewrite_provider: TextRewriteProvider::Local,
            ..Default::default()
        };
        assert!(normalize_homemaker_settings(&mut settings));
        assert_eq!(settings.global_hotkey, "CapsLock");
    }

    #[test]
    fn homemaker_unifies_mismatched_providers_to_local_without_api_key() {
        crate::settings::secrets::reset_api_key_cache_for_tests();
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            transcription_provider: "openai".to_string(),
            text_rewrite_provider: TextRewriteProvider::Local,
            ..Default::default()
        };
        assert!(normalize_homemaker_settings(&mut settings));
        assert_eq!(settings.transcription_provider, "local");
        assert_eq!(settings.text_rewrite_provider, TextRewriteProvider::Local);
    }

    #[test]
    fn homemaker_disables_game_mode() {
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            hotkey_game_mode: true,
            transcription_provider: "local".to_string(),
            text_rewrite_provider: TextRewriteProvider::Local,
            ..Default::default()
        };
        assert!(normalize_homemaker_settings(&mut settings));
        assert!(!settings.hotkey_game_mode);
    }

    #[test]
    fn homemaker_uses_toggle_ptt_not_hold() {
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            ptt_hold: true,
            ..Default::default()
        };
        assert!(normalize_homemaker_settings(&mut settings));
        assert!(!settings.ptt_hold);
    }

    #[test]
    fn homemaker_keeps_custom_skill_mode() {
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            text_processing_mode: TextProcessingMode::CustomSkill,
            ai_rewrite_skill: Some("my-skill.md".to_string()),
            transcription_provider: "local".to_string(),
            text_rewrite_provider: TextRewriteProvider::Local,
            ..Default::default()
        };
        assert!(!normalize_homemaker_settings(&mut settings));
        assert_eq!(
            settings.text_processing_mode,
            TextProcessingMode::CustomSkill
        );
    }

    #[test]
    fn homemaker_enables_silero_te_for_local_basic() {
        let mut settings = AppSettings {
            ui_mode: UiMode::Homemaker,
            transcription_provider: "local".to_string(),
            text_rewrite_provider: TextRewriteProvider::Local,
            text_processing_mode: TextProcessingMode::Basic,
            silero_te: false,
            ..Default::default()
        };
        assert!(normalize_homemaker_settings(&mut settings));
        assert!(settings.silero_te);
    }

}
