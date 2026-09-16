use std::sync::OnceLock;

use sha2::{Digest, Sha256};

static HWID_HASH: OnceLock<String> = OnceLock::new();

pub fn hash() -> String {
    HWID_HASH.get_or_init(compute_hash).clone()
}

fn compute_hash() -> String {
    sha256_hex(&collect_material())
}

fn collect_material() -> String {
    let parts = platform_identifiers();
    if parts.is_empty() {
        return String::new();
    }
    parts.join("|")
}

fn platform_identifiers() -> Vec<String> {
    #[cfg(windows)]
    {
        return windows::identifiers();
    }
    #[cfg(target_os = "linux")]
    {
        return linux::identifiers();
    }
    #[cfg(target_os = "macos")]
    {
        return macos::identifiers();
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(windows)]
mod windows {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    pub fn identifiers() -> Vec<String> {
        let mut parts = Vec::new();
        if let Some(guid) = machine_guid() {
            parts.push(guid);
        }
        if let Some(serial) = boot_disk_serial() {
            parts.push(serial);
        }
        parts
    }

    fn machine_guid() -> Option<String> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let key = hklm
            .open_subkey("SOFTWARE\\Microsoft\\Cryptography")
            .ok()?;
        let guid: String = key.get_value("MachineGuid").ok()?;
        let guid = guid.trim().to_string();
        (!guid.is_empty()).then_some(guid)
    }

    fn boot_disk_serial() -> Option<String> {
        use serde::Deserialize;
        use wmi::{COMLibrary, WMIConnection};

        #[derive(Debug, Deserialize)]
        #[serde(rename = "Win32_DiskDrive")]
        #[serde(rename_all = "PascalCase")]
        struct DiskDrive {
            serial_number: Option<String>,
        }

        let com = COMLibrary::new().ok()?;
        let wmi = WMIConnection::new(com).ok()?;
        let drives: Vec<DiskDrive> = wmi.query().ok()?;

        drives.into_iter().find_map(|drive| {
            drive
                .serial_number
                .map(|serial| serial.trim().to_string())
                .filter(|serial| !serial.is_empty())
        })
    }
}

#[cfg(target_os = "linux")]
mod linux {
    pub fn identifiers() -> Vec<String> {
        std::fs::read_to_string("/etc/machine-id")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| vec![value])
            .unwrap_or_default()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    pub fn identifiers() -> Vec<String> {
        let output = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
            .ok()?;

        if !output.status.success() {
            return Vec::new();
        }

        let text = String::from_utf8_lossy(&output.stdout);
        parse_ioreg_uuid(&text).into_iter().collect()
    }

    fn parse_ioreg_uuid(text: &str) -> Option<String> {
        for line in text.lines() {
            let line = line.trim();
            if !line.contains("IOPlatformUUID") {
                continue;
            }
            let uuid = line
                .split('=')
                .nth(1)?
                .trim()
                .trim_matches('"')
                .trim()
                .to_string();
            if !uuid.is_empty() {
                return Some(uuid);
            }
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::parse_ioreg_uuid;

        #[test]
        fn parses_ioreg_uuid() {
            let sample = r#""IOPlatformUUID" = "12345678-ABCD-1234-ABCD-1234567890AB""#;
            assert_eq!(
                parse_ioreg_uuid(sample),
                Some("12345678-ABCD-1234-ABCD-1234567890AB".to_string())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_hex() {
        let hash = sha256_hex("test|material");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_eq!(hash, sha256_hex("test|material"));
    }

}
