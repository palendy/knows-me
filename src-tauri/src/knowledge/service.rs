//! `KnowledgeService` — implements [`KnowledgeApi`].
//!
//! Orchestrates [`FactStore`], [`HistoryTracker`], and the in-memory
//! [`SearchIndex`].
//!
//! - **Lazy index rebuild (P-1)**: the derived index is (re)built from the
//!   encrypted store on first use after construction/unlock, so a fresh service
//!   handle (e.g. after restart) does not report an empty knowledge base. Callers
//!   may also force a rebuild via [`KnowledgeService::build_index`].
//! - **Serialized writes (P-6)**: an async write lock is held across the
//!   `upsert` read-modify-write so concurrent upserts cannot lose a history
//!   entry. The in-memory index (`std::sync::Mutex`) is only locked for
//!   non-awaiting critical sections.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex as AsyncMutex;

use crate::core::error::{AppError, Result};
use crate::core::traits::{EncryptedStore, KnowledgeApi};
use crate::core::types::{
    DashboardDto, Fact, FactChange, FactFilter, FactId, FactSummary, GraphDto, GraphFilter,
    QueueItem, TopicPage,
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
    /// Serializes writes (upsert / index build) so read-modify-write is atomic.
    write_lock: AsyncMutex<()>,
    /// Whether the in-memory index has been populated from the store yet.
    initialized: AtomicBool,
}

impl KnowledgeService {
    pub fn new(store: Arc<dyn EncryptedStore>) -> Self {
        Self {
            facts: FactStore::new(store.clone()),
            history: HistoryTracker::new(store.clone()),
            index: Mutex::new(SearchIndex::new()),
            write_lock: AsyncMutex::new(()),
            initialized: AtomicBool::new(false),
            store,
        }
    }

    fn rebuild_locked(&self, all: &[Fact]) {
        let mut idx = self.index.lock().expect("index mutex poisoned");
        idx.clear();
        for f in all {
            idx.upsert(f);
        }
    }

    /// Force a full (re)build of the in-memory index from the store. Call after
    /// unlock; safe to call again at any time.
    pub async fn build_index(&self) -> Result<()> {
        let _w = self.write_lock.lock().await;
        let all = self.facts.load_all().await?;
        self.rebuild_locked(&all);
        self.initialized.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Build the index once, lazily, if it has not been populated yet.
    async fn ensure_index(&self) -> Result<()> {
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }
        let _w = self.write_lock.lock().await;
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(()); // built while we waited for the lock
        }
        let all = self.facts.load_all().await?;
        self.rebuild_locked(&all);
        self.initialized.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Every stored fact, owner-scope — the raw material the sharing surface
    /// (`crate::sharing`) filters through a token. Enforcing a caller's scope is
    /// the sharing layer's job (its single authorization point), not this
    /// method's; this deliberately returns everything, `Private` included.
    pub async fn all_facts(&self) -> Result<Vec<Fact>> {
        self.facts.load_all().await
    }
}

#[async_trait]
impl KnowledgeApi for KnowledgeService {
    async fn upsert(&self, fact: Fact) -> Result<FactId> {
        // KR-1/KR-2: only confirmed facts, with complete metadata, persist.
        if !fact.metadata.confirmed {
            return Err(AppError::InvalidInput(
                "only confirmed facts may be stored".into(),
            ));
        }
        if fact.metadata.confirmed_at.is_none() {
            return Err(AppError::InvalidInput(
                "confirmed fact must have confirmed_at set".into(),
            ));
        }
        if fact.title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "fact title must not be empty".into(),
            ));
        }

        self.ensure_index().await?;

        // P-6: serialize the read-modify-write so concurrent upserts of the same
        // fact cannot lose a history entry (KR-3).
        let _w = self.write_lock.lock().await;
        let prev = self.facts.get(fact.id).await?;
        // Persist the fact first, then append history: if the write fails and is
        // retried, we neither orphan nor duplicate a history entry (P2/KR-3).
        self.facts.put(&fact).await?;
        if let Some(prev) = prev {
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
        self.ensure_index().await?;
        let idx = self.index.lock().expect("index mutex poisoned");
        Ok(idx.search(&query, &filter))
    }

    async fn graph(&self, filter: GraphFilter) -> Result<GraphDto> {
        self.ensure_index().await?;
        let idx = self.index.lock().expect("index mutex poisoned");
        Ok(idx.graph(&filter))
    }

    async fn topics(&self, filter: FactFilter) -> Result<Vec<TopicPage>> {
        self.ensure_index().await?;
        let idx = self.index.lock().expect("index mutex poisoned");
        let facts = idx.search("", &filter);
        Ok(crate::knowledge::search_index::topic_pages(
            &facts,
            &idx.confirmed_at_map(),
        ))
    }

    async fn dashboard(&self) -> Result<DashboardDto> {
        self.ensure_index().await?;
        // Count only non-expired queue items, consistent with InterviewService::list.
        let now = Utc::now();
        let mut pending_queue = 0;
        for k in self.store.list(QUEUE_NS).await? {
            if let Some(bytes) = self.store.get(QUEUE_NS, &k).await? {
                if let Ok(item) = serde_json::from_slice::<QueueItem>(&bytes) {
                    if item.expires_at.is_none_or(|e| e > now) {
                        pending_queue += 1;
                    }
                }
            }
        }
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
                topics: vec![],
                kind: Default::default(),
                visibility: Default::default(),
                category: None,
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
    async fn rejects_missing_confirmed_at() {
        let s = svc();
        let mut f = fact(FactId::new(), "t", "b", Scope::Personal);
        f.metadata.confirmed_at = None;
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
    async fn index_rebuilds_after_restart() {
        // Simulate a restart: a fresh service handle over the same store must
        // repopulate its in-memory index lazily (fix for empty-after-unlock).
        let store = Arc::new(InMemoryStore::default());
        let s1 = KnowledgeService::new(store.clone());
        s1.upsert(fact(FactId::new(), "hello world", "body", Scope::Personal))
            .await
            .unwrap();

        let s2 = KnowledgeService::new(store.clone());
        let r = s2
            .search("hello".into(), FactFilter::default())
            .await
            .unwrap();
        assert_eq!(r.len(), 1, "index should rebuild from store on first query");
        assert_eq!(s2.dashboard().await.unwrap().collected_count, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_upserts_do_not_deadlock() {
        let s = Arc::new(svc());
        let (a, b) = (FactId::new(), FactId::new());
        let s1 = s.clone();
        let s2 = s.clone();
        let h1 = tokio::spawn(async move { s1.upsert(fact(a, "a", "1", Scope::Personal)).await });
        let h2 = tokio::spawn(async move { s2.upsert(fact(b, "b", "2", Scope::Personal)).await });
        h1.await.unwrap().unwrap();
        h2.await.unwrap().unwrap();
        assert_eq!(s.dashboard().await.unwrap().collected_count, 2);
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
                    category: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(company.len(), 1);
        assert_eq!(company[0].scope, Scope::Company);
    }

    #[tokio::test]
    async fn nonmatching_punctuation_query_returns_empty_not_all() {
        let s = svc();
        s.upsert(fact(FactId::new(), "alpha", "beta", Scope::Personal))
            .await
            .unwrap();
        // non-empty query with no searchable tokens ⇒ empty, not the whole DB
        assert!(s
            .search("???".into(), FactFilter::default())
            .await
            .unwrap()
            .is_empty());
        // truly empty query ⇒ all
        assert_eq!(
            s.search(String::new(), FactFilter::default())
                .await
                .unwrap()
                .len(),
            1
        );
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

    #[tokio::test]
    async fn facts_sharing_a_topic_link_through_a_hub() {
        use crate::core::types::GraphNodeKind;
        let s = svc();
        let a = FactId::new();
        let b = FactId::new();
        let mut fa = fact(a, "A", "", Scope::Personal);
        fa.metadata.topics = vec!["deploy".into()];
        let mut fb = fact(b, "B", "", Scope::Personal);
        fb.metadata.topics = vec!["deploy".into()];
        s.upsert(fa).await.unwrap();
        s.upsert(fb).await.unwrap();

        let g = s.graph(GraphFilter::default()).await.unwrap();

        let facts = g.nodes.iter().filter(|n| n.kind == GraphNodeKind::Fact);
        let hubs: Vec<_> = g
            .nodes
            .iter()
            .filter(|n| n.kind == GraphNodeKind::Topic)
            .collect();
        assert_eq!(facts.count(), 2);
        assert_eq!(hubs.len(), 1, "the shared topic becomes one hub");
        assert_eq!(hubs[0].label, "deploy");

        // Both facts wire to the hub; there is no fact-to-fact edge.
        let hub = hubs[0].id;
        assert_eq!(g.edges.len(), 2);
        assert!(g
            .edges
            .iter()
            .all(|e| e.to == hub && (e.from == a || e.from == b)));
    }

    #[tokio::test]
    async fn topic_spelling_variants_share_one_hub() {
        use crate::core::types::GraphNodeKind;
        let s = svc();
        for (title, topic) in [("A", "ai-dlc"), ("B", "ai-dlc"), ("C", "aidlc")] {
            let mut f = fact(FactId::new(), title, "", Scope::Personal);
            f.metadata.topics = vec![topic.into()];
            s.upsert(f).await.unwrap();
        }
        let g = s.graph(GraphFilter::default()).await.unwrap();

        let hubs: Vec<_> = g
            .nodes
            .iter()
            .filter(|n| n.kind == GraphNodeKind::Topic)
            .collect();
        assert_eq!(hubs.len(), 1, "ai-dlc and aidlc are the same subject");
        assert_eq!(hubs[0].label, "ai-dlc", "the common spelling names the hub");
        assert_eq!(
            g.edges.iter().filter(|e| e.to == hubs[0].id).count(),
            3,
            "all three facts connect to the single hub"
        );
    }

    #[tokio::test]
    async fn a_lone_topic_makes_no_hub() {
        use crate::core::types::GraphNodeKind;
        let s = svc();
        let mut f = fact(FactId::new(), "A", "", Scope::Personal);
        f.metadata.topics = vec!["solo".into()];
        s.upsert(f).await.unwrap();

        let g = s.graph(GraphFilter::default()).await.unwrap();
        assert!(
            g.nodes.iter().all(|n| n.kind == GraphNodeKind::Fact),
            "a topic with a single fact links nothing, so no hub is made"
        );
        assert!(g.edges.is_empty());
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
            let filter = FactFilter { scope: Some(target.metadata.scope), category: None };
            let results = idx.search(&target.title, &filter);
            let qtoks = tokenize(&target.title);
            for r in &results {
                prop_assert_eq!(r.scope, target.metadata.scope);
                let f = facts.iter().find(|f| f.id == r.id).unwrap();
                let doc = tokenize(&format!("{} {}", f.title, f.body));
                prop_assert!(qtoks.iter().all(|t| doc.contains(t)));
            }
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
