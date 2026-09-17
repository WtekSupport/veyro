use std::sync::atomic::{AtomicBool, Ordering};

use enigo::{Direction, Enigo, Key, Keyboard, Settings};

use super::injector::InjectionError;

/// Only the first `Enigo` of a session may show the macOS "grant Accessibility" dialog.
static PERMISSION_PROMPT_USED: AtomicBool = AtomicBool::new(false);

pub fn send_return() -> Result<(), InjectionError> {
    with_enigo(|enigo| {
        enigo
            .key(Key::Return, Direction::Click)
            .map_err(|error| InjectionError::Keyboard(error.to_string()))
    })
}

pub fn send_backspaces(char_count: u32) -> Result<(), InjectionError> {
    if char_count == 0 {
        return Ok(());
    }

    with_enigo(|enigo| {
        for _ in 0..char_count {
            enigo
                .key(Key::Backspace, Direction::Click)
                .map_err(|error| InjectionError::Keyboard(error.to_string()))?;
        }
        Ok(())
    })
}

/// Run `f` against a fresh `Enigo`.
///
/// On macOS this runs on the main thread: `Enigo::new` and layout lookups call AppKit/HIToolbox,
/// which trap with SIGTRAP ("Block was expected to execute on queue [com.apple.main-thread]")
/// when called from the injection thread while Veyro itself is frontmost.
pub fn with_enigo<R, F>(f: F) -> Result<R, InjectionError>
where
    F: FnOnce(&mut Enigo) -> Result<R, InjectionError> + Send,
    R: Send,
{
    #[cfg(target_os = "macos")]
    {
        dispatch2::run_on_main(|_| run_with_enigo(f))
    }
    #[cfg(not(target_os = "macos"))]
    {
        run_with_enigo(f)
    }
}

fn run_with_enigo<R>(
    f: impl FnOnce(&mut Enigo) -> Result<R, InjectionError>,
) -> Result<R, InjectionError> {
    // enigo re-shows the dialog on every `Enigo::new` while permission is missing, i.e. once per
    // dictated phrase. Later failures surface as injection errors instead.
    let settings = Settings {
        open_prompt_to_get_permissions: !PERMISSION_PROMPT_USED.swap(true, Ordering::Relaxed),
        ..Settings::default()
    };
    let mut enigo =
        Enigo::new(&settings).map_err(|error| InjectionError::Keyboard(error.to_string()))?;
    f(&mut enigo)
}
