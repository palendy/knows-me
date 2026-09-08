//! `KnowledgeService` — implements [`KnowledgeApi`].
//!
//! Orchestrates [`FactStore`], [`HistoryTracker`], and the in-memory
//! [`SearchIndex`]. Write order in `upsert` (P-5): append history (if the body
//! changed) → write fact → update index. The index is derived/rebuildable, so a
//! crash between steps self-heals on the next `build_index`. Index locks are
//! never held across `.await` (P-6, serialized single-writer).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;

use crate::core::error::{AppError, Result};
use crate::core::traits::{EncryptedStore, KnowledgeApi};
use crate::core::types::{
    DashboardDto, Fact, FactChange, FactFilter, FactId, FactSummary, GraphDto, GraphFilter,
};
use crate::knowledge::fact_store::FactStore;
use crate::knowledge::history::HistoryTracker;
use crate::knowledge::search_index::SearchIndex;

/// Namespace whose key-count reflects the pending interview queue (read-only,
/// avoids a reverse dependency on `InterviewService`).
const QUEUE_NS: &str = "queue";

pub struct KnowledgeService {
    store: Arc<dyn EncryptedStore>,
    facts: FactStore,
    history: HistoryTracker,
    index: Mutex<SearchIndex>,
}

impl KnowledgeService {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self {
            facts: FactStore::new(store.clone()),
            history: HistoryTracker::new(store.clone()),
            index: Mutex::new(SearchIndex::new()),
            store,
        }
    }

    /// (Re)build the in-memory index from the encrypted store. Call after unlock.
    pub async fn build_index(&self) -> Result<()> {
        let all = self.facts.load_all().await?;
        let mut idx = self.index.lock().expect("index mutex poisoned");
        idx.clear();
        for f in &all {
            idx.upsert(f);
        }
        Ok(())
    }
}

#[async_trait]
impl KnowledgeApi for KnowledgeService {
    async fn upsert(&self, fact: Fact) -> Result<FactId> {
        // KR-1: only confirmed facts persist. KR-2: title required.
        if !fact.metadata.confirmed {
            return Err(AppError::InvalidInput(
                "only confirmed facts may be stored".into(),
            ));
        }
        if fact.title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "fact title must not be empty".into(),
            ));
        }

        // KR-3: preserve history when the body changes (no-op write ⇒ no entry).
        if let Some(prev) = self.facts.get(fact.id).await? {
            if prev.body != fact.body {
                self.history
                    .append(
                        fact.id,
                        FactChange {
                            changed_at: Utc::now(),
                            before: Some(prev.body),
                            after: fact.body.clone(),
                            note: None,
                        },
                    )
                    .await?;
            }
        }

        self.facts.put(&fact).await?;
        {
            let mut idx = self.index.lock().expect("index mutex poisoned");
            idx.upsert(&fact);
        }
        Ok(fact.id)
    }

    async fn get(&self, id: FactId) -> Result<Fact> {
        self.facts
            .get(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("fact {id:?}")))
    }

    async fn links(&self, id: FactId) -> Result<Vec<FactId>> {
        Ok(self.get(id).await?.links)
    }

    async fn history(&self, id: FactId) -> Result<Vec<FactChange>> {
        self.history.history(id).await
    }

    async fn search(&self, query: String, filter: FactFilter) -> Result<Vec<FactSummary>> {
        let idx = self.index.lock().expect("index mutex poisoned");
        Ok(idx.search(&query, &filter))
    }

    async fn graph(&self, filter: GraphFilter) -> Result<GraphDto> {
        let idx = self.index.lock().expect("index mutex poisoned");
        Ok(idx.graph(&filter))
    }

    async fn dashboard(&self) -> Result<DashboardDto> {
        let pending_queue = self.store.list(QUEUE_NS).await?.len();
        let idx = self.index.lock().expect("index mutex poisoned");
        Ok(DashboardDto {
            collected_count: idx.len(),
            pending_queue,
            recent_facts: idx.recent(5),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{FactMetadata, Provenance, Scope, SourceKind};
    use crate::knowledge::search_index::{tokenize, SearchIndex};
    use crate::mocks::InMemoryStore;
    use proptest::prelude::*;

    fn fact(id: FactId, title: &str, body: &str, scope: Scope) -> Fact {
        Fact {
            id,
            title: title.into(),
            body: body.into(),
            links: vec![],
            metadata: FactMetadata {
                provenance: Provenance {
                    source: SourceKind::Session,
                    collected_at: Utc::now(),
                },
                confirmed: true,
                scope,
                confirmed_at: Some(Utc::now()),
            },
        }
    }

    fn svc() -> KnowledgeService {
        KnowledgeService::new(Arc::new(InMemoryStore::default()))
    }

    // ---- example-based tests ------------------------------------------------

    #[tokio::test]
    async fn rejects_unconfirmed_fact() {
        let s = svc();
        let mut f = fact(FactId::new(), "t", "b", Scope::Personal);
        f.metadata.confirmed = false;
        assert!(matches!(s.upsert(f).await, Err(AppError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn rejects_empty_title() {
        let s = svc();
        let f = fact(FactId::new(), "   ", "b", Scope::Personal);
        assert!(matches!(s.upsert(f).await, Err(AppError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn upsert_get_and_dashboard() {
        let s = svc();
        let id = FactId::new();
        s.upsert(fact(id, "run cmd", "./run.sh", Scope::Personal))
            .await
            .unwrap();
        assert_eq!(s.get(id).await.unwrap().title, "run cmd");
        let d = s.dashboard().await.unwrap();
        assert_eq!(d.collected_count, 1);
        assert_eq!(d.pending_queue, 0);
    }

    #[tokio::test]
    async fn body_change_keeps_history_no_op_does_not() {
        let s = svc();
        let id = FactId::new();
        let base = fact(id, "editor", "vim", Scope::Personal);
        s.upsert(base.clone()).await.unwrap();
        // no-op re-write ⇒ no history (KR-3)
        s.upsert(base.clone()).await.unwrap();
        assert_eq!(s.history(id).await.unwrap().len(), 0);
        // body change ⇒ exactly one history entry
        let mut updated = base;
        updated.body = "neovim".into();
        s.upsert(updated).await.unwrap();
        let h = s.history(id).await.unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].before.as_deref(), Some("vim"));
        assert_eq!(h[0].after, "neovim");
    }

    #[tokio::test]
    async fn search_keyword_and_scope_filter_including_korean() {
        let s = svc();
        s.upsert(fact(
            FactId::new(),
            "러스트 프로그래밍",
            "소유권",
            Scope::Personal,
        ))
        .await
        .unwrap();
        s.upsert(fact(
            FactId::new(),
            "deploy guide",
            "make deploy",
            Scope::Company,
        ))
        .await
        .unwrap();
        // Korean substring via bigram index
        let r = s
            .search("러스트".into(), FactFilter::default())
            .await
            .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "러스트 프로그래밍");
        // scope filter
        let company = s
            .search(
                String::new(),
                FactFilter {
                    scope: Some(Scope::Company),
                },
            )
            .await
            .unwrap();
        assert_eq!(company.len(), 1);
        assert_eq!(company[0].scope, Scope::Company);
    }

    #[tokio::test]
    async fn graph_edges_only_within_nodes() {
        let s = svc();
        let a = FactId::new();
        let b = FactId::new();
        let missing = FactId::new();
        let mut fa = fact(a, "A", "", Scope::Personal);
        fa.links = vec![b, missing]; // one valid, one dangling
        s.upsert(fa).await.unwrap();
        s.upsert(fact(b, "B", "", Scope::Personal)).await.unwrap();
        let g = s.graph(GraphFilter::default()).await.unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].from, a);
        assert_eq!(g.edges[0].to, b);
    }

    // ---- property-based tests (proptest) ------------------------------------

    fn arb_scope() -> impl Strategy<Value = Scope> {
        prop_oneof![
            Just(Scope::Company),
            Just(Scope::Personal),
            Just(Scope::Unknown)
        ]
    }

    fn arb_fact() -> impl Strategy<Value = Fact> {
        ("[a-zA-Z0-9가-힣]{1,20}", "\\PC{0,60}", arb_scope())
            .prop_map(|(title, body, scope)| fact(FactId::new(), &title, &body, scope))
    }

    proptest! {
        // P1 (PBT-02): Fact JSON round-trips (value equality, order-independent).
        #[test]
        fn prop_fact_json_roundtrip(f in arb_fact()) {
            let v1 = serde_json::to_value(&f).unwrap();
            let back: Fact = serde_json::from_value(v1.clone()).unwrap();
            let v2 = serde_json::to_value(&back).unwrap();
            prop_assert_eq!(v1, v2);
        }

        // P4 (PBT-03): search results satisfy the scope filter, and for a
        // non-empty query every result actually contains all query tokens.
        #[test]
        fn prop_search_results_match(facts in prop::collection::vec(arb_fact(), 1..12)) {
            let mut idx = SearchIndex::new();
            for f in &facts { idx.upsert(f); }
            let target = &facts[0];
            let filter = FactFilter { scope: Some(target.metadata.scope) };
            let results = idx.search(&target.title, &filter);
            let qtoks = tokenize(&target.title);
            for r in &results {
                prop_assert_eq!(r.scope, target.metadata.scope);
                let f = facts.iter().find(|f| f.id == r.id).unwrap();
                let doc = tokenize(&format!("{} {}", f.title, f.body));
                prop_assert!(qtoks.iter().all(|t| doc.contains(t)));
            }
            // the target itself must be found (its own tokens match & scope matches)
            prop_assert!(results.iter().any(|r| r.id == target.id));
        }

        // P7 (PBT-03): graph never emits an edge with an endpoint outside nodes.
        #[test]
        fn prop_graph_edges_within_nodes(facts in prop::collection::vec(arb_fact(), 1..12)) {
            let mut idx = SearchIndex::new();
            for f in &facts { idx.upsert(f); }
            let g = idx.graph(&GraphFilter::default());
            let node_ids: std::collections::HashSet<_> = g.nodes.iter().map(|n| n.id).collect();
            for e in &g.edges {
                prop_assert!(node_ids.contains(&e.from));
                prop_assert!(node_ids.contains(&e.to));
            }
        }
    }
}
