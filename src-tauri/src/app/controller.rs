use std::sync::{Arc, Mutex};

use tauri::AppHandle;
use tracing::{info, warn};

use crate::app::events::{
    emit_error, emit_listening_started, emit_listening_stopped, emit_state_changed,
};
use crate::app::runtime::PipelineRuntime;
use crate::app::state::{AppState, StatusSnapshot};
use crate::audio::capture::CaptureMode;
use crate::audio::pipeline::AudioPipeline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioAction {
    None,
    Enable,
    Disable,
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyPlan {
    None,
    Toggle(AudioAction),
    PttPress,
    PttRelease,
}

#[derive(Clone)]
pub struct SettingsUpdatePlan {
    pub settings: AppSettings,
    pub reregister_hotkey: bool,
    pub hotkey_spec_update: bool,
    pub audio_action: AudioAction,
    pub reload_transcriber: bool,
    pub reload_llm_engine: bool,
    /// Drop STT/LLM/TE weights from RAM (not only swap handles).
    pub purge_inference_memory: bool,
    pub capslock_ptt_changed: bool,
}
use crate::error::{AppError, AudioError};
use crate::i18n;
use crate::injection::TextInjector;
use crate::settings::{AppSettings, SettingsPatch, UiLocale};
use crate::tray;

pub struct AppController {
    state: AppState,
    settings: AppSettings,
    push_to_talk_active: bool,
    last_error: Option<String>,
    injection_available: bool,
    injection_backend: String,
}

impl AppController {
    pub fn new(settings: AppSettings, injector: Arc<dyn TextInjector>) -> Self {
        let backend = injector.backend_info();
        Self {
            state: AppState::Initializing,
            settings,
            push_to_talk_active: false,
            last_error: None,
            injection_available: backend.available,
            injection_backend: backend.backend,
        }
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn status(&self) -> StatusSnapshot {
        StatusSnapshot {
            state: self.state,
            enabled: true,
            push_to_talk: self.settings.push_to_talk,
            ptt_hold: self.settings.ptt_hold,
            hotkey: self.settings.global_hotkey.clone(),
            microphone_device: self.settings.microphone_device.clone(),
            language: self.settings.language.clone(),
            injection_mode: self.settings.injection_mode,
            last_error: self.last_error.clone(),
            injection_available: self.injection_available,
            injection_backend: self.injection_backend.clone(),
        }
    }

    pub fn startup_disabled(&mut self, app: &AppHandle) -> Result<(), AppError> {
        info!("starting voice input core (disabled)");
        self.transition(app, AppState::Disabled)
    }

    pub fn set_enabled_plan(&mut self, enabled: bool) -> Result<AudioAction, AppError> {
        if self.settings.enabled == enabled {
            return Ok(AudioAction::None);
        }
        Ok(if enabled {
            AudioAction::Enable
        } else {
            AudioAction::Disable
        })
    }

    pub fn plan_settings_update(&mut self, patch: SettingsPatch) -> Result<SettingsUpdatePlan, AppError> {
        let previous_hotkey = self.settings.global_hotkey.clone();
        let previous_device = self.settings.microphone_device.clone();
        let previous_ptt = self.settings.push_to_talk;
        let previous_silence = self.settings.silence_timeout_ms;
        let previous_vad_pre = self.settings.vad_pre_speech_buffer_ms;
        let previous_vad_min = self.settings.vad_minimum_speech_ms;
        let previous_vad_max = self.settings.vad_maximum_segment_ms;
        let previous_vad_engine = self.settings.vad_engine;
        let previous_vad_threshold_mode = self.settings.vad_threshold_mode;
        let previous_vad_voice_threshold = self.settings.vad_voice_threshold_percent;
        let previous_vad_auto_threshold = self.settings.vad_auto_threshold_percent;
        let previous_provider = self.settings.transcription_provider.clone();
        let previous_transcription_model = self.settings.transcription_model.clone();
        let previous_silero_te = self.settings.silero_te;
        let previous_variant = self.settings.local_stt_variant();
        let previous_data_storage_dir = self.settings.data_storage_dir.clone();
        let previous_models_dir = self.settings.local_whisper_models_dir.clone();
        let previous_use_gpu = self.settings.local_whisper_use_gpu;
        let previous_beam = self.settings.local_whisper_beam_size;
        let previous_sherpa_threads = self.settings.local_sherpa_num_threads;
        let previous_text_processing_mode = self.settings.text_processing_mode;
        let previous_rewrite_provider = self.settings.text_rewrite_provider;
        let previous_llm_model = self.settings.local_llm_model;
        let previous_llm_models_dir = self.settings.local_llm_models_dir.clone();
        let previous_llm_use_gpu = self.settings.local_llm_use_gpu;
        let previous_capslock = self.settings.capslock_ptt;
        let previous_hotkey_game_mode = self.settings.hotkey_game_mode;
        let previous_hotkey_block_system = self.settings.hotkey_block_system;
        let previous_ptt_hold = self.settings.ptt_hold;
        let previous_ui_mode = self.settings.ui_mode;
        let homemaker_data_storage = patch.homemaker_data_storage;
        let apply_homemaker_local_setup = patch.apply_homemaker_local_setup;

        patch.apply_to(&mut self.settings);
        crate::settings::apply_homemaker_patch_side_effects(
            &mut self.settings,
            previous_ui_mode,
            homemaker_data_storage,
            apply_homemaker_local_setup.unwrap_or(false),
        );
        crate::settings::normalize_locale_dependent_settings(&mut self.settings);
        self.settings.sync_capslock_hotkey(previous_capslock);
        self.settings.enabled = true;
        if self.settings.text_processing_mode.uses_ai()
            && !crate::llm::ai_rewrite_available(&self.settings)
        {
            self.settings.text_processing_mode = self.settings.canonical_light_cleanup_mode();
        }
        if !self.settings.push_to_talk && self.settings.text_processing_mode.uses_ai() {
            self.settings.text_processing_mode = self.settings.canonical_light_cleanup_mode();
        }
        self.settings.validate().map_err(AppError::from)?;

        let audio_action = if previous_device != self.settings.microphone_device
            || previous_ptt != self.settings.push_to_talk
            || previous_silence != self.settings.silence_timeout_ms
            || previous_vad_pre != self.settings.vad_pre_speech_buffer_ms
            || previous_vad_min != self.settings.vad_minimum_speech_ms
            || previous_vad_max != self.settings.vad_maximum_segment_ms
            || previous_vad_engine != self.settings.vad_engine
            || previous_vad_threshold_mode != self.settings.vad_threshold_mode
            || previous_vad_voice_threshold != self.settings.vad_voice_threshold_percent
            || previous_vad_auto_threshold != self.settings.vad_auto_threshold_percent
        {
            AudioAction::Restart
        } else {
            AudioAction::None
        };

        let reload_transcriber = previous_provider != self.settings.transcription_provider
            || previous_transcription_model != self.settings.transcription_model
            || previous_variant != self.settings.local_stt_variant()
            || previous_data_storage_dir != self.settings.data_storage_dir
            || previous_models_dir != self.settings.local_whisper_models_dir
            || previous_use_gpu != self.settings.local_whisper_use_gpu
            || previous_beam != self.settings.local_whisper_beam_size
            || previous_sherpa_threads != self.settings.local_sherpa_num_threads;

        let reload_llm_engine = previous_text_processing_mode != self.settings.text_processing_mode
            || previous_rewrite_provider != self.settings.text_rewrite_provider
            || previous_llm_model != self.settings.local_llm_model
            || previous_data_storage_dir != self.settings.data_storage_dir
            || previous_llm_models_dir != self.settings.local_llm_models_dir
            || previous_llm_use_gpu != self.settings.local_llm_use_gpu;

        let purge_inference_memory = reload_transcriber
            || reload_llm_engine
            || previous_silero_te != self.settings.silero_te;

        let hotkey_changed = previous_hotkey != self.settings.global_hotkey;
        let game_mode_changed = previous_hotkey_game_mode != self.settings.hotkey_game_mode;
        let block_system_changed =
            previous_hotkey_block_system != self.settings.hotkey_block_system;
        let ptt_hold_changed = previous_ptt_hold != self.settings.ptt_hold;

        Ok(SettingsUpdatePlan {
            settings: self.settings.clone(),
            reregister_hotkey: hotkey_changed || game_mode_changed || block_system_changed,
            hotkey_spec_update: (hotkey_changed || block_system_changed || ptt_hold_changed)
                && !game_mode_changed,
            audio_action,
            reload_transcriber,
            reload_llm_engine,
            purge_inference_memory,
            capslock_ptt_changed: previous_capslock != self.settings.capslock_ptt,
        })
    }

    pub fn capture_mode(&self) -> CaptureMode {
        if self.settings.push_to_talk {
            CaptureMode::PushToTalk
        } else {
            CaptureMode::Continuous
        }
    }

    pub fn finish_audio_action(
        &mut self,
        app: &AppHandle,
        action: AudioAction,
    ) -> Result<(), AppError> {
        match action {
            AudioAction::None => emit_state_changed(app, &self.status()),
            AudioAction::Enable => {
                self.settings.enabled = true;
                self.last_error = None;
                if self.state == AppState::Disabled {
                    self.transition(app, AppState::Initializing)?;
                }
                self.transition(app, AppState::Ready)?;
                info!("voice input enabled");
                let locale = self.settings.ui_locale;
                if self.settings.push_to_talk {
                    crate::notify::notify(
                        app,
                        &i18n::translate(locale, "notify.enabled", &[]),
                        &i18n::translate(
                            locale,
                            "notify.enabled_ptt",
                            &[("hotkey", &self.settings.global_hotkey)],
                        ),
                    );
                } else {
                    crate::notify::notify(
                        app,
                        &i18n::translate(locale, "notify.enabled", &[]),
                        &i18n::translate(locale, "notify.enabled_continuous", &[]),
                    );
                }
                tray::menu::refresh_tray_menu(app);
            }
            AudioAction::Disable => {
                self.settings.enabled = false;
                self.push_to_talk_active = false;
                self.transition(app, AppState::Disabled)?;
                info!("voice input disabled");
                crate::notify::notify(
                    app,
                    &i18n::translate(self.settings.ui_locale, "app.title", &[]),
                    &i18n::translate(self.settings.ui_locale, "notify.disabled", &[]),
                );
                tray::menu::refresh_tray_menu(app);
            }
            AudioAction::Restart => {
                self.settings.enabled = true;
                self.last_error = None;
                if self.state == AppState::Disabled
                    || self.state == AppState::Initializing
                    || self.state.can_transition_to(AppState::Ready)
                {
                    self.transition(app, AppState::Ready)?;
                } else {
                    emit_state_changed(app, &self.status());
                }
                tray::menu::refresh_tray_menu(app);
            }
        }
        Ok(())
    }

    pub fn is_ptt_pipeline_busy(&self) -> bool {
        matches!(
            self.state,
            AppState::Processing | AppState::Transcribing | AppState::Injecting
        )
    }

    /// New PTT capture may start only from a clean Ready state (not while listening or processing).
    pub fn can_start_new_ptt_capture(&self) -> bool {
        self.settings.enabled
            && self.settings.push_to_talk
            && self.state == AppState::Ready
            && !self.push_to_talk_active
            && !self.is_ptt_pipeline_busy()
    }

    pub fn plan_hotkey_pressed(&mut self) -> Result<HotkeyPlan, AppError> {
        if !self.settings.push_to_talk || !self.settings.enabled {
            return Ok(HotkeyPlan::None);
        }

        if !crate::hotkey::ptt_mode::press_to_toggle(&self.settings) {
            if !self.can_start_new_ptt_capture() {
                return Ok(HotkeyPlan::None);
            }
            self.push_to_talk_active = true;
            return Ok(HotkeyPlan::PttPress);
        }

        match self.state {
            AppState::Ready => {
                if !self.can_start_new_ptt_capture() {
                    return Ok(HotkeyPlan::None);
                }
                self.push_to_talk_active = true;
                Ok(HotkeyPlan::PttPress)
            }
            AppState::Listening => {
                self.push_to_talk_active = false;
                Ok(HotkeyPlan::PttRelease)
            }
            _ => Ok(HotkeyPlan::None),
        }
    }

    pub fn plan_hotkey_released(&mut self) -> Result<HotkeyPlan, AppError> {
        if !self.settings.push_to_talk || !self.push_to_talk_active {
            return Ok(HotkeyPlan::None);
        }
        // Hold mode ends the take on key release. Toggle mode stops on a second press only.
        if crate::hotkey::ptt_mode::press_to_toggle(&self.settings) {
            return Ok(HotkeyPlan::None);
        }
        // Always honor key-up in hold mode: close capture even if a prior VAD split is still
        // transcribing (common on slow machines when silence segmentation was enabled).
        if !crate::game_input::begin_toggle_stop() {
            return Ok(HotkeyPlan::None);
        }
        self.push_to_talk_active = false;
        Ok(HotkeyPlan::PttRelease)
    }

    pub fn finish_ptt_press(&mut self, app: &AppHandle) -> Result<UiLocale, AppError> {
        self.transition(app, AppState::Listening)?;
        emit_listening_started(app);
        Ok(self.settings.ui_locale)
    }

    pub fn finish_ptt_release(
        &mut self,
        app: &AppHandle,
        speech_queued: bool,
    ) -> Result<(), AppError> {
        if speech_queued {
            return Ok(());
        }

        match self.state {
            AppState::Listening => {
                emit_listening_stopped(app);
                self.recover_to_ready(app)?;
            }
            AppState::Processing => {
                self.recover_to_ready(app)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn begin_toggle_stop(&mut self) -> bool {
        if !self.settings().push_to_talk {
            return false;
        }
        let toggle = crate::hotkey::ptt_mode::press_to_toggle(self.settings());
        if !toggle && !crate::game_input::toggle_capture_active() {
            return false;
        }
        self.push_to_talk_active = false;
        true
    }

    pub fn abort_processing(&mut self, app: &AppHandle) -> Result<(), AppError> {
        self.push_to_talk_active = false;
        if self.state.can_transition_to(AppState::Ready) {
            self.transition(app, AppState::Ready)?;
        } else if self.state != AppState::Ready {
            self.state = AppState::Ready;
            emit_state_changed(app, &self.status());
        }
        Ok(())
    }

    pub fn reset_ptt_active(&mut self) {
        self.push_to_talk_active = false;
    }

    pub fn is_push_to_talk_active(&self) -> bool {
        self.push_to_talk_active
    }

    pub fn on_speech_started(&mut self, app: &AppHandle) -> Result<(), AppError> {
        if self.state == AppState::Ready {
            self.transition(app, AppState::Listening)?;
            emit_listening_started(app);
            tray::menu::refresh_tray_menu(app);
        }
        Ok(())
    }

    pub fn transition_only(&mut self, app: &AppHandle, next: AppState) -> Result<(), AppError> {
        self.transition(app, next)
    }

    pub fn recover_to_ready(&mut self, app: &AppHandle) -> Result<(), AppError> {
        self.recover_after_segment(app)
    }

    fn segment_recovery_target(&self) -> AppState {
        if self.push_to_talk_active {
            AppState::Listening
        } else {
            AppState::Ready
        }
    }

    /// After a queued segment finishes, return to capture UI when PTT is still active.
    pub fn recover_after_segment(&mut self, app: &AppHandle) -> Result<(), AppError> {
        let target = self.segment_recovery_target();

        if self.state == target {
            return Ok(());
        }

        if self.state.can_transition_to(target) {
            return self.transition(app, target);
        }

        if target == AppState::Listening {
            self.state = AppState::Listening;
            emit_state_changed(app, &self.status());
            tray::menu::refresh_tray_menu(app);
            return Ok(());
        }

        if self.state.can_transition_to(AppState::Ready) {
            self.transition(app, AppState::Ready)?;
        }
        Ok(())
    }

    pub fn recover_from_error(&mut self, app: &AppHandle) -> Result<AudioAction, AppError> {
        self.last_error = None;
        self.push_to_talk_active = false;

        if self.state.can_transition_to(AppState::Ready) {
            self.transition(app, AppState::Ready)?;
        } else if self.state != AppState::Ready {
            self.state = AppState::Ready;
            emit_state_changed(app, &self.status());
            tray::menu::refresh_tray_menu(app);
        }

        Ok(if self.settings.enabled {
            AudioAction::Restart
        } else {
            AudioAction::Enable
        })
    }

    fn transition(&mut self, app: &AppHandle, next: AppState) -> Result<(), AppError> {
        if !self.state.can_transition_to(next) {
            let error = AppError::InvalidTransition {
                from: format!("{:?}", self.state),
                to: format!("{:?}", next),
            };
            warn!("{error}");
            return Err(error);
        }

        self.state = next;
        emit_state_changed(app, &self.status());
        tray::menu::schedule_tray_visual(
            app,
            tray::menu::TrayVisualSnapshot {
                status: self.status(),
                locale: self.settings.ui_locale,
            },
        );
        Ok(())
    }

    pub fn report_error(&mut self, app: &AppHandle, error: AppError) {
        self.last_error = Some(error.to_string());
        let payload = error.to_payload();
        let next = match &error {
            AppError::Audio(AudioError::PermissionDenied) => AppState::PermissionRequired,
            AppError::Audio(AudioError::MicrophoneUnavailable) => AppState::MicrophoneUnavailable,
            AppError::Network(_) => AppState::NetworkUnavailable,
            _ => AppState::Error,
        };
        if self.state.can_transition_to(next) {
            self.state = next;
            emit_state_changed(app, &self.status());
        }
        emit_error(app, payload);
        tray::menu::refresh_tray_menu(app);
    }
}

pub type SharedController = Arc<Mutex<AppController>>;

pub fn with_controller<F, T>(controller: &SharedController, app: &AppHandle, f: F) -> Result<T, String>
where
    F: FnOnce(&mut AppController, &AppHandle) -> Result<T, AppError>,
{
    let mut guard = controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?;
    f(&mut guard, app).map_err(|error| error.to_string())
}

pub fn with_context<F, T>(ctx: &crate::app::context::AppContext, app: &AppHandle, f: F) -> Result<T, String>
where
    F: FnOnce(
        &mut AppController,
        &mut AudioPipeline,
        &PipelineRuntime,
        &AppHandle,
    ) -> Result<T, AppError>,
{
    let mut controller = ctx
        .controller
        .lock()
        .map_err(|_| "application controller lock poisoned".to_string())?;
    let mut audio = ctx
        .audio
        .lock()
        .map_err(|_| "audio pipeline lock poisoned".to_string())?;
    f(&mut controller, &mut audio, &ctx.runtime, app).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::injection::MockInjector;
    use crate::settings::{AppSettings, SettingsPatch};

    fn ptt_controller(ptt_hold: bool) -> AppController {
        let settings = AppSettings {
            push_to_talk: true,
            ptt_hold,
            enabled: true,
            ..Default::default()
        };
        let mut controller = AppController::new(settings, MockInjector::new());
        controller.state = AppState::Ready;
        controller
    }

    #[test]
    fn hold_mode_press_and_release() {
        let _lock = crate::game_input::toggle_test_lock();
        crate::game_input::reset_toggle_capture();
        let mut controller = ptt_controller(true);

        assert!(crate::game_input::begin_toggle_start());
        assert_eq!(
            controller.plan_hotkey_pressed().unwrap(),
            HotkeyPlan::PttPress
        );
        crate::game_input::confirm_toggle_start();
        controller.state = AppState::Listening;
        assert_eq!(
            controller.plan_hotkey_released().unwrap(),
            HotkeyPlan::PttRelease
        );
        crate::game_input::reset_toggle_capture();
    }

    #[test]
    fn toggle_mode_release_does_not_end_take() {
        let mut controller = ptt_controller(false);

        assert_eq!(
            controller.plan_hotkey_pressed().unwrap(),
            HotkeyPlan::PttPress
        );
        controller.state = AppState::Listening;

        assert_eq!(
            controller.plan_hotkey_released().unwrap(),
            HotkeyPlan::None
        );
        assert!(controller.push_to_talk_active);
    }

    #[test]
    fn scroll_lock_with_hold_ignores_second_press_until_release() {
        let _lock = crate::game_input::toggle_test_lock();
        crate::game_input::reset_toggle_capture();
        let settings = AppSettings {
            push_to_talk: true,
            ptt_hold: true,
            global_hotkey: "ScrollLock".to_string(),
            enabled: true,
            ..Default::default()
        };
        let mut controller = AppController::new(settings, MockInjector::new());
        controller.state = AppState::Ready;

        assert!(crate::game_input::begin_toggle_start());
        assert_eq!(
            controller.plan_hotkey_pressed().unwrap(),
            HotkeyPlan::PttPress
        );
        crate::game_input::confirm_toggle_start();
        controller.state = AppState::Listening;

        assert_eq!(controller.plan_hotkey_pressed().unwrap(), HotkeyPlan::None);
        crate::game_input::confirm_toggle_start();
        controller.state = AppState::Listening;
        assert_eq!(
            controller.plan_hotkey_released().unwrap(),
            HotkeyPlan::PttRelease
        );
        crate::game_input::reset_toggle_capture();
    }

    #[test]
    fn toggle_mode_second_press_also_ends_take() {
        let mut controller = ptt_controller(false);

        assert_eq!(
            controller.plan_hotkey_pressed().unwrap(),
            HotkeyPlan::PttPress
        );
        controller.state = AppState::Listening;

        assert_eq!(
            controller.plan_hotkey_pressed().unwrap(),
            HotkeyPlan::PttRelease
        );
        assert_eq!(
            controller.plan_hotkey_released().unwrap(),
            HotkeyPlan::None
        );
    }

    #[test]
    fn toggle_mode_press_during_processing_is_ignored() {
        let mut controller = ptt_controller(false);
        controller.state = AppState::Processing;

        assert_eq!(
            controller.plan_hotkey_pressed().unwrap(),
            HotkeyPlan::None
        );
    }

    #[test]
    fn hold_mode_press_during_processing_is_ignored() {
        let mut controller = ptt_controller(true);
        controller.state = AppState::Processing;

        assert_eq!(
            controller.plan_hotkey_pressed().unwrap(),
            HotkeyPlan::None
        );
    }

    #[test]
    fn hold_mode_release_during_processing_stops_capture() {
        let _lock = crate::game_input::toggle_test_lock();
        crate::game_input::reset_toggle_capture();
        assert!(crate::game_input::begin_toggle_start());
        crate::game_input::confirm_toggle_start();

        let mut controller = ptt_controller(true);
        controller.push_to_talk_active = true;
        controller.state = AppState::Processing;

        assert_eq!(
            controller.plan_hotkey_released().unwrap(),
            HotkeyPlan::PttRelease
        );
        assert!(!controller.push_to_talk_active);
        crate::game_input::reset_toggle_capture();
    }

    #[test]
    fn hold_mode_second_press_while_active_flag_is_ignored() {
        let mut controller = ptt_controller(true);
        controller.push_to_talk_active = true;
        controller.state = AppState::Ready;

        assert_eq!(
            controller.plan_hotkey_pressed().unwrap(),
            HotkeyPlan::None
        );
    }

    #[test]
    fn segment_recovery_target_listening_while_ptt_active() {
        let mut controller = ptt_controller(true);
        controller.push_to_talk_active = true;

        assert_eq!(controller.segment_recovery_target(), AppState::Listening);
    }

    #[test]
    fn segment_recovery_target_ready_when_ptt_inactive() {
        let controller = ptt_controller(true);

        assert_eq!(controller.segment_recovery_target(), AppState::Ready);
    }

    #[test]
    fn transcription_model_change_triggers_stt_reload() {
        let mut controller = AppController::new(AppSettings::default(), MockInjector::new());

        let plan = controller
            .plan_settings_update(SettingsPatch {
                transcription_model: Some("gpt-4o-mini-transcribe".to_string()),
                ..Default::default()
            })
            .expect("settings update should succeed");

        assert!(plan.reload_transcriber);
        assert!(plan.purge_inference_memory);
    }

    #[test]
    fn text_processing_mode_change_triggers_llm_reload() {
        let mut controller = AppController::new(
            AppSettings {
                text_processing_mode: crate::settings::TextProcessingMode::Basic,
                ..Default::default()
            },
            MockInjector::new(),
        );

        let plan = controller
            .plan_settings_update(SettingsPatch {
                text_processing_mode: Some(crate::settings::TextProcessingMode::Optimization),
                ..Default::default()
            })
            .expect("settings update should succeed");

        assert!(plan.reload_llm_engine);
    }

}
