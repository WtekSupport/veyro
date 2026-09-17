use std::sync::Mutex;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use getrandom::getrandom;
use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

const SERVICE: &str = "com.mkdee.veyro";
const KEY_USER: &str = "settings-encryption-key";
const ENVELOPE_VERSION: u32 = 1;

/// The settings key, read from the Keychain once per launch.
static SETTINGS_KEY: Mutex<Option<[u8; 32]>> = Mutex::new(None);

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    pub v: u32,
    pub nonce: String,
    pub data: String,
}

pub fn encrypt_json(plaintext: &str) -> Result<EncryptedEnvelope, ConfigError> {
    let key = load_or_create_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|error| ConfigError::Write(error.to_string()))?;
    let mut nonce_bytes = [0u8; 12];
    getrandom(&mut nonce_bytes).map_err(|error| ConfigError::Write(error.to_string()))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|error| ConfigError::Write(error.to_string()))?;

    Ok(EncryptedEnvelope {
        v: ENVELOPE_VERSION,
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce_bytes),
        data: base64::engine::general_purpose::STANDARD.encode(ciphertext),
    })
}

pub fn decrypt_json(envelope: &EncryptedEnvelope) -> Result<String, ConfigError> {
    if envelope.v != ENVELOPE_VERSION {
        return Err(ConfigError::Invalid(format!(
            "unsupported settings envelope version {}",
            envelope.v
        )));
    }

    let key = load_or_create_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|error| ConfigError::Read(error.to_string()))?;
    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(&envelope.nonce)
        .map_err(|error| ConfigError::Read(error.to_string()))?;
    if nonce_bytes.len() != 12 {
        return Err(ConfigError::Read("invalid settings nonce length".to_string()));
    }
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&envelope.data)
        .map_err(|error| ConfigError::Read(error.to_string()))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|error| ConfigError::Read(error.to_string()))?;

    String::from_utf8(plaintext).map_err(|error| ConfigError::Read(error.to_string()))
}

fn load_or_create_key() -> Result<[u8; 32], ConfigError> {
    // Holding the lock across the Keychain call also keeps concurrent callers from each raising
    // their own Keychain access dialog.
    let mut cached = SETTINGS_KEY
        .lock()
        .map_err(|_| ConfigError::Read("settings key lock poisoned".to_string()))?;
    if let Some(key) = *cached {
        return Ok(key);
    }

    let entry = Entry::new(SERVICE, KEY_USER).map_err(|error| ConfigError::Read(error.to_string()))?;
    let key = match stored_key(entry.get_password())? {
        Some(key) => key,
        None => {
            let mut key = [0u8; 32];
            getrandom(&mut key).map_err(|error| ConfigError::Write(error.to_string()))?;
            let encoded = base64::engine::general_purpose::STANDARD.encode(key);
            entry
                .set_password(&encoded)
                .map_err(|error| ConfigError::Write(error.to_string()))?;
            key
        }
    };
    *cached = Some(key);
    Ok(key)
}

/// `Ok(None)` only when no key is stored yet. Any other failure (Keychain access declined,
/// keychain locked) is an error: generating a new key then would overwrite the stored one and
/// leave the existing config.json unreadable.
fn stored_key(lookup: Result<String, keyring::Error>) -> Result<Option<[u8; 32]>, ConfigError> {
    match lookup {
        Ok(stored) => decode_key(&stored).map(Some),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(ConfigError::Read(format!(
            "settings key unavailable: {error}"
        ))),
    }
}

fn decode_key(stored: &str) -> Result<[u8; 32], ConfigError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(stored.trim())
        .map_err(|error| ConfigError::Read(error.to_string()))?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| ConfigError::Read("invalid settings encryption key length".to_string()))?;
    Ok(array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrip_with_fixed_key() {
        let key = [7u8; 32];
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let nonce_bytes = [1u8; 12];
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = r#"{"ui_locale":"ru"}"#;
        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes()).unwrap();

        let envelope = EncryptedEnvelope {
            v: ENVELOPE_VERSION,
            nonce: base64::engine::general_purpose::STANDARD.encode(nonce_bytes),
            data: base64::engine::general_purpose::STANDARD.encode(ciphertext),
        };

        let decoded_nonce = base64::engine::general_purpose::STANDARD
            .decode(&envelope.nonce)
            .unwrap();
        let decoded_data = base64::engine::general_purpose::STANDARD
            .decode(&envelope.data)
            .unwrap();
        let recovered = cipher
            .decrypt(Nonce::from_slice(&decoded_nonce), decoded_data.as_ref())
            .unwrap();

        assert_eq!(String::from_utf8(recovered).unwrap(), plaintext);
    }

    #[test]
    fn missing_entry_means_first_run() {
        assert!(matches!(stored_key(Err(keyring::Error::NoEntry)), Ok(None)));
    }

    #[test]
    fn declined_or_failed_keychain_access_is_an_error_not_a_new_key() {
        let declined = keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("declined")));
        assert!(stored_key(Err(declined)).is_err());
        let locked = keyring::Error::PlatformFailure(Box::new(std::io::Error::other("locked")));
        assert!(stored_key(Err(locked)).is_err());
    }

    #[test]
    fn stored_key_is_decoded() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        assert_eq!(stored_key(Ok(encoded)).unwrap(), Some([7u8; 32]));
    }
}
