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
use crate::ingestion::connectors::spec::{
    credential_satisfies, credential_spec, verify_credentials, FieldSpec,
};

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

/// Remove a stored credential — disconnect a source (US-7.3). Idempotent.
pub async fn delete_credential(state: &AppState, source: SourceKind) -> Result<()> {
    state.store().delete(source).await
}

// --- U2: source connection catalog ----------------------------------------

/// One source's connection status for the sources screen. Secret values are
/// never included — only whether the required fields are filled — so a stored
/// token cannot leak back through this read (write-only credential handling).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceStatus {
    pub kind: SourceKind,
    /// The credential fields this source declares (form template for the UI).
    pub fields: Vec<FieldSpec>,
    /// A credential is stored for this source.
    pub connected: bool,
    /// Every required field is satisfied → the source can be collected.
    pub ready: bool,
}

/// Which sources the app can connect. The order is the display order.
const CATALOG: [SourceKind; 4] = [
    SourceKind::Session,
    SourceKind::File,
    SourceKind::Notion,
    SourceKind::Gmail,
];

/// The connection status of every catalog source. Reads (not writes) the
/// encrypted credential store, so it requires an unlocked vault.
pub async fn list_sources(state: &AppState) -> Result<Vec<SourceStatus>> {
    let store = state.store();
    let mut out = Vec::with_capacity(CATALOG.len());
    for kind in CATALOG {
        let cred = store.load(kind).await?;
        let fields = credential_spec(kind);
        out.push(SourceStatus {
            kind,
            // Credential-less sources are never "connected" in the credential
            // sense; their readiness comes from having no required fields.
            connected: cred.is_some(),
            ready: credential_satisfies(kind, cred.as_ref()),
            fields,
        });
    }
    Ok(out)
}

/// Store the credential for a source (connect). Rejects sources that declare no
/// fields — Session/File are always ready and take no credentials.
///
/// Before persisting, the credential is validated (required fields, plus a live
/// handshake where the connector supports it) so a bad token is rejected up
/// front instead of only surfacing on the first sync.
pub async fn connect_source(
    state: &AppState,
    source: SourceKind,
    values: serde_json::Value,
) -> Result<String> {
    if credential_spec(source).is_empty() {
        return Err(crate::core::error::AppError::InvalidInput(format!(
            "{source:?} takes no credentials"
        )));
    }
    let note = verify_credentials(source, &values).await?;
    state.store().store(source, Credential(values)).await?;
    Ok(note)
}

/// Remove a source's credential (disconnect). Idempotent.
pub async fn disconnect_source(state: &AppState, source: SourceKind) -> Result<()> {
    state.store().delete(source).await
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

    #[tokio::test]
    async fn list_sources_reflects_connection_state() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());
        setup_password(&state, "pw-pw-pw").await.unwrap();

        let before = list_sources(&state).await.unwrap();
        let notion = before
            .iter()
            .find(|s| s.kind == SourceKind::Notion)
            .unwrap();
        assert!(!notion.connected);
        assert!(!notion.ready);
        assert!(!notion.fields.is_empty());
        // Credential-less sources are ready without being "connected".
        let session = before
            .iter()
            .find(|s| s.kind == SourceKind::Session)
            .unwrap();
        assert!(session.ready);
        assert!(session.fields.is_empty());

        connect_source(
            &state,
            SourceKind::Notion,
            serde_json::json!({ "token": "secret_x" }),
        )
        .await
        .unwrap();
        let after = list_sources(&state).await.unwrap();
        let notion = after.iter().find(|s| s.kind == SourceKind::Notion).unwrap();
        assert!(notion.connected);
        assert!(notion.ready);
    }

    #[tokio::test]
    async fn connect_then_disconnect_source() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());
        setup_password(&state, "pw-pw-pw").await.unwrap();

        connect_source(
            &state,
            SourceKind::Gmail,
            serde_json::json!({ "address": "me@gmail.com", "app_password": "pw" }),
        )
        .await
        .unwrap();
        assert!(load_credential(&state, SourceKind::Gmail)
            .await
            .unwrap()
            .is_some());

        disconnect_source(&state, SourceKind::Gmail).await.unwrap();
        assert!(load_credential(&state, SourceKind::Gmail)
            .await
            .unwrap()
            .is_none());
        // Idempotent.
        disconnect_source(&state, SourceKind::Gmail).await.unwrap();
    }

    #[tokio::test]
    async fn connect_rejects_credential_less_source() {
        let dir = tempdir().unwrap();
        let state = AppState::new(dir.path());
        setup_password(&state, "pw-pw-pw").await.unwrap();
        assert!(
            connect_source(&state, SourceKind::Session, serde_json::json!({}))
                .await
                .is_err()
        );
    }
}
