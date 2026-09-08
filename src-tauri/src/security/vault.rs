//! AES-256-GCM authenticated encryption (the `Vault`).
//!
//! Every byte persisted by [`crate::security::store::FileEncryptedStore`] passes
//! through here, so data at rest is always encrypted with the password-derived
//! key (FR-5.2). GCM is authenticated: a wrong key or tampered ciphertext fails
//! [`decrypt`] instead of returning garbage (US-7.2 AC2).
//!
//! On-disk blob layout: `nonce (12 bytes) || ciphertext || tag (16 bytes)`.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use rand::RngCore;

use crate::core::error::{AppError, Result};
use crate::core::types::KeyHandle;

/// GCM standard nonce length.
const NONCE_LEN: usize = 12;

fn cipher(key: &KeyHandle) -> Result<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key.expose())
        .map_err(|_| AppError::Crypto("invalid key length".into()))
}

/// Encrypt `plaintext` with a fresh random nonce. Output is self-describing
/// (`nonce || ciphertext+tag`) so [`decrypt`] needs only the key.
pub fn encrypt(plaintext: &[u8], key: &KeyHandle) -> Result<Vec<u8>> {
    let cipher = cipher(key)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AppError::Crypto(format!("encrypt failed: {e}")))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt a blob produced by [`encrypt`]. Returns [`AppError::Crypto`] if the
/// key is wrong or the data was tampered with (GCM tag mismatch).
pub fn decrypt(blob: &[u8], key: &KeyHandle) -> Result<Vec<u8>> {
    if blob.len() < NONCE_LEN {
        return Err(AppError::Crypto("ciphertext too short".into()));
    }
    let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
    let cipher = cipher(key)?;
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|_| AppError::Crypto("decryption failed (wrong key or tampered data)".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_example() {
        let key = KeyHandle::new_for_test();
        let blob = encrypt(b"hello vault", &key).unwrap();
        assert_ne!(&blob[..], b"hello vault"); // actually encrypted
        assert_eq!(decrypt(&blob, &key).unwrap(), b"hello vault");
    }

    #[test]
    fn wrong_key_fails() {
        let k1 = KeyHandle::new_for_test();
        let k2 = KeyHandle::new_for_test();
        let blob = encrypt(b"secret", &k1).unwrap();
        assert!(decrypt(&blob, &k2).is_err());
    }

    #[test]
    fn tamper_fails() {
        let key = KeyHandle::new_for_test();
        let mut blob = encrypt(b"secret", &key).unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        assert!(decrypt(&blob, &key).is_err());
    }

    #[test]
    fn distinct_nonces_produce_distinct_ciphertexts() {
        let key = KeyHandle::new_for_test();
        let a = encrypt(b"same input", &key).unwrap();
        let b = encrypt(b"same input", &key).unwrap();
        assert_ne!(a, b, "nonce reuse would leak equality");
    }
}
