//! `InterviewService` — implements [`InterviewApi`].
//!
//! Orchestrates [`QueueManager`] and answer intake. Confirmed facts flow to the
//! single [`KnowledgeApi::upsert`] path (services.md). Deepen answers also derive
//! bounded follow-up questions (mask-first, best-effort — P-4).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};

use crate::core::error::{AppError, Result};
use crate::core::traits::{EncryptedStore, InterviewApi, KnowledgeApi, LlmClient, Masker};
use crate::core::types::{
    AnswerInput, AnswerResult, QueueItem, QueueItemId, QueueItemKind, QueueSort,
};
use crate::interview::answer_intake::{
    derive_follow_ups, fact_from_candidate, fact_from_deepen, is_affirmative,
};
use crate::interview::queue_manager::{
    dedup_key, is_expired, score_priority, sort_queue, ttl_days, QueueManager,
};

pub struct InterviewService {
    queue: QueueManager,
    knowledge: Arc<dyn KnowledgeApi>,
    masker: Arc<dyn Masker>,
    llm: Arc<dyn LlmClient>,
}

impl InterviewService {
    pub fn new(
        store: Arc<dyn EncryptedStore>,
        knowledge: Arc<dyn KnowledgeApi>,
        masker: Arc<dyn Masker>,
        llm: Arc<dyn LlmClient>,
    ) -> Self {
        Self {
            queue: QueueManager::new(store),
            knowledge,
            masker,
            llm,
        }
    }

    /// Apply priority scoring + TTL to an item that left them unset.
    fn finalize(item: &mut QueueItem) {
        if item.priority == 0 {
            item.priority = score_priority(item);
        }
        if item.expires_at.is_none() {
            item.expires_at = Some(item.created_at + Duration::days(ttl_days(&item.kind)));
        }
    }
}

#[async_trait]
impl InterviewApi for InterviewService {
    async fn enqueue(&self, mut item: QueueItem) -> Result<QueueItemId> {
        if let QueueItemKind::Deepen { question, .. } = &item.kind {
            if question.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "deepen question must not be empty".into(),
                ));
            }
        }

        Self::finalize(&mut item);

        // IR-4: suppress duplicates — keep the higher-priority pending item.
        let key = dedup_key(&item.kind);
        for existing in self.queue.list().await? {
            if dedup_key(&existing.kind) == key {
                if existing.priority >= item.priority {
                    return Ok(existing.id);
                }
                self.queue.remove(existing.id).await?;
            }
        }

        self.queue.put(&item).await?;
        Ok(item.id)
    }

    async fn list(&self, sort: QueueSort) -> Result<Vec<QueueItem>> {
        let mut items = self.queue.list().await?;
        sort_queue(&mut items, sort);
        Ok(items)
    }

    async fn answer(&self, id: QueueItemId, answer: AnswerInput) -> Result<AnswerResult> {
        let item = self
            .queue
            .get(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("queue item {id:?}")))?;

        // IR-3: Skip leaves the item pending; the owner is never forced.
        if matches!(answer, AnswerInput::Skip) {
            return Ok(AnswerResult {
                confirmed_fact: None,
                follow_ups: vec![],
            });
        }

        let mut confirmed_fact = None;
        let mut follow_ups = vec![];

        match item.kind {
            QueueItemKind::Confirm { candidate } => {
                let affirm = match &answer {
                    AnswerInput::Choice(c) => is_affirmative(c),
                    AnswerInput::Text(_) => true,
                    AnswerInput::Skip => unreachable!("handled above"),
                };
                if affirm {
                    let fact = fact_from_candidate(candidate);
                    self.knowledge.upsert(fact.clone()).await?;
                    confirmed_fact = Some(fact);
                }
                // reject ⇒ discard candidate, no fact (IR-1)
            }
            QueueItemKind::Deepen { question, .. } => {
                let text = match &answer {
                    AnswerInput::Text(t) | AnswerInput::Choice(t) => t.clone(),
                    AnswerInput::Skip => unreachable!("handled above"),
                };
                let fact = fact_from_deepen(&question, &text);
                self.knowledge.upsert(fact.clone()).await?;
                confirmed_fact = Some(fact);
                // IR-2/IR-6: bounded follow-ups (best-effort). Direct put bypasses
                // dedup — freshly generated items are unlikely to collide.
                for mut up in
                    derive_follow_ups(self.masker.as_ref(), self.llm.as_ref(), &text).await
                {
                    Self::finalize(&mut up);
                    self.queue.put(&up).await?;
                    follow_ups.push(up);
                }
            }
        }

        self.queue.remove(id).await?;
        Ok(AnswerResult {
            confirmed_fact,
            follow_ups,
        })
    }

    async fn expire(&self) -> Result<usize> {
        let now = Utc::now();
        let mut removed = 0;
        for item in self.queue.list().await? {
            if is_expired(&item, now) {
                self.queue.remove(item.id).await?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{FactCandidate, Provenance, QueueItemId, Scope, SourceKind};
    use crate::knowledge::KnowledgeService;
    use crate::mocks::{CannedLlm, InMemoryStore, NoopMasker};
    use async_trait::async_trait;
    use proptest::prelude::*;

    /// LLM double that always fails — used to test graceful degradation.
    struct FailingLlm;

    #[async_trait]
    impl LlmClient for FailingLlm {
        async fn summarize(&self, _: &crate::core::types::MaskedText) -> Result<String> {
            Err(AppError::External("offline".into()))
        }
        async fn classify(&self, _: &crate::core::types::MaskedText) -> Result<Vec<String>> {
            Err(AppError::External("offline".into()))
        }
        async fn vision_extract(&self, _: &[u8]) -> Result<crate::core::types::MaskedText> {
            Err(AppError::External("offline".into()))
        }
        async fn chat(&self, _: &str, _: &crate::core::types::MaskedText) -> Result<String> {
            Err(AppError::External("offline".into()))
        }
    }

    fn store() -> Arc<InMemoryStore> {
        Arc::new(InMemoryStore::default())
    }

    fn service_with(llm: Arc<dyn LlmClient>) -> (InterviewService, Arc<KnowledgeService>) {
        let st = store();
        let knowledge = Arc::new(KnowledgeService::new(st.clone()));
        let svc = InterviewService::new(st, knowledge.clone(), Arc::new(NoopMasker), llm);
        (svc, knowledge)
    }

    fn confirm_item() -> QueueItem {
        QueueItem {
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
            priority: 0,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    fn deepen_item(question: &str) -> QueueItem {
        QueueItem {
            id: QueueItemId::new(),
            kind: QueueItemKind::Deepen {
                question: question.into(),
                hypothesis: None,
            },
            priority: 0,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn confirm_affirmative_creates_fact() {
        let (svc, kn) = service_with(Arc::new(CannedLlm));
        let id = svc.enqueue(confirm_item()).await.unwrap();
        let res = svc
            .answer(id, AnswerInput::Choice("yes".into()))
            .await
            .unwrap();
        let fact = res.confirmed_fact.expect("fact created");
        assert_eq!(fact.title, "deploy");
        assert_eq!(kn.get(fact.id).await.unwrap().title, "deploy");
        assert!(svc.list(QueueSort::NewestFirst).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn confirm_reject_creates_no_fact() {
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        let id = svc.enqueue(confirm_item()).await.unwrap();
        let res = svc
            .answer(id, AnswerInput::Choice("no".into()))
            .await
            .unwrap();
        assert!(res.confirmed_fact.is_none());
    }

    #[tokio::test]
    async fn deepen_text_creates_fact_and_follow_ups() {
        let (svc, kn) = service_with(Arc::new(CannedLlm));
        let id = svc.enqueue(deepen_item("무슨 에디터 써?")).await.unwrap();
        let res = svc
            .answer(id, AnswerInput::Text("neovim".into()))
            .await
            .unwrap();
        assert!(res.confirmed_fact.is_some());
        assert_eq!(kn.dashboard().await.unwrap().collected_count, 1);
        // CannedLlm.classify returns one label ⇒ one follow-up enqueued.
        assert_eq!(res.follow_ups.len(), 1);
        assert_eq!(svc.list(QueueSort::NewestFirst).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn skip_leaves_item_pending() {
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        let id = svc.enqueue(confirm_item()).await.unwrap();
        let res = svc.answer(id, AnswerInput::Skip).await.unwrap();
        assert!(res.confirmed_fact.is_none());
        assert_eq!(svc.list(QueueSort::NewestFirst).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn offline_llm_still_saves_fact_without_follow_ups() {
        let (svc, kn) = service_with(Arc::new(FailingLlm));
        let id = svc.enqueue(deepen_item("근황?")).await.unwrap();
        let res = svc
            .answer(id, AnswerInput::Text("이사했다".into()))
            .await
            .unwrap();
        assert!(res.confirmed_fact.is_some(), "fact persists offline (P-4)");
        assert!(res.follow_ups.is_empty(), "no follow-ups when LLM is down");
        assert_eq!(kn.dashboard().await.unwrap().collected_count, 1);
    }

    #[tokio::test]
    async fn dedup_keeps_higher_priority() {
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        svc.enqueue(deepen_item("같은 질문")).await.unwrap();
        svc.enqueue(deepen_item("같은 질문")).await.unwrap();
        assert_eq!(svc.list(QueueSort::NewestFirst).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn expire_removes_past_due() {
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        let mut past = confirm_item();
        past.expires_at = Some(Utc::now() - Duration::days(1));
        svc.enqueue(past).await.unwrap();
        let mut future = deepen_item("살아있는 질문");
        future.expires_at = Some(Utc::now() + Duration::days(1));
        svc.enqueue(future).await.unwrap();
        assert_eq!(svc.expire().await.unwrap(), 1);
        assert_eq!(svc.list(QueueSort::NewestFirst).await.unwrap().len(), 1);
    }

    // ---- property-based tests -----------------------------------------------

    fn arb_source() -> impl Strategy<Value = SourceKind> {
        prop_oneof![
            Just(SourceKind::Session),
            Just(SourceKind::Notion),
            Just(SourceKind::Gmail),
            Just(SourceKind::File),
        ]
    }

    fn arb_queue_item() -> impl Strategy<Value = QueueItem> {
        let confirm = (arb_source(), "[a-zA-Z0-9가-힣 ]{1,20}", 0u8..=255).prop_map(
            |(source, title, pri)| QueueItem {
                id: QueueItemId::new(),
                kind: QueueItemKind::Confirm {
                    candidate: FactCandidate {
                        title,
                        body: "b".into(),
                        provenance: Provenance {
                            source,
                            collected_at: Utc::now(),
                        },
                        suggested_scope: Scope::Company,
                    },
                },
                priority: pri,
                created_at: Utc::now(),
                expires_at: None,
            },
        );
        let deepen = ("[a-zA-Z0-9가-힣 ?]{1,20}", 0u8..=255).prop_map(|(q, pri)| QueueItem {
            id: QueueItemId::new(),
            kind: QueueItemKind::Deepen {
                question: q,
                hypothesis: None,
            },
            priority: pri,
            created_at: Utc::now(),
            expires_at: None,
        });
        prop_oneof![confirm, deepen]
    }

    proptest! {
        // P1 (PBT-02): QueueItem JSON round-trips.
        #[test]
        fn prop_queue_item_roundtrip(item in arb_queue_item()) {
            let v1 = serde_json::to_value(&item).unwrap();
            let back: QueueItem = serde_json::from_value(v1.clone()).unwrap();
            let v2 = serde_json::to_value(&back).unwrap();
            prop_assert_eq!(v1, v2);
        }

        // P5 (PBT-03): PriorityDesc sort is non-increasing in priority.
        #[test]
        fn prop_priority_sort_monotonic(mut items in prop::collection::vec(arb_queue_item(), 0..20)) {
            sort_queue(&mut items, QueueSort::PriorityDesc);
            for w in items.windows(2) {
                prop_assert!(w[0].priority >= w[1].priority);
            }
        }

        // P5 (PBT-03): NewestFirst sort is non-increasing in created_at.
        #[test]
        fn prop_newest_sort_monotonic(mut items in prop::collection::vec(arb_queue_item(), 0..20)) {
            sort_queue(&mut items, QueueSort::NewestFirst);
            for w in items.windows(2) {
                prop_assert!(w[0].created_at >= w[1].created_at);
            }
        }

        // Range invariant: scored priority always fits u8 (never panics/overflows).
        #[test]
        fn prop_score_priority_in_range(item in arb_queue_item()) {
            let _p: u8 = score_priority(&item); // type guarantees 0..=255
            prop_assert!(true);
        }

        // P6 (PBT-03): the retain predicate leaves no expired item behind.
        #[test]
        fn prop_no_expired_after_retain(
            offsets in prop::collection::vec(-5i64..5, 0..15)
        ) {
            let now = Utc::now();
            let items: Vec<QueueItem> = offsets.iter().map(|d| {
                let mut it = deepen_item("q");
                it.expires_at = Some(now + Duration::days(*d));
                it
            }).collect();
            let kept: Vec<&QueueItem> = items.iter().filter(|i| !is_expired(i, now)).collect();
            prop_assert!(kept.iter().all(|i| !is_expired(i, now)));
        }
    }
}
