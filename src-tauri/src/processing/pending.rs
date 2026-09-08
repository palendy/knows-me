//! Offline-degradation queue (Pattern P3, NFR-3 / BR-O).
//!
//! When the LLM is unreachable (or retries are exhausted), the raw item is
//! parked here instead of being dropped. On reconnection the ProcessingService
//! drains and re-processes it. Persisted via `EncryptedStore` ns `processing.pending`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, Result};
use crate::core::traits::EncryptedStore;
use crate::core::types::RawItem;

const NS_PENDING: &str = "processing.pending";

/// A raw item awaiting processing (offline).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingProcessingItem {
    pub raw: RawItem,
    pub queued_at_rfc3339: String,
    pub attempts: u32,
}

/// Persistent pending queue.
pub struct PendingQueue {
    store: Arc<dyn EncryptedStore>,
}

impl PendingQueue {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self { store }
    }

    /// Park an item. Key = external_id (dedup: re-queuing the same item bumps
    /// attempts rather than duplicating).
    pub async fn push(&self, raw: RawItem, at_rfc3339: &str) -> Result<()> {
        let key = format!("{}:{}", source_tag(&raw), raw.external_id);
        let attempts = match self.store.get(NS_PENDING, &key).await? {
            Some(bytes) => {
                let prev: PendingProcessingItem = serde_json::from_slice(&bytes)
                    .map_err(|e| AppError::Serde(format!("pending: {e}")))?;
                prev.attempts + 1
            }
            None => 0,
        };
        let item = PendingProcessingItem {
            raw,
            queued_at_rfc3339: at_rfc3339.to_string(),
            attempts,
        };
        let bytes =
            serde_json::to_vec(&item).map_err(|e| AppError::Serde(format!("pending: {e}")))?;
        self.store.put(NS_PENDING, &key, &bytes).await
    }

    /// Remove and return all pending items (for re-processing).
    pub async fn drain(&self) -> Result<Vec<PendingProcessingItem>> {
        let mut out = Vec::new();
        for key in self.store.list(NS_PENDING).await? {
            if let Some(bytes) = self.store.get(NS_PENDING, &key).await? {
                let item: PendingProcessingItem = serde_json::from_slice(&bytes)
                    .map_err(|e| AppError::Serde(format!("pending: {e}")))?;
                out.push(item);
            }
            self.store.delete(NS_PENDING, &key).await?;
        }
        Ok(out)
    }

    pub async fn len(&self) -> Result<usize> {
        Ok(self.store.list(NS_PENDING).await?.len())
    }

    pub async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }
}

fn source_tag(raw: &RawItem) -> &'static str {
    use crate::core::types::SourceKind::*;
    match raw.source {
        Session => "session",
        Notion => "notion",
        Gmail => "gmail",
        File => "file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::InMemoryStore;
    use crate::core::types::SourceKind;
    use chrono::Utc;

    fn raw(id: &str) -> RawItem {
        RawItem {
            source: SourceKind::Session,
            external_id: id.into(),
            collected_at: Utc::now(),
            text: Some("t".into()),
            image_png: None,
        }
    }

    #[tokio::test]
    async fn push_drain_roundtrip() {
        let q = PendingQueue::new(Arc::new(InMemoryStore::default()));
        assert!(q.is_empty().await.unwrap());
        q.push(raw("a"), "t0").await.unwrap();
        q.push(raw("b"), "t0").await.unwrap();
        assert_eq!(q.len().await.unwrap(), 2);
        let drained = q.drain().await.unwrap();
        assert_eq!(drained.len(), 2);
        assert!(q.is_empty().await.unwrap());
    }

    #[tokio::test]
    async fn requeue_bumps_attempts_not_count() {
        let q = PendingQueue::new(Arc::new(InMemoryStore::default()));
        q.push(raw("a"), "t0").await.unwrap();
        q.push(raw("a"), "t1").await.unwrap();
        assert_eq!(q.len().await.unwrap(), 1);
        let drained = q.drain().await.unwrap();
        assert_eq!(drained[0].attempts, 1);
    }
}
