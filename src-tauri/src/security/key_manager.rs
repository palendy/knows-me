//! Password-based key management (`KeyManager`).
//!
//! Flow (FR-5.3, US-7.1/7.2):
//! 1. **setup** — generate a random salt, derive a 256-bit key from the password
//!    via Argon2id, and persist `salt` + an encrypted *verifier* blob. The
//!    plaintext key is **never** written to disk (US-7.1 AC2).
//! 2. **unlock** — re-derive the key from the entered password and prove it by
//!    decrypting the verifier. A wrong password fails the GCM tag and never
//!    exposes data (US-7.2 AC2).
//! 3. **lock** — drop the in-memory key (zeroized on drop).
//!
//! The derived key lives only in memory, inside a [`KeyHandle`].

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use argon2::Argon2;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, Result};
use crate::core::traits::KeyManager;
use crate::core::types::KeyHandle;
use crate::security::vault;

/// Constant plaintext encrypted at setup and decrypted at unlock to verify the
/// password without storing it.
const VERIFIER_MAGIC: &[u8] = b"knows-me-vault-verifier-v1";
const SALT_LEN: usize = 16;
const KEYSTORE_FILE: &str = "keystore.json";

/// Persisted, non-secret key metadata. Contains no plaintext key material.
#[derive(Serialize, Deserialize)]
struct KeyStoreFile {
    version: u8,
    /// Base64 random salt fed to the KDF.
    salt: String,
    /// Base64 `encrypt(VERIFIER_MAGIC, key)` — proves password correctness.
    verifier: String,
}

/// Argon2id-backed [`KeyManager`]. Holds the unlocked key in memory only.
pub struct PasswordKeyManager {
    keystore_path: PathBuf,
    current: RwLock<Option<KeyHandle>>,
}

impl PasswordKeyManager {
    /// Create a manager whose keystore lives under `data_dir`.
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            keystore_path: data_dir.as_ref().join(KEYSTORE_FILE),
            current: RwLock::new(None),
        }
    }

    /// Whether first-run setup has already happened (keystore exists).
    pub fn is_initialized(&self) -> bool {
        self.keystore_path.exists()
    }

    /// The currently-unlocked key, or [`AppError::Locked`] if the app is locked.
    /// Crate-internal: used by the encrypted store/vault.
    pub(crate) fn current_key(&self) -> Result<KeyHandle> {
        self.current
            .read()
            .expect("key lock poisoned")
            .clone()
            .ok_or(AppError::Locked)
    }

    fn derive_key(password: &str, salt: &[u8]) -> Result<KeyHandle> {
        let mut key = [0u8; 32];
        Argon2::default()
            .hash_password_into(password.as_bytes(), salt, &mut key)
            .map_err(|e| AppError::Crypto(format!("key derivation failed: {e}")))?;
        Ok(KeyHandle::from_bytes(key))
    }

    fn read_keystore(&self) -> Result<KeyStoreFile> {
        let raw = std::fs::read(&self.keystore_path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::InvalidInput("not initialized: set a password first".into())
            }
            _ => AppError::Io(format!("read keystore: {e}")),
        })?;
        serde_json::from_slice(&raw).map_err(|e| AppError::Serde(format!("keystore: {e}")))
    }
}

impl KeyManager for PasswordKeyManager {
    fn setup(&self, password: &str) -> Result<()> {
        if password.is_empty() {
            return Err(AppError::InvalidInput("password must not be empty".into()));
        }
        if self.is_initialized() {
            return Err(AppError::InvalidInput(
                "already initialized: use unlock".into(),
            ));
        }

        let mut salt = [0u8; SALT_LEN];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let key = Self::derive_key(password, &salt)?;
        let verifier = vault::encrypt(VERIFIER_MAGIC, &key)?;

        let file = KeyStoreFile {
            version: 1,
            salt: B64.encode(salt),
            verifier: B64.encode(&verifier),
        };
        if let Some(parent) = self.keystore_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Io(format!("mkdir: {e}")))?;
        }
        let json = serde_json::to_vec_pretty(&file)
            .map_err(|e| AppError::Serde(format!("keystore: {e}")))?;
        std::fs::write(&self.keystore_path, json)
            .map_err(|e| AppError::Io(format!("write keystore: {e}")))?;

        // First-run leaves the app unlocked so onboarding flows straight in.
        *self.current.write().expect("key lock poisoned") = Some(key);
        Ok(())
    }

    fn unlock(&self, password: &str) -> Result<KeyHandle> {
        let file = self.read_keystore()?;
        let salt = B64
            .decode(&file.salt)
            .map_err(|e| AppError::Serde(format!("salt: {e}")))?;
        let verifier = B64
            .decode(&file.verifier)
            .map_err(|e| AppError::Serde(format!("verifier: {e}")))?;

        let key = Self::derive_key(password, &salt)?;
        // Correct password ⟺ verifier decrypts to the known magic.
        match vault::decrypt(&verifier, &key) {
            Ok(plain) if plain == VERIFIER_MAGIC => {
                *self.current.write().expect("key lock poisoned") = Some(key.clone());
                Ok(key)
            }
            _ => Err(AppError::InvalidInput("incorrect password".into())),
        }
    }

    fn lock(&self) {
        *self.current.write().expect("key lock poisoned") = None;
    }

    fn is_unlocked(&self) -> bool {
        self.current.read().expect("key lock poisoned").is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn setup_then_unlock_roundtrip() {
        let dir = tempdir().unwrap();
        let km = PasswordKeyManager::new(dir.path());
        assert!(!km.is_initialized());

        km.setup("correct horse battery staple").unwrap();
        assert!(km.is_initialized());
        assert!(km.is_unlocked()); // setup leaves it unlocked

        km.lock();
        assert!(!km.is_unlocked());

        km.unlock("correct horse battery staple").unwrap();
        assert!(km.is_unlocked());
    }

    #[test]
    fn wrong_password_is_rejected_and_stays_locked() {
        let dir = tempdir().unwrap();
        let km = PasswordKeyManager::new(dir.path());
        km.setup("right-password").unwrap();
        km.lock();

        let res = km.unlock("wrong-password");
        assert!(matches!(res, Err(AppError::InvalidInput(_))));
        assert!(!km.is_unlocked());
    }

    #[test]
    fn plaintext_key_never_touches_disk() {
        let dir = tempdir().unwrap();
        let km = PasswordKeyManager::new(dir.path());
        km.setup("s3cr3t-passphrase-value").unwrap();

        let bytes = std::fs::read(dir.path().join(KEYSTORE_FILE)).unwrap();
        // Neither the password nor a raw derived key should appear on disk.
        assert!(!bytes.windows(24).any(|w| w == b"s3cr3t-passphrase-value\0"));
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("s3cr3t-passphrase-value"));
    }

    #[test]
    fn cannot_setup_twice() {
        let dir = tempdir().unwrap();
        let km = PasswordKeyManager::new(dir.path());
        km.setup("first").unwrap();
        assert!(km.setup("second").is_err());
    }

    #[test]
    fn unlock_before_setup_errors() {
        let dir = tempdir().unwrap();
        let km = PasswordKeyManager::new(dir.path());
        assert!(km.unlock("whatever").is_err());
    }
}
