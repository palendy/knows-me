//! `HistoryTracker` — append-only change history (US-3.2).
//!
//! On every fact-body change, a [`FactChange`] snapshot is appended (never
//! overwritten). History is stored per fact under the `fact_history` namespace
//! and read lazily (only when `history(id)` is called).

use std::sync::Arc;

use crate::core::error::{AppError, Result};
use crate::core::traits::EncryptedStore;
use crate::core::types::{FactChange, FactId};

const NS: &str = "fact_history";

fn key(id: FactId) -> String {
    id.0.to_string()
}

/// Append-only per-fact change log.
pub struct HistoryTracker {
    store: Arc<dyn EncryptedStore>,
}

impl HistoryTracker {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self { store }
    }

    /// Read the chronological (ascending) change log for a fact; empty if none.
    pub async fn history(&self, id: FactId) -> Result<Vec<FactChange>> {
        match self.store.get(NS, &key(id)).await? {
            Some(b) => serde_json::from_slice(&b).map_err(|e| AppError::Serde(e.to_string())),
            None => Ok(Vec::new()),
        }
    }

    /// Append one change entry, preserving prior entries (KR-3/KR-4).
    pub async fn append(&self, id: FactId, change: FactChange) -> Result<()> {
        let mut log = self.history(id).await?;
        log.push(change);
        let bytes = serde_json::to_vec(&log).map_err(|e| AppError::Serde(e.to_string()))?;
        self.store.put(NS, &key(id), &bytes).await
    }
}
