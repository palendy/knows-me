//! Application-wide state (`AppState`): the wiring that holds the security,
//! storage, masking and LLM-transparency components together.
//!
//! Cheaply cloneable (everything is behind `Arc`) so it can be shared into Tauri
//! command handlers, the scheduler, and background jobs.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::core::error::Result;
use crate::core::traits::EncryptedStore;
use crate::core::types::{AppConfig, TransferPolicy};
use crate::llm::{RegexMasker, TransferLog};
use crate::security::{FileEncryptedStore, PasswordKeyManager};

const APP_NS: &str = "app";
const CONFIG_KEY: &str = "config";

#[derive(Clone)]
pub struct AppState {
    key_manager: Arc<PasswordKeyManager>,
    store: Arc<FileEncryptedStore>,
    masker: Arc<RegexMasker>,
    transfer_log: Arc<TransferLog>,
    config: Arc<RwLock<AppConfig>>,
}

impl AppState {
    /// Wire up all components rooted at `data_dir`. The keystore lives directly
    /// under `data_dir`; encrypted values go under `data_dir/store`.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let key_manager = Arc::new(PasswordKeyManager::new(&data_dir));
        let store = Arc::new(FileEncryptedStore::new(
            data_dir.join("store"),
            key_manager.clone(),
        ));
        Self {
            key_manager,
            store,
            masker: Arc::new(RegexMasker::new()),
            transfer_log: Arc::new(TransferLog::new()),
            config: Arc::new(RwLock::new(AppConfig::default())),
        }
    }

    pub fn key_manager(&self) -> Arc<PasswordKeyManager> {
        self.key_manager.clone()
    }

    pub fn store(&self) -> Arc<FileEncryptedStore> {
        self.store.clone()
    }

    pub fn masker(&self) -> Arc<RegexMasker> {
        self.masker.clone()
    }

    pub fn transfer_log(&self) -> Arc<TransferLog> {
        self.transfer_log.clone()
    }

    /// A snapshot of the current configuration.
    pub fn config(&self) -> AppConfig {
        self.config.read().expect("config lock poisoned").clone()
    }

    /// Persist the given config (encrypted) and commit it in memory on success.
    /// Requires the app to be unlocked (the store needs the key).
    ///
    /// Mirrors the LLM selection to the process environment so the gateway's
    /// `from_env` paths pick up the change on the next client build — see
    /// [`AppConfig::apply_to_env`]. Only the persisted non-secret fields go to
    /// the environment; the API key is the desktop shell's responsibility.
    pub async fn save_config(&self, next: AppConfig) -> Result<()> {
        let bytes = serde_json::to_vec(&next)
            .map_err(|e| crate::core::error::AppError::Serde(e.to_string()))?;
        self.store.put(APP_NS, CONFIG_KEY, &bytes).await?;
        next.apply_to_env();
        *self.config.write().expect("config lock poisoned") = next;
        Ok(())
    }

    /// Load persisted config after unlock; falls back to the default silently if
    /// none was stored yet. Either way the resulting LLM selection is applied to
    /// the process environment so a saved provider/model wins over `.env`.
    pub async fn restore_config(&self) {
        if let Ok(Some(bytes)) = self.store.get(APP_NS, CONFIG_KEY).await {
            if let Ok(cfg) = serde_json::from_slice::<AppConfig>(&bytes) {
                *self.config.write().expect("config lock poisoned") = cfg;
            }
        }
        self.config().apply_to_env();
    }

    /// Convenience: current transfer policy.
    pub fn transfer_policy(&self) -> TransferPolicy {
        self.config().transfer_policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::traits::KeyManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn config_persists_across_lock_unlock() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());
        state.key_manager().setup("pw").unwrap();

        let mut cfg = state.config();
        cfg.transfer_policy = TransferPolicy::LocalOnlyNoLlm;
        state.save_config(cfg).await.unwrap();

        state.key_manager().lock();
        // Fresh state over the same dir simulates an app restart.
        let reopened = AppState::new(dir.path());
        reopened.key_manager().unlock("pw").unwrap();
        reopened.restore_config().await;
        assert_eq!(reopened.transfer_policy(), TransferPolicy::LocalOnlyNoLlm);
    }

    #[tokio::test]
    async fn save_config_requires_unlock() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());
        state.key_manager().setup("pw").unwrap();
        state.key_manager().lock();
        assert!(state.save_config(AppConfig::default()).await.is_err());
    }
}
