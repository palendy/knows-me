//! Idempotency gate + incremental cursors, layered on U1's [`EncryptedStore`].
//!
//! Pattern P4 (Idempotency Gate) / BR-I1..I3 / NFR-5.
//! - `ingestion.seen`   : `"{source}:{external_id}" -> collected_at` (dedup key)
//! - `ingestion.cursor` : `"{source}" -> Cursor` (next incremental start point)
//!
//! The dedup key is `(SourceKind, external_id)` only (design decision Q1=A).

use std::sync::Arc;

use crate::core::error::{AppError, Result};
use crate::core::traits::EncryptedStore;
use crate::core::types::{Cursor, SourceKind};

const NS_SEEN: &str = "ingestion.seen";
const NS_CURSOR: &str = "ingestion.cursor";

/// Stable string form of a `SourceKind` used in storage keys.
fn source_tag(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Session => "session",
        SourceKind::Notion => "notion",
        SourceKind::Gmail => "gmail",
        SourceKind::File => "file",
    }
}

fn seen_key(source: SourceKind, external_id: &str) -> String {
    format!("{}:{}", source_tag(source), external_id)
}

/// Thin idempotency + cursor layer over [`EncryptedStore`].
pub struct IngestionCursorStore {
    store: Arc<dyn EncryptedStore>,
}

impl IngestionCursorStore {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self { store }
    }

    /// Load the incremental cursor for a source (`None` on first run).
    pub async fn load_cursor(&self, source: SourceKind) -> Result<Option<Cursor>> {
        match self.store.get(NS_CURSOR, source_tag(source)).await? {
            Some(bytes) => {
                let s = String::from_utf8(bytes)
                    .map_err(|e| AppError::Serde(format!("cursor utf8: {e}")))?;
                Ok(Some(Cursor(s)))
            }
            None => Ok(None),
        }
    }

    /// Persist the cursor. Only called after a successful `sync` (BR-I3).
    pub async fn save_cursor(&self, source: SourceKind, cursor: &Cursor) -> Result<()> {
        self.store
            .put(NS_CURSOR, source_tag(source), cursor.0.as_bytes())
            .await
    }

    /// Whether `(source, external_id)` was already collected (dedup, Q1=A).
    pub async fn is_seen(&self, source: SourceKind, external_id: &str) -> Result<bool> {
        Ok(self
            .store
            .get(NS_SEEN, &seen_key(source, external_id))
            .await?
            .is_some())
    }

    /// Mark `(source, external_id)` collected. Idempotent: re-marking is a no-op
    /// with respect to the seen-set (monotonic — BR-I2, PBT-03 seen monotonicity).
    pub async fn mark_seen(
        &self,
        source: SourceKind,
        external_id: &str,
        at_rfc3339: &str,
    ) -> Result<()> {
        self.store
            .put(
                NS_SEEN,
                &seen_key(source, external_id),
                at_rfc3339.as_bytes(),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::InMemoryStore;

    fn store() -> IngestionCursorStore {
        IngestionCursorStore::new(Arc::new(InMemoryStore::default()))
    }

    #[tokio::test]
    async fn cursor_roundtrip() {
        let cs = store();
        assert!(cs.load_cursor(SourceKind::Session).await.unwrap().is_none());
        cs.save_cursor(SourceKind::Session, &Cursor("c1".into()))
            .await
            .unwrap();
        assert_eq!(
            cs.load_cursor(SourceKind::Session)
                .await
                .unwrap()
                .unwrap()
                .0,
            "c1"
        );
    }

    #[tokio::test]
    async fn seen_is_monotonic() {
        let cs = store();
        assert!(!cs.is_seen(SourceKind::File, "a").await.unwrap());
        cs.mark_seen(SourceKind::File, "a", "t0").await.unwrap();
        assert!(cs.is_seen(SourceKind::File, "a").await.unwrap());
        // Re-marking keeps it seen (monotonic).
        cs.mark_seen(SourceKind::File, "a", "t1").await.unwrap();
        assert!(cs.is_seen(SourceKind::File, "a").await.unwrap());
    }

    #[tokio::test]
    async fn seen_key_is_source_scoped() {
        let cs = store();
        cs.mark_seen(SourceKind::File, "x", "t").await.unwrap();
        // Same external_id, different source → not seen.
        assert!(!cs.is_seen(SourceKind::Notion, "x").await.unwrap());
    }
}
