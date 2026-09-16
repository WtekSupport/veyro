use std::sync::RwLock;

use keyring::Entry;

use crate::error::ConfigError;

const SERVICE: &str = "com.mkdee.veyro";
const USER: &str = "openai-api-key";

static API_KEY_CACHE: RwLock<Option<String>> = RwLock::new(None);

/// Whether a key is stored, tracked apart from the key itself so that checking for it never reads
/// the secret (which raises a Keychain access dialog for every new build).
static API_KEY_PRESENT: RwLock<Option<bool>> = RwLock::new(None);

pub fn save_api_key(key: &str) -> Result<(), ConfigError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::Invalid("api key must not be empty".to_string()));
    }

    Entry::new(SERVICE, USER)
        .map_err(|error| ConfigError::Write(error.to_string()))?
        .set_password(trimmed)
        .map_err(|error| ConfigError::Write(error.to_string()))?;

    if let Ok(mut cache) = API_KEY_CACHE.write() {
        *cache = Some(trimmed.to_string());
    }
    set_present(true);

    Ok(())
}

pub fn load_api_key() -> Result<String, ConfigError> {
    if let Ok(cache) = API_KEY_CACHE.read() {
        if let Some(key) = cache.as_ref() {
            return Ok(key.clone());
        }
    }

    let key = Entry::new(SERVICE, USER)
        .map_err(|error| ConfigError::Read(error.to_string()))?
        .get_password()
        .map_err(|error| ConfigError::Read(error.to_string()))?;

    if let Ok(mut cache) = API_KEY_CACHE.write() {
        *cache = Some(key.clone());
    }

    Ok(key)
}

pub fn clear_api_key() -> Result<(), ConfigError> {
    if let Ok(mut cache) = API_KEY_CACHE.write() {
        *cache = None;
    }
    set_present(false);

    match Entry::new(SERVICE, USER) {
        Ok(entry) => entry
            .delete_credential()
            .map_err(|error| ConfigError::Write(error.to_string())),
        Err(error) => Err(ConfigError::Write(error.to_string())),
    }
}

pub fn has_api_key() -> bool {
    if let Ok(cache) = API_KEY_CACHE.read() {
        if cache.is_some() {
            return true;
        }
    }
    if let Ok(present) = API_KEY_PRESENT.read() {
        if let Some(present) = *present {
            return present;
        }
    }
    let present = api_key_stored();
    set_present(present);
    present
}

fn set_present(present: bool) {
    if let Ok(mut cache) = API_KEY_PRESENT.write() {
        *cache = Some(present);
    }
}

/// Look the entry up by its attributes only; without reading the secret no dialog is shown.
#[cfg(target_os = "macos")]
fn api_key_stored() -> bool {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit};

    ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(SERVICE)
        .account(USER)
        .load_attributes(true)
        .limit(Limit::Max(1))
        .search()
        .is_ok_and(|items| !items.is_empty())
}

#[cfg(not(target_os = "macos"))]
fn api_key_stored() -> bool {
    load_api_key().is_ok()
}

#[cfg(test)]
pub fn reset_api_key_cache_for_tests() {
    if let Ok(mut cache) = API_KEY_CACHE.write() {
        *cache = None;
    }
    set_present(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_roundtrip_after_manual_update() {
        if let Ok(mut cache) = API_KEY_CACHE.write() {
            *cache = Some("cached-test-key".to_string());
        }
        assert!(has_api_key());
        if let Ok(key) = load_api_key() {
            assert_eq!(key, "cached-test-key");
        }
        if let Ok(mut cache) = API_KEY_CACHE.write() {
            *cache = None;
        }
    }
}
