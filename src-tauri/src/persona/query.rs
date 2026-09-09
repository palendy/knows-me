//! Read-only view services — US-5.1 dashboard, US-5.2 mini-home, US-5.3 graph.
//!
//! This type is deliberately constructed **without** an `LlmClient`. The three
//! views therefore cannot reach the network even by mistake, which is how
//! NFR-3 / U4-NFR-A1 ("read views work offline") is guaranteed structurally
//! rather than by convention.

use std::collections::HashMap;
use std::sync::Arc;

use crate::core::error::Result;
use crate::core::traits::KnowledgeApi;
use crate::core::types::{DashboardDto, FactFilter, FactId, GraphDto, GraphFilter, MiniHomeDto};

use super::selection::{rank_highlights, DEFAULT_HIGHLIGHTS};

/// Serves the three read views by delegating to U3's knowledge read side.
pub struct QueryService {
    knowledge: Arc<dyn KnowledgeApi>,
}

impl QueryService {
    pub fn new(knowledge: Arc<dyn KnowledgeApi>) -> Self {
        Self { knowledge }
    }

    /// US-5.1. The counts come straight from U3 — U4 never re-aggregates them
    /// (BR-V1: single source of truth stays in the owning unit).
    pub async fn dashboard(&self) -> Result<DashboardDto> {
        self.knowledge.dashboard().await
    }

    /// US-5.2. Highlight ranking is U4's rule (BR-V2), so it is applied here.
    ///
    /// Two calls, regardless of how many facts exist: the summary list, and the
    /// graph — which already carries every edge, so connection degree comes for
    /// free. Fetching each fact to count its links would be one round trip per
    /// fact and would blow U4-NFR-P2 on any real knowledge base.
    ///
    /// The graph now links facts to topic hubs, so a fact's degree here counts
    /// the shared-topic hubs it sits on — topic participation, a fine
    /// representativeness proxy. `rank_highlights` only looks degree up by fact
    /// id, so the synthetic hub ids also present in the map are never read.
    pub async fn minihome(&self, limit: Option<usize>) -> Result<MiniHomeDto> {
        let limit = limit.unwrap_or(DEFAULT_HIGHLIGHTS);
        let summaries = self
            .knowledge
            .search(String::new(), FactFilter::default())
            .await?;

        let graph = self.knowledge.graph(GraphFilter::default()).await?;
        let mut degree: HashMap<FactId, usize> = HashMap::new();
        for e in &graph.edges {
            *degree.entry(e.from).or_insert(0) += 1;
            *degree.entry(e.to).or_insert(0) += 1;
        }

        Ok(MiniHomeDto {
            highlights: rank_highlights(&summaries, &degree, limit),
        })
    }

    /// US-5.3. Edge normalization and layout happen in the view layer; the
    /// service returns the logical graph as U3 reports it.
    pub async fn graph(&self, filter: GraphFilter) -> Result<GraphDto> {
        self.knowledge.graph(filter).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::InMemoryKnowledge;
    use crate::persona::testgen::{fact, linked_fact};

    fn service() -> (Arc<InMemoryKnowledge>, QueryService) {
        let kn = Arc::new(InMemoryKnowledge::default());
        let svc = QueryService::new(kn.clone());
        (kn, svc)
    }

    #[tokio::test]
    async fn dashboard_passes_through_knowledge_counts() {
        let (kn, svc) = service();
        kn.upsert(fact("사실", "본문", true)).await.unwrap();
        kn.upsert(fact("사실2", "본문", true)).await.unwrap();

        let dto = svc.dashboard().await.unwrap();
        assert_eq!(dto.collected_count, 2);
    }

    #[tokio::test]
    async fn dashboard_is_empty_before_anything_is_collected() {
        let (_kn, svc) = service();
        let dto = svc.dashboard().await.unwrap();
        assert_eq!(dto.collected_count, 0);
        assert!(dto.recent_facts.is_empty());
    }

    #[tokio::test]
    async fn minihome_ranks_by_connection_degree() {
        let (kn, svc) = service();
        kn.upsert(linked_fact("허브", 4)).await.unwrap();
        kn.upsert(linked_fact("잎", 0)).await.unwrap();

        let dto = svc.minihome(Some(9)).await.unwrap();
        assert_eq!(dto.highlights[0].title, "허브");
    }

    #[tokio::test]
    async fn minihome_excludes_unconfirmed_facts() {
        let (kn, svc) = service();
        kn.upsert(fact("확정", "본문", true)).await.unwrap();
        kn.upsert(fact("미확정", "본문", false)).await.unwrap();

        let dto = svc.minihome(None).await.unwrap();
        assert_eq!(dto.highlights.len(), 1);
        assert_eq!(dto.highlights[0].title, "확정");
    }

    #[tokio::test]
    async fn graph_returns_nodes_and_edges() {
        let (kn, svc) = service();
        let mut a = fact("A", "본문", true);
        let b = fact("B", "본문", true);
        a.links = vec![b.id];
        kn.upsert(a).await.unwrap();
        kn.upsert(b).await.unwrap();

        let g = svc.graph(GraphFilter::default()).await.unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
    }
}
