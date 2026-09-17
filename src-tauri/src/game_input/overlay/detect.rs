#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Windowed,
    ExclusiveFullscreen,
}

pub fn current_display_mode() -> DisplayMode {
    #[cfg(windows)]
    {
        return windows_display_mode();
    }
    #[cfg(not(windows))]
    {
        DisplayMode::Windowed
    }
}

#[cfg(windows)]
fn windows_display_mode() -> DisplayMode {
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE,
    };

    unsafe {
        if let Ok(state) = SHQueryUserNotificationState() {
            if state == QUERY_USER_NOTIFICATION_STATE(2) {
                return DisplayMode::ExclusiveFullscreen;
            }
        }
    }
    DisplayMode::Windowed
}
