pub mod binding;
pub mod capslock;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod low_level;
pub mod manager;
pub mod normalize;
pub mod ptt_mode;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod runtime;

pub fn game_mode_supported() -> bool {
    #[cfg(any(windows, target_os = "macos"))]
    {
        true
    }
    #[cfg(target_os = "linux")]
    {
        !crate::injection::linux_wayland::is_wayland_session()
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        false
    }
}
