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
use crate::core::traits::{Connector, CredentialStore, ProgressReporter};
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

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _progress: &dyn ProgressReporter,
    ) -> Result<(Vec<RawItem>, Cursor)> {
        // INTEGRATION-TODO(US-1.4): real Gmail API sync (see module docs).
        //
        // Until then, debug (local dev) builds return deterministic fake mail so
        // the full ingestion→processing→FactStore pipeline exercises Gmail data.
        // Stable `external_id`s make repeated collects idempotent via the dedup
        // gate. Release builds keep the safe no-op above.
        #[cfg(debug_assertions)]
        {
            let items = super::gmail_fixtures::fake_gmail_items();
            return Ok((items, cursor.unwrap_or_default()));
        }
        #[cfg(not(debug_assertions))]
        {
            Ok((Vec::new(), cursor.unwrap_or_default()))
        }
    }

    fn supports_manual(&self) -> bool {
        // Allow manual "collect" in local dev so the fake data can be pulled on
        // demand from the UI; the real connector will decide this once wired up.
        cfg!(debug_assertions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::traits::NoProgress;
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
    async fn sync_matches_build_profile() {
        let conn = GmailConnector::new(Arc::new(NoCreds));
        assert_eq!(conn.id(), SourceKind::Gmail);
        let (items, _c) = conn.sync(None, &NoProgress).await.unwrap();

        if cfg!(debug_assertions) {
            // Local dev: fake Gmail fixtures flow through the real pipeline.
            assert!(!items.is_empty());
            assert!(items.iter().all(|it| it.source == SourceKind::Gmail));
            // Stable external_ids keep repeated collects idempotent.
            let ids: std::collections::HashSet<_> =
                items.iter().map(|it| it.external_id.as_str()).collect();
            assert_eq!(ids.len(), items.len(), "external_ids must be unique");
        } else {
            // Release: safe no-op until the real Gmail integration lands.
            assert_eq!(items.len(), 0);
        }
    }
}
