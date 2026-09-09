//! Gmail connector — **skeleton** (US-1.4). INTEGRATION-TODO.
//!
//! INTEGRATION-TODO(US-1.4):
//!  1. Enable `reqwest` + `oauth2` in Cargo.toml.
//!  2. Load OAuth token from U1 `CredentialStore::load(SourceKind::Gmail)`;
//!     absent/expired → `AppError::External("reauth required: gmail")` (BR-C3).
//!  3. Incremental sync via Gmail `historyId` / `internalDate` stored in `cursor`.
//!  4. "Mine" filter (Q3=A / BR-C1): messages sent by or received by the owner's
//!     address(es).
//!  5. Map message → `RawItem { source: Gmail, external_id: message_id, text: Some(body) }`.
//!     Bodies may contain sensitive data → they MUST pass through masking in
//!     Processing before any cloud call (US-1.4 AC2 / BR-C4 / BR-K1).
//!  6. Return the newest `historyId`/`internalDate` as the next `Cursor`.

use async_trait::async_trait;

use crate::core::error::Result;
use crate::core::traits::{Connector, CredentialStore};
use crate::core::types::{Cursor, RawItem, SourceKind};
use std::sync::Arc;

/// Gmail connector (skeleton).
pub struct GmailConnector {
    #[allow(dead_code)] // used once integration is wired up (INTEGRATION-TODO)
    credentials: Arc<dyn CredentialStore>,
}

impl GmailConnector {
    pub fn new(credentials: Arc<dyn CredentialStore>) -> Self {
        Self { credentials }
    }
}

#[async_trait]
impl Connector for GmailConnector {
    fn id(&self) -> SourceKind {
        SourceKind::Gmail
    }

    async fn sync(&self, cursor: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)> {
        // INTEGRATION-TODO(US-1.4): real Gmail API sync (see module docs).
        Ok((Vec::new(), cursor.unwrap_or_default()))
    }

    fn supports_manual(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        async fn delete(&self, _s: SourceKind) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn skeleton_is_safe_noop() {
        let conn = GmailConnector::new(Arc::new(NoCreds));
        assert_eq!(conn.id(), SourceKind::Gmail);
        let (items, _c) = conn.sync(None).await.unwrap();
        assert_eq!(items.len(), 0);
    }
}
