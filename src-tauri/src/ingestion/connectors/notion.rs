//! Notion connector — **skeleton** (US-1.3). INTEGRATION-TODO.
//!
//! Contract-satisfying stub so the crate builds & the registry wires up. The
//! real implementation is added during integration; the shape below is the
//! agreed design so the next developer can fill it in directly.
//!
//! INTEGRATION-TODO(US-1.3):
//!  1. Enable `reqwest` + `oauth2` in Cargo.toml.
//!  2. Load the OAuth token from U1 `CredentialStore::load(SourceKind::Notion)`.
//!     If absent/expired → return `AppError::External("reauth required: notion")`
//!     so the UI can prompt re-auth (BR-C3). Never store the token in code/plaintext
//!     (BR-C2 / 심사기준⑥).
//!  3. Incremental sync: use `cursor` as the last `last_edited_time`; query the
//!     search/database endpoint for pages edited after it (BR-I / NFR-5).
//!  4. "Mine" filter (Q3=A / BR-C1): keep only pages the owner owns/edits.
//!  5. Map each page → `RawItem { source: Notion, external_id: page_id, text: Some(plain text) }`.
//!  6. Return the max `last_edited_time` as the next `Cursor`.

use async_trait::async_trait;

use crate::core::error::Result;
use crate::core::traits::{Connector, CredentialStore};
use crate::core::types::{Cursor, RawItem, SourceKind};
use std::sync::Arc;

/// Notion connector (skeleton). Holds the credential store it will use once the
/// real API calls are integrated.
pub struct NotionConnector {
    #[allow(dead_code)] // used once integration is wired up (INTEGRATION-TODO)
    credentials: Arc<dyn CredentialStore>,
}

impl NotionConnector {
    pub fn new(credentials: Arc<dyn CredentialStore>) -> Self {
        Self { credentials }
    }
}

#[async_trait]
impl Connector for NotionConnector {
    fn id(&self) -> SourceKind {
        SourceKind::Notion
    }

    async fn sync(&self, cursor: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)> {
        // INTEGRATION-TODO(US-1.3): real Notion API sync (see module docs).
        // Skeleton behavior: no new items, cursor unchanged (safe no-op so the
        // orchestrator and idempotency logic can be exercised end-to-end).
        Ok((Vec::new(), cursor.unwrap_or_default()))
    }

    fn supports_manual(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::InMemoryStore;
    // InMemoryStore is not a CredentialStore; use a tiny local stub instead.
    use crate::core::types::Credential;

    #[derive(Default)]
    struct NoCreds;
    #[async_trait]
    impl CredentialStore for NoCreds {
        async fn store(&self, _s: SourceKind, _c: Credential) -> Result<()> {
            Ok(())
        }
        async fn load(&self, _s: SourceKind) -> Result<Option<Credential>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn skeleton_is_safe_noop() {
        let _ = InMemoryStore::default(); // ensure mocks import path is valid
        let conn = NotionConnector::new(Arc::new(NoCreds));
        assert_eq!(conn.id(), SourceKind::Notion);
        let (items, _c) = conn.sync(None).await.unwrap();
        assert_eq!(items.len(), 0);
    }
}
