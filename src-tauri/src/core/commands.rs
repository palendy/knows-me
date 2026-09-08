//! Command layer: the front-end-facing operations for U1 (onboarding, security,
//! settings, transfer transparency).
//!
//! These are plain `async` functions over [`AppState`] so they are unit-testable
//! without a GUI. The Tauri `#[tauri::command]` handlers in the `desktop` crate
//! are thin wrappers that call straight through to these.

use serde::{Deserialize, Serialize};

use crate::core::app_state::AppState;
use crate::core::error::Result;
use crate::core::traits::{CredentialStore, KeyManager};
use crate::core::types::{AppConfig, Credential, SourceKind, TransferPolicy, TransferRecord};

/// Snapshot the onboarding UI uses to decide which screen to show.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppStatus {
    /// A password has been set (keystore exists) → show *Unlock*, else *Setup*.
    pub initialized: bool,
    /// The key is currently in memory → the app is usable.
    pub unlocked: bool,
}

/// Current initialization/lock status (US-7.1/7.2 — which screen to render).
pub fn status(state: &AppState) -> AppStatus {
    let km = state.key_manager();
    AppStatus {
        initialized: km.is_initialized(),
        unlocked: km.is_unlocked(),
    }
}

/// First-run: set the password, initialize encryption, leave the app unlocked
/// and persist the default config (US-7.1).
pub async fn setup_password(state: &AppState, password: &str) -> Result<()> {
    state.key_manager().setup(password)?;
    state.save_config(AppConfig::default()).await?;
    Ok(())
}

/// Unlock with the password and restore persisted config (US-7.2).
pub async fn unlock(state: &AppState, password: &str) -> Result<()> {
    state.key_manager().unlock(password)?;
    state.restore_config().await;
    Ok(())
}

/// Drop the in-memory key (US-7.2).
pub fn lock(state: &AppState) {
    state.key_manager().lock();
}

/// Set the cloud-transfer policy (persisted).
pub async fn set_transfer_policy(state: &AppState, policy: TransferPolicy) -> Result<()> {
    let mut cfg = state.config();
    cfg.transfer_policy = policy;
    state.save_config(cfg).await
}

/// Enable/disable the local persona API server flag (persisted). The server
/// itself is owned by U4; this is the shared config toggle.
pub async fn set_server_enabled(state: &AppState, on: bool) -> Result<()> {
    let mut cfg = state.config();
    cfg.server_enabled = on;
    state.save_config(cfg).await
}

/// Store an external-system credential, encrypted (US-7.3).
pub async fn store_credential(
    state: &AppState,
    source: SourceKind,
    cred: Credential,
) -> Result<()> {
    state.store().store(source, cred).await
}

/// Load a stored credential (decrypted in memory only) (US-7.3).
pub async fn load_credential(state: &AppState, source: SourceKind) -> Result<Option<Credential>> {
    state.store().load(source).await
}

/// The transfer-transparency log: what has been sent to the cloud LLM (NFR-2).
pub fn list_transfers(state: &AppState) -> Vec<TransferRecord> {
    state.transfer_log().list()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn onboarding_flow_setup_then_unlock() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());

        assert_eq!(
            status(&state),
            AppStatus {
                initialized: false,
                unlocked: false
            }
        );

        setup_password(&state, "hunter2-hunter2").await.unwrap();
        assert_eq!(
            status(&state),
            AppStatus {
                initialized: true,
                unlocked: true
            }
        );

        lock(&state);
        assert!(!status(&state).unlocked);

        unlock(&state, "hunter2-hunter2").await.unwrap();
        assert!(status(&state).unlocked);
    }

    #[tokio::test]
    async fn wrong_password_unlock_fails() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());
        setup_password(&state, "correct-pass").await.unwrap();
        lock(&state);
        assert!(unlock(&state, "nope").await.is_err());
        assert!(!status(&state).unlocked);
    }

    #[tokio::test]
    async fn transfer_policy_can_be_changed() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());
        setup_password(&state, "pw-pw-pw").await.unwrap();
        set_transfer_policy(&state, TransferPolicy::AllowAll)
            .await
            .unwrap();
        assert_eq!(state.transfer_policy(), TransferPolicy::AllowAll);
    }

    #[tokio::test]
    async fn credential_store_and_load() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());
        setup_password(&state, "pw-pw-pw").await.unwrap();
        let cred = Credential(serde_json::json!({ "oauth": "tok" }));
        store_credential(&state, SourceKind::Gmail, cred.clone())
            .await
            .unwrap();
        let loaded = load_credential(&state, SourceKind::Gmail)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.0, cred.0);
    }
}
