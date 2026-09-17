//! Windows UIPI: low-level hooks do not receive input from higher-integrity foreground apps.

/// Mandatory label RID for elevated (admin) processes on Windows.
const HIGH_INTEGRITY_RID: u32 = 0x3000;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, TokenIntegrityLevel, SID_AND_ATTRIBUTES, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationMismatch {
    Ok,
    /// Foreground app is elevated but Veyro is not — PTT and injection into that app fail.
    VeyroNotElevated,
    /// Foreground integrity exceeds Veyro (rare; e.g. system-level process).
    ForegroundHigherIntegrity,
}

pub fn is_process_elevated() -> bool {
    process_integrity(unsafe { GetCurrentProcessId() })
        .is_some_and(|level| level >= HIGH_INTEGRITY_RID)
}

pub fn check_elevation_mismatch() -> ElevationMismatch {
    let Some(foreground) = foreground_process_integrity() else {
        return ElevationMismatch::Ok;
    };
    let Some(current) = process_integrity(unsafe { GetCurrentProcessId() }) else {
        return ElevationMismatch::Ok;
    };
    if foreground <= current {
        return ElevationMismatch::Ok;
    }
    if !is_process_elevated() {
        return ElevationMismatch::VeyroNotElevated;
    }
    ElevationMismatch::ForegroundHigherIntegrity
}

pub fn hotkeys_blocked_by_foreground_elevation() -> bool {
    !matches!(check_elevation_mismatch(), ElevationMismatch::Ok)
}

fn foreground_process_integrity() -> Option<u32> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        process_integrity(pid)
    }
}

fn process_integrity(pid: u32) -> Option<u32> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let integrity = read_integrity(process);
        let _ = CloseHandle(process);
        integrity
    }
}

fn read_integrity(process: HANDLE) -> Option<u32> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut buffer = vec![0u8; 256];
        let mut returned = 0u32;
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            &mut returned,
        )
        .ok()?;
        let _ = CloseHandle(token);

        let sid_and_attributes = &*(buffer.as_ptr() as *const SID_AND_ATTRIBUTES);
        let mut sid_string = windows::core::PWSTR::null();
        ConvertSidToStringSidW(sid_and_attributes.Sid, &mut sid_string).ok()?;
        let label = sid_string.to_string().ok()?;
        let _ = LocalFree(HLOCAL(sid_string.0 as _));
        parse_integrity_rid(&label)
    }
}

fn parse_integrity_rid(label: &str) -> Option<u32> {
    label.rsplit('-').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_current_process_integrity() {
        let _ = process_integrity(unsafe { GetCurrentProcessId() });
    }

    #[test]
    fn parses_integrity_sid_suffix() {
        assert_eq!(parse_integrity_rid("S-1-16-12288"), Some(12288));
    }

    #[test]
    fn high_integrity_rid_matches_admin_elevation() {
        assert_eq!(HIGH_INTEGRITY_RID, 12288);
    }

    #[test]
    fn ok_when_foreground_not_higher() {
        assert_eq!(check_elevation_mismatch(), ElevationMismatch::Ok);
    }
}
