use std::sync::atomic::{AtomicIsize, Ordering};

#[cfg(windows)]
use tracing::debug;

static INJECTION_TARGET: AtomicIsize = AtomicIsize::new(0);

/// Remember the foreground window when the user starts speaking (PTT press / VAD start).
pub fn capture_injection_target() {
    #[cfg(windows)]
    {
        if let Some(hwnd) = foreground_target_hwnd() {
            INJECTION_TARGET.store(hwnd, Ordering::SeqCst);
            debug!(hwnd, "captured injection target window");
        }
    }
}

/// Clear the remembered injection target (e.g. when opening settings).
pub fn clear_injection_target() {
    INJECTION_TARGET.store(0, Ordering::SeqCst);
}

/// Restore focus to the captured window before pasting text.
pub fn restore_injection_target() {
    #[cfg(windows)]
    {
        let hwnd = INJECTION_TARGET.load(Ordering::SeqCst);
        if hwnd == 0 {
            return;
        }
        if injection_target_already_active(hwnd) {
            debug!(
                hwnd,
                "injection target already foreground; skipping focus restore"
            );
            return;
        }
        if focus_hwnd(hwnd) {
            debug!(hwnd, "restored injection target window");
        } else {
            debug!(hwnd, "failed to restore injection target focus");
        }
    }
}

#[cfg(windows)]
fn injection_target_already_active(stored: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let target = HWND(stored as *mut _);
        if target.0.is_null() {
            return false;
        }
        let foreground = GetForegroundWindow();
        if foreground == target {
            return true;
        }
        let mut fg_pid = 0u32;
        let mut target_pid = 0u32;
        GetWindowThreadProcessId(foreground, Some(&mut fg_pid));
        GetWindowThreadProcessId(target, Some(&mut target_pid));
        fg_pid != 0 && fg_pid == target_pid
    }
}

#[cfg(windows)]
fn foreground_target_hwnd() -> Option<isize> {
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            debug!("skipping injection target capture: our own window is focused");
            return None;
        }

        Some(hwnd.0 as isize)
    }
}

#[cfg(windows)]
fn focus_hwnd(hwnd: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
        ShowWindow, SW_SHOW,
    };

    unsafe {
        let target = HWND(hwnd as *mut _);
        if target.0.is_null() {
            return false;
        }

        let foreground = GetForegroundWindow();
        let target_thread = GetWindowThreadProcessId(target, None);
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let current_thread = GetCurrentThreadId();

        if target_thread != foreground_thread {
            let _ = AttachThreadInput(current_thread, foreground_thread, true);
            let _ = AttachThreadInput(current_thread, target_thread, true);
        }

        let _ = ShowWindow(target, SW_SHOW);
        let _ = BringWindowToTop(target);
        let focused = SetForegroundWindow(target).as_bool();

        if target_thread != foreground_thread {
            let _ = AttachThreadInput(current_thread, target_thread, false);
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }

        focused
    }
}
