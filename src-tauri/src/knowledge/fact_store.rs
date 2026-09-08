//! `FactStore` — encrypted fact documents.
//!
//! Facts are the canonical, at-rest form (US-3.1) stored as JSON bytes through
//! U1's [`EncryptedStore`] under the `facts` namespace, keyed by `FactId`.

use std::sync::Arc;

use uuid::Uuid;

use crate::core::error::{AppError, Result};
use crate::core::traits::EncryptedStore;
use crate::core::types::{Fact, FactId};

const NS: &str = "facts";

fn key(id: FactId) -> String {
    id.0.to_string()
}

fn de(bytes: &[u8]) -> Result<Fact> {
    serde_json::from_slice(bytes).map_err(|e| AppError::Serde(e.to_string()))
}

/// Encrypted, JSON-backed store of confirmed fact documents.
pub struct FactStore {
    store: Arc<dyn EncryptedStore>,
}

impl FactStore {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self { store }
    }

    /// Write (create or overwrite) a fact document.
    pub async fn put(&self, fact: &Fact) -> Result<()> {
        let bytes = serde_json::to_vec(fact).map_err(|e| AppError::Serde(e.to_string()))?;
        self.store.put(NS, &key(fact.id), &bytes).await
    }

    /// Read a fact by id, if present.
    pub async fn get(&self, id: FactId) -> Result<Option<Fact>> {
        match self.store.get(NS, &key(id)).await? {
            Some(b) => Ok(Some(de(&b)?)),
            None => Ok(None),
        }
    }

    /// Load every stored fact (used to (re)build the in-memory index on unlock).
    pub async fn load_all(&self) -> Result<Vec<Fact>> {
        let keys = self.store.list(NS).await?;
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            if let Some(b) = self.store.get(NS, &k).await? {
                out.push(de(&b)?);
            }
        }
        Ok(out)
    }

    /// Delete a fact document.
    pub async fn delete(&self, id: FactId) -> Result<()> {
        self.store.delete(NS, &key(id)).await
    }
}

/// Parse a stored key back into a `FactId` (tolerant: skips malformed keys).
pub fn parse_id(key: &str) -> Option<FactId> {
    Uuid::parse_str(key).ok().map(FactId)
}
