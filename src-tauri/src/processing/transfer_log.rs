//! Append-only transfer transparency log (US-2.3, Pattern P7).
//!
//! Every outbound LLM call records what (masked) content went out. Stored via
//! U1 `EncryptedStore` under ns `transfer.log`. `masked_preview` is derived ONLY
//! from masked text — original identifiers must never be logged (BR-T2/BR-K5).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, Result};
use crate::core::traits::EncryptedStore;
use crate::core::types::SourceKind;

const NS_TRANSFER: &str = "transfer.log";

/// Which outbound operation was performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferOp {
    Summarize,
    Classify,
    VisionExtract,
}

/// One outbound-transfer record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferLogEntry {
    pub at_rfc3339: String,
    pub source: SourceKind,
    pub operation: TransferOp,
    /// Masked-only preview of what was sent (never the original).
    pub masked_preview: String,
    pub target: String,
}

/// Append-only transfer log over `EncryptedStore`.
pub struct TransferLog {
    store: Arc<dyn EncryptedStore>,
}

impl TransferLog {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self { store }
    }

    /// Append an entry. Key is timestamp + a monotonic-ish index to avoid
    /// collisions within the same instant (index derived from current count).
    pub async fn append(&self, entry: TransferLogEntry) -> Result<()> {
        let existing = self.store.list(NS_TRANSFER).await?.len();
        let key = format!("{}-{}", entry.at_rfc3339, existing);
        let bytes = serde_json::to_vec(&entry)
            .map_err(|e| AppError::Serde(format!("transfer entry: {e}")))?;
        self.store.put(NS_TRANSFER, &key, &bytes).await
    }

    /// Read all entries (for the transparency view; consumed by U4 later).
    pub async fn all(&self) -> Result<Vec<TransferLogEntry>> {
        let mut out = Vec::new();
        for key in self.store.list(NS_TRANSFER).await? {
            if let Some(bytes) = self.store.get(NS_TRANSFER, &key).await? {
                let e: TransferLogEntry = serde_json::from_slice(&bytes)
                    .map_err(|e| AppError::Serde(format!("transfer entry: {e}")))?;
                out.push(e);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::InMemoryStore;

    #[tokio::test]
    async fn append_and_read() {
        let log = TransferLog::new(Arc::new(InMemoryStore::default()));
        log.append(TransferLogEntry {
            at_rfc3339: "2026-09-08T00:00:00Z".into(),
            source: SourceKind::Session,
            operation: TransferOp::Summarize,
            masked_preview: "[NAME] ran deploy".into(),
            target: "cloud-llm".into(),
        })
        .await
        .unwrap();
        let all = log.all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].operation, TransferOp::Summarize);
    }
}
