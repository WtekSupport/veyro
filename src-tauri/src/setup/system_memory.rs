/// Total installed physical RAM in megabytes (best effort).
pub fn total_physical_memory_mb() -> u64 {
    #[cfg(windows)]
    {
        return windows_total_mb().unwrap_or(default_assumed_mb());
    }
    #[cfg(target_os = "linux")]
    {
        return linux_total_mb().unwrap_or(default_assumed_mb());
    }
    #[cfg(target_os = "macos")]
    {
        return macos_total_mb().unwrap_or(default_assumed_mb());
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        default_assumed_mb()
    }
}

fn default_assumed_mb() -> u64 {
    8 * 1024
}

#[cfg(windows)]
fn windows_total_mb() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe {
        GlobalMemoryStatusEx(&mut status).ok()?;
    }
    Some(status.ullTotalPhys / (1024 * 1024))
}

#[cfg(target_os = "linux")]
fn linux_total_mb() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in contents.lines() {
        if let Some(kb) = line.strip_prefix("MemTotal:") {
            let kb: u64 = kb.trim().split_whitespace().next()?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn macos_total_mb() -> Option<u64> {
    use std::mem;
    use std::ptr;

    let mut size: u64 = 0;
    let mut len = mem::size_of::<u64>();
    let name = b"hw.memsize\0";
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr() as *const i8,
            &mut size as *mut u64 as *mut _,
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if rc == 0 {
        Some(size / (1024 * 1024))
    } else {
        None
    }
}
