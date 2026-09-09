//! `QueueManager` — encrypted queue storage + pure policy helpers.
//!
//! Storage lives under the `queue` namespace. Priority scoring, TTL, dedup keys,
//! sorting, and expiry are pure functions so they can be property-tested without
//! any I/O.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::core::error::{AppError, Result};
use crate::core::traits::EncryptedStore;
use crate::core::types::{QueueItem, QueueItemId, QueueItemKind, QueueSort, SourceKind};

const NS: &str = "queue";

fn key(id: QueueItemId) -> String {
    id.0.to_string()
}

/// Encrypted, JSON-backed store of pending interview items.
pub struct QueueManager {
    store: Arc<dyn EncryptedStore>,
}

impl QueueManager {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self { store }
    }

    pub async fn put(&self, item: &QueueItem) -> Result<()> {
        let bytes = serde_json::to_vec(item).map_err(|e| AppError::Serde(e.to_string()))?;
        self.store.put(NS, &key(item.id), &bytes).await
    }

    pub async fn get(&self, id: QueueItemId) -> Result<Option<QueueItem>> {
        match self.store.get(NS, &key(id)).await? {
            Some(b) => Ok(Some(
                serde_json::from_slice(&b).map_err(|e| AppError::Serde(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    pub async fn list(&self) -> Result<Vec<QueueItem>> {
        let keys = self.store.list(NS).await?;
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            if let Some(b) = self.store.get(NS, &k).await? {
                out.push(serde_json::from_slice(&b).map_err(|e| AppError::Serde(e.to_string()))?);
            }
        }
        Ok(out)
    }

    pub async fn remove(&self, id: QueueItemId) -> Result<()> {
        self.store.delete(NS, &key(id)).await
    }
}

// --- pure policy helpers (IR-3/IR-4/IR-5) -----------------------------------

/// Weighted priority (0–255): base + source-trust + confirm-need. Recency/age is
/// handled by the `NewestFirst` sort and TTL expiry rather than baked in here.
pub fn score_priority(item: &QueueItem) -> u8 {
    let base: i32 = 100;
    let (source_w, need_w) = match &item.kind {
        QueueItemKind::Confirm { candidate } => {
            let s = match candidate.provenance.source {
                SourceKind::Session => 40,
                SourceKind::Notion | SourceKind::Gmail => 25,
                SourceKind::File => 10,
            };
            (s, 20)
        }
        QueueItemKind::Deepen { .. } => (15, 10),
    };
    (base + source_w + need_w).clamp(0, 255) as u8
}

/// Time-to-live in days: Confirm items 14 days, Deepen items 30 days (IR-5).
pub fn ttl_days(kind: &QueueItemKind) -> i64 {
    match kind {
        QueueItemKind::Confirm { .. } => 14,
        QueueItemKind::Deepen { .. } => 30,
    }
}

/// The kind namespace and the text an item is deduplicated on.
///
/// Namespaced so a Confirm candidate and a Deepen question that happen to share
/// a string do not collide.
fn dedup_text(kind: &QueueItemKind) -> (&'static str, &str) {
    match kind {
        QueueItemKind::Confirm { candidate } => ("confirm", candidate.title.as_str()),
        QueueItemKind::Deepen { question, .. } => ("deepen", question.as_str()),
    }
}

/// Whether two queue items are the same item asked twice (IR-4).
///
/// This used to be an equality test on `format!("{ns}:{}", title.to_lowercase())`,
/// which never fired in practice: what arrives is the same idea rewritten, not
/// the same string. See [`crate::core::text`] for the measurement and the
/// threshold.
pub fn is_near_duplicate(a: &QueueItemKind, b: &QueueItemKind) -> bool {
    let (ns_a, ta) = dedup_text(a);
    let (ns_b, tb) = dedup_text(b);
    ns_a == ns_b && crate::core::text::is_near_duplicate(ta, tb)
}

/// Whether an item is past its expiry at `now`.
pub fn is_expired(item: &QueueItem, now: DateTime<Utc>) -> bool {
    item.expires_at.is_some_and(|e| e <= now)
}

/// Sort items for display (IR-4): priority desc (tie-break newest), or newest.
pub fn sort_queue(items: &mut [QueueItem], sort: QueueSort) {
    match sort {
        QueueSort::PriorityDesc => items.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(b.created_at.cmp(&a.created_at))
        }),
        QueueSort::NewestFirst => items.sort_by_key(|i| std::cmp::Reverse(i.created_at)),
    }
}
