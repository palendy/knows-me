//! In-memory mock implementations of the contract traits.
//!
//! Purpose: let U2 / U3 / U4 build and test today against the shared contracts
//! while the real implementations are developed in parallel. These are NOT
//! production implementations — they hold data in memory and use trivial logic.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;

use crate::core::error::{AppError, Result};
use crate::core::traits::*;
use crate::core::types::*;

fn summary_of(f: &Fact) -> FactSummary {
    FactSummary {
        id: f.id,
        title: f.title.clone(),
        scope: f.metadata.scope,
        confirmed: f.metadata.confirmed,
    }
}

fn fact_from_candidate(c: FactCandidate) -> Fact {
    Fact {
        id: FactId::new(),
        title: c.title,
        body: c.body,
        links: vec![],
        metadata: FactMetadata {
            provenance: c.provenance,
            confirmed: true,
            scope: c.suggested_scope,
            confirmed_at: Some(Utc::now()),
        },
    }
}

// ---------------------------------------------------------------------------
// U3 mocks (the common dependency of U2 and U4)
// ---------------------------------------------------------------------------

/// In-memory [`KnowledgeApi`] mock.
#[derive(Default)]
pub struct InMemoryKnowledge {
    facts: Mutex<HashMap<FactId, Fact>>,
    history: Mutex<HashMap<FactId, Vec<FactChange>>>,
}

#[async_trait]
impl KnowledgeApi for InMemoryKnowledge {
    async fn upsert(&self, fact: Fact) -> Result<FactId> {
        let id = fact.id;
        let mut facts = self.facts.lock().unwrap();
        if let Some(prev) = facts.get(&id) {
            // Preserve history instead of silently overwriting (US-3.2).
            self.history
                .lock()
                .unwrap()
                .entry(id)
                .or_default()
                .push(FactChange {
                    changed_at: Utc::now(),
                    before: Some(prev.body.clone()),
                    after: fact.body.clone(),
                    note: None,
                });
        }
        facts.insert(id, fact);
        Ok(id)
    }

    async fn get(&self, id: FactId) -> Result<Fact> {
        self.facts
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("fact {id:?}")))
    }

    async fn links(&self, id: FactId) -> Result<Vec<FactId>> {
        Ok(self
            .facts
            .lock()
            .unwrap()
            .get(&id)
            .map(|f| f.links.clone())
            .unwrap_or_default())
    }

    async fn history(&self, id: FactId) -> Result<Vec<FactChange>> {
        Ok(self
            .history
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .unwrap_or_default())
    }

    async fn search(&self, query: String, filter: FactFilter) -> Result<Vec<FactSummary>> {
        let facts = self.facts.lock().unwrap();
        Ok(facts
            .values()
            .filter(|f| {
                (query.is_empty()
                    || f.title.contains(query.as_str())
                    || f.body.contains(query.as_str()))
                    && filter.scope.is_none_or(|s| s == f.metadata.scope)
            })
            .map(summary_of)
            .collect())
    }

    async fn graph(&self, _filter: GraphFilter) -> Result<GraphDto> {
        let facts = self.facts.lock().unwrap();
        let nodes = facts
            .values()
            .map(|f| GraphNode {
                id: f.id,
                label: f.title.clone(),
            })
            .collect();
        let mut edges = Vec::new();
        for f in facts.values() {
            for l in &f.links {
                edges.push(GraphEdge {
                    from: f.id,
                    to: *l,
                });
            }
        }
        Ok(GraphDto { nodes, edges })
    }

    async fn dashboard(&self) -> Result<DashboardDto> {
        let facts = self.facts.lock().unwrap();
        let recent_facts = facts.values().take(5).map(summary_of).collect();
        Ok(DashboardDto {
            collected_count: facts.len(),
            pending_queue: 0,
            recent_facts,
        })
    }
}

/// In-memory [`InterviewApi`] mock.
#[derive(Default)]
pub struct InMemoryInterview {
    items: Mutex<Vec<QueueItem>>,
}

#[async_trait]
impl InterviewApi for InMemoryInterview {
    async fn enqueue(&self, item: QueueItem) -> Result<QueueItemId> {
        let id = item.id;
        self.items.lock().unwrap().push(item);
        Ok(id)
    }

    async fn list(&self, sort: QueueSort) -> Result<Vec<QueueItem>> {
        let mut v = self.items.lock().unwrap().clone();
        match sort {
            QueueSort::PriorityDesc => v.sort_by_key(|a| std::cmp::Reverse(a.priority)),
            QueueSort::NewestFirst => v.sort_by_key(|a| std::cmp::Reverse(a.created_at)),
        }
        Ok(v)
    }

    async fn answer(&self, id: QueueItemId, answer: AnswerInput) -> Result<AnswerResult> {
        let mut items = self.items.lock().unwrap();
        let pos = items
            .iter()
            .position(|i| i.id == id)
            .ok_or_else(|| AppError::NotFound(format!("queue item {id:?}")))?;
        let item = items.remove(pos);
        let confirmed_fact = match (item.kind, answer) {
            (QueueItemKind::Confirm { candidate }, AnswerInput::Choice(_))
            | (QueueItemKind::Confirm { candidate }, AnswerInput::Text(_)) => {
                Some(fact_from_candidate(candidate))
            }
            _ => None,
        };
        Ok(AnswerResult {
            confirmed_fact,
            follow_ups: vec![],
        })
    }

    async fn expire(&self) -> Result<usize> {
        let now = Utc::now();
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        items.retain(|i| i.expires_at.is_none_or(|e| e > now));
        Ok(before - items.len())
    }
}

// ---------------------------------------------------------------------------
// U1 mocks
// ---------------------------------------------------------------------------

/// Pass-through [`Masker`] (no masking). Real masker lands in U1 full impl.
pub struct NoopMasker;

impl Masker for NoopMasker {
    fn mask(&self, text: &str) -> (MaskedText, UnmaskMap) {
        (
            MaskedText {
                text: text.to_string(),
            },
            UnmaskMap::new(),
        )
    }
    fn unmask(&self, masked: &MaskedText, _map: &UnmaskMap) -> String {
        masked.text.clone()
    }
}

/// Canned [`LlmClient`] returning deterministic strings (no network).
pub struct CannedLlm;

#[async_trait]
impl LlmClient for CannedLlm {
    async fn summarize(&self, input: &MaskedText) -> Result<String> {
        Ok(format!("[summary] {}", input.text))
    }
    async fn classify(&self, _input: &MaskedText) -> Result<Vec<String>> {
        Ok(vec!["general".to_string()])
    }
    async fn vision_extract(&self, _image_png: &[u8]) -> Result<MaskedText> {
        Ok(MaskedText {
            text: "[image description]".to_string(),
        })
    }
    async fn chat(&self, _system: &str, input: &MaskedText) -> Result<String> {
        Ok(format!("[reply] {}", input.text))
    }
}

/// In-memory [`EncryptedStore`] mock (stores plaintext bytes in a map).
#[derive(Default)]
pub struct InMemoryStore {
    map: Mutex<HashMap<(String, String), Vec<u8>>>,
}

#[async_trait]
impl EncryptedStore for InMemoryStore {
    async fn put(&self, ns: &str, key: &str, bytes: &[u8]) -> Result<()> {
        self.map
            .lock()
            .unwrap()
            .insert((ns.to_string(), key.to_string()), bytes.to_vec());
        Ok(())
    }
    async fn get(&self, ns: &str, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .map
            .lock()
            .unwrap()
            .get(&(ns.to_string(), key.to_string()))
            .cloned())
    }
    async fn list(&self, ns: &str) -> Result<Vec<String>> {
        Ok(self
            .map
            .lock()
            .unwrap()
            .keys()
            .filter(|(n, _)| n == ns)
            .map(|(_, k)| k.clone())
            .collect())
    }
    async fn delete(&self, ns: &str, key: &str) -> Result<()> {
        self.map
            .lock()
            .unwrap()
            .remove(&(ns.to_string(), key.to_string()));
        Ok(())
    }
}

/// [`KeyManager`] mock backed by a boolean flag (no real crypto).
#[derive(Default)]
pub struct MockKeyManager {
    unlocked: AtomicBool,
}

impl KeyManager for MockKeyManager {
    fn setup(&self, _password: &str) -> Result<()> {
        Ok(())
    }
    fn unlock(&self, _password: &str) -> Result<KeyHandle> {
        self.unlocked.store(true, Ordering::SeqCst);
        Ok(KeyHandle::new_for_test())
    }
    fn lock(&self) {
        self.unlocked.store(false, Ordering::SeqCst);
    }
    fn is_unlocked(&self) -> bool {
        self.unlocked.load(Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// Smoke tests (also seed for later PBT — see NFR-8)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_masker_roundtrip() {
        let m = NoopMasker;
        let (masked, map) = m.mask("hello world");
        assert_eq!(m.unmask(&masked, &map), "hello world");
    }

    #[tokio::test]
    async fn knowledge_upsert_get_dashboard() {
        let kn = InMemoryKnowledge::default();
        let f = Fact {
            id: FactId::new(),
            title: "run cmd".into(),
            body: "./run.sh".into(),
            links: vec![],
            metadata: FactMetadata {
                provenance: Provenance {
                    source: SourceKind::Session,
                    collected_at: Utc::now(),
                },
                confirmed: true,
                scope: Scope::Personal,
                confirmed_at: Some(Utc::now()),
            },
        };
        let id = kn.upsert(f).await.unwrap();
        assert_eq!(kn.get(id).await.unwrap().title, "run cmd");
        assert_eq!(kn.dashboard().await.unwrap().collected_count, 1);
    }

    #[tokio::test]
    async fn knowledge_upsert_twice_keeps_history() {
        let kn = InMemoryKnowledge::default();
        let id = FactId::new();
        let base = Fact {
            id,
            title: "editor".into(),
            body: "vim".into(),
            links: vec![],
            metadata: FactMetadata {
                provenance: Provenance {
                    source: SourceKind::Session,
                    collected_at: Utc::now(),
                },
                confirmed: true,
                scope: Scope::Personal,
                confirmed_at: Some(Utc::now()),
            },
        };
        kn.upsert(base.clone()).await.unwrap();
        let mut updated = base;
        updated.body = "neovim".into();
        kn.upsert(updated).await.unwrap();
        let hist = kn.history(id).await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].before.as_deref(), Some("vim"));
        assert_eq!(hist[0].after, "neovim");
    }

    #[tokio::test]
    async fn interview_confirm_produces_fact() {
        let iv = InMemoryInterview::default();
        let item = QueueItem {
            id: QueueItemId::new(),
            kind: QueueItemKind::Confirm {
                candidate: FactCandidate {
                    title: "deploy".into(),
                    body: "make deploy".into(),
                    provenance: Provenance {
                        source: SourceKind::Session,
                        collected_at: Utc::now(),
                    },
                    suggested_scope: Scope::Company,
                },
            },
            priority: 5,
            created_at: Utc::now(),
            expires_at: None,
        };
        let id = iv.enqueue(item).await.unwrap();
        let res = iv.answer(id, AnswerInput::Choice("yes".into())).await.unwrap();
        assert!(res.confirmed_fact.is_some());
        assert_eq!(res.confirmed_fact.unwrap().title, "deploy");
    }

    #[test]
    fn key_manager_unlock_lock() {
        let km = MockKeyManager::default();
        assert!(!km.is_unlocked());
        let _h = km.unlock("pw").unwrap();
        assert!(km.is_unlocked());
        km.lock();
        assert!(!km.is_unlocked());
    }
}
