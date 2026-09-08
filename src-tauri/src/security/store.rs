//! File-backed encrypted key/value store (`EncryptedStore` + `CredentialStore`).
//!
//! Every value is encrypted with the unlocked key before it touches disk, so
//! the on-disk data is always ciphertext (FR-5.2). Layout:
//! `<root>/<namespace>/<b64url(key)>.bin`. Keys are base64url-encoded into
//! filenames so any string key is representable and recoverable by [`list`].
//!
//! If the app is locked, every operation returns [`AppError::Locked`].
//!
//! [`list`]: EncryptedStore::list

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;

use crate::core::error::{AppError, Result};
use crate::core::traits::{CredentialStore, EncryptedStore};
use crate::core::types::{Credential, SourceKind};
use crate::security::key_manager::PasswordKeyManager;
use crate::security::vault;

const CREDENTIALS_NS: &str = "credentials";

/// Encrypted store persisting to the local filesystem under `root`.
pub struct FileEncryptedStore {
    root: PathBuf,
    keys: Arc<PasswordKeyManager>,
}

impl FileEncryptedStore {
    pub fn new(root: impl Into<PathBuf>, keys: Arc<PasswordKeyManager>) -> Self {
        Self {
            root: root.into(),
            keys,
        }
    }

    fn ns_dir(&self, ns: &str) -> PathBuf {
        // Namespaces are internal identifiers; encode to stay filesystem-safe.
        self.root.join(B64URL.encode(ns))
    }

    fn path(&self, ns: &str, key: &str) -> PathBuf {
        self.ns_dir(ns).join(format!("{}.bin", B64URL.encode(key)))
    }
}

#[async_trait]
impl EncryptedStore for FileEncryptedStore {
    async fn put(&self, ns: &str, key: &str, bytes: &[u8]) -> Result<()> {
        let handle = self.keys.current_key()?;
        let blob = vault::encrypt(bytes, &handle)?;
        let dir = self.ns_dir(ns);
        std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(format!("mkdir {ns}: {e}")))?;
        std::fs::write(self.path(ns, key), blob)
            .map_err(|e| AppError::Io(format!("write {ns}/{key}: {e}")))
    }

    async fn get(&self, ns: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let handle = self.keys.current_key()?;
        let path = self.path(ns, key);
        match std::fs::read(&path) {
            Ok(blob) => Ok(Some(vault::decrypt(&blob, &handle)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::Io(format!("read {ns}/{key}: {e}"))),
        }
    }

    async fn list(&self, ns: &str) -> Result<Vec<String>> {
        // Listing needs the app unlocked, matching put/get semantics.
        self.keys.current_key()?;
        let dir = self.ns_dir(ns);
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(AppError::Io(format!("list {ns}: {e}"))),
        };

        let mut keys = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| AppError::Io(format!("list {ns}: {e}")))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(stem) = name.strip_suffix(".bin") else {
                continue;
            };
            let decoded = B64URL
                .decode(stem)
                .map_err(|e| AppError::Serde(format!("key name: {e}")))?;
            keys.push(String::from_utf8_lossy(&decoded).into_owned());
        }
        Ok(keys)
    }

    async fn delete(&self, ns: &str, key: &str) -> Result<()> {
        match std::fs::remove_file(self.path(ns, key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::Io(format!("delete {ns}/{key}: {e}"))),
        }
    }
}

fn source_key(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Session => "Session",
        SourceKind::Notion => "Notion",
        SourceKind::Gmail => "Gmail",
        SourceKind::File => "File",
    }
}

#[async_trait]
impl CredentialStore for FileEncryptedStore {
    async fn store(&self, source: SourceKind, cred: Credential) -> Result<()> {
        let bytes = serde_json::to_vec(&cred).map_err(|e| AppError::Serde(e.to_string()))?;
        self.put(CREDENTIALS_NS, source_key(source), &bytes).await
    }

    async fn load(&self, source: SourceKind) -> Result<Option<Credential>> {
        match self.get(CREDENTIALS_NS, source_key(source)).await? {
            Some(bytes) => {
                let cred =
                    serde_json::from_slice(&bytes).map_err(|e| AppError::Serde(e.to_string()))?;
                Ok(Some(cred))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::traits::KeyManager;
    use tempfile::tempdir;

    fn unlocked_store() -> (tempfile::TempDir, FileEncryptedStore) {
        let dir = tempdir().unwrap();
        let km = Arc::new(PasswordKeyManager::new(dir.path()));
        km.setup("test-password").unwrap(); // leaves unlocked
        let store = FileEncryptedStore::new(dir.path().join("data"), km);
        (dir, store)
    }

    #[tokio::test]
    async fn put_get_roundtrip() {
        let (_d, store) = unlocked_store();
        store.put("facts", "a", b"payload").await.unwrap();
        assert_eq!(store.get("facts", "a").await.unwrap().unwrap(), b"payload");
    }

    #[tokio::test]
    async fn on_disk_bytes_are_encrypted() {
        let dir = tempdir().unwrap();
        let km = Arc::new(PasswordKeyManager::new(dir.path()));
        km.setup("pw").unwrap();
        let data_root = dir.path().join("data");
        let store = FileEncryptedStore::new(&data_root, km);
        store.put("facts", "a", b"PLAINTEXT-MARKER").await.unwrap();

        // Walk the data dir; no file should contain the plaintext marker.
        for entry in walk(&data_root) {
            let bytes = std::fs::read(&entry).unwrap();
            assert!(
                !bytes.windows(16).any(|w| w == b"PLAINTEXT-MARKER"),
                "plaintext leaked to {entry:?}"
            );
        }
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut out = vec![];
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.extend(walk(&p));
                } else {
                    out.push(p);
                }
            }
        }
        out
    }

    #[tokio::test]
    async fn list_recovers_keys() {
        let (_d, store) = unlocked_store();
        store.put("facts", "alpha/one", b"1").await.unwrap();
        store.put("facts", "beta two", b"2").await.unwrap();
        let mut keys = store.list("facts").await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["alpha/one".to_string(), "beta two".to_string()]);
    }

    #[tokio::test]
    async fn delete_removes() {
        let (_d, store) = unlocked_store();
        store.put("facts", "a", b"x").await.unwrap();
        store.delete("facts", "a").await.unwrap();
        assert!(store.get("facts", "a").await.unwrap().is_none());
        store.delete("facts", "a").await.unwrap(); // idempotent
    }

    #[tokio::test]
    async fn locked_store_denies_access() {
        let dir = tempdir().unwrap();
        let km = Arc::new(PasswordKeyManager::new(dir.path()));
        km.setup("pw").unwrap();
        km.lock();
        let store = FileEncryptedStore::new(dir.path().join("data"), km);
        assert!(matches!(
            store.put("facts", "a", b"x").await,
            Err(AppError::Locked)
        ));
        assert!(matches!(
            store.get("facts", "a").await,
            Err(AppError::Locked)
        ));
    }

    #[tokio::test]
    async fn credential_roundtrip() {
        let (_d, store) = unlocked_store();
        let cred = Credential(serde_json::json!({ "token": "abc123" }));
        store.store(SourceKind::Notion, cred.clone()).await.unwrap();
        let loaded = store.load(SourceKind::Notion).await.unwrap().unwrap();
        assert_eq!(loaded.0, cred.0);
        assert!(store.load(SourceKind::Gmail).await.unwrap().is_none());
    }
}
