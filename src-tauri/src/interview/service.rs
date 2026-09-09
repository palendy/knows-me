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
    is_expired, is_near_duplicate, score_priority, sort_queue, ttl_days, QueueManager,
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

        // IR-4: suppress duplicates (per-kind key). Ignore expired items (they
        // are about to be swept and must not block a fresh item). Decide against
        // the highest-priority live duplicate BEFORE mutating, so we never drop both.
        let now = Utc::now();
        let dups: Vec<QueueItem> = self
            .queue
            .list()
            .await?
            .into_iter()
            .filter(|e| !is_expired(e, now) && is_near_duplicate(&e.kind, &item.kind))
            .collect();
        if let Some(best) = dups.iter().max_by_key(|e| e.priority) {
            if best.priority >= item.priority {
                return Ok(best.id); // incoming loses; leave existing untouched
            }
        }
        // incoming wins: remove all existing duplicates, then store it.
        for e in &dups {
            self.queue.remove(e.id).await?;
        }
        self.queue.put(&item).await?;
        Ok(item.id)
    }

    async fn list(&self, sort: QueueSort) -> Result<Vec<QueueItem>> {
        let now = Utc::now();
        // Never surface expired items even before the sweep runs (US-4.3).
        let mut items: Vec<QueueItem> = self
            .queue
            .list()
            .await?
            .into_iter()
            .filter(|i| !is_expired(i, now))
            .collect();
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

        // Guard empty/whitespace answers BEFORE mutating: an accidental blank
        // answer must not silently create knowledge or discard an item.
        match &answer {
            AnswerInput::Choice(c) if c.trim().is_empty() => {
                return Err(AppError::InvalidInput(
                    "answer choice must not be empty".into(),
                ));
            }
            AnswerInput::Text(t) if t.trim().is_empty() => {
                return Err(AppError::InvalidInput(
                    "answer text must not be empty".into(),
                ));
            }
            _ => {}
        }

        let mut confirmed_fact = None;
        let mut follow_ups = vec![];

        match item.kind {
            QueueItemKind::Confirm { candidate } => {
                // Choice: affirm/reject as-is. Text: treat as a correction to the
                // candidate body and confirm the corrected fact (frontend-components §3).
                let (affirm, correction) = match &answer {
                    AnswerInput::Choice(c) => (is_affirmative(c), None),
                    AnswerInput::Text(t) => (true, Some(t.clone())),
                    AnswerInput::Skip => unreachable!("handled above"),
                };
                if affirm {
                    let mut fact = fact_from_candidate(candidate);
                    if let Some(body) = correction {
                        fact.body = body;
                    }
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
                // IR-2/IR-6: bounded follow-ups (best-effort). Route through
                // enqueue so dedup/priority/TTL apply consistently (IR-4).
                for mut up in
                    derive_follow_ups(self.masker.as_ref(), self.llm.as_ref(), &text).await
                {
                    // Finalize so the item returned in follow_ups carries its scored
                    // priority/TTL; enqueue re-finalizes its own copy (no-op here).
                    Self::finalize(&mut up);
                    let stored = self.enqueue(up.clone()).await?;
                    if stored == up.id {
                        follow_ups.push(up); // only report items actually added
                    }
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

        // 1) TTL expiry (IR-5).
        let mut alive = Vec::new();
        for item in self.queue.list().await? {
            if is_expired(&item, now) {
                self.queue.remove(item.id).await?;
                removed += 1;
            } else {
                alive.push(item);
            }
        }

        // 2) Dedup suppression (IR-4): keep the highest-priority item per group.
        //    Near-duplication is not an equivalence relation — A can match B and
        //    B match C without A matching C — so there is no key to bucket on.
        //    Comparing each item against the ones already kept is the honest
        //    implementation; the pending queue is tens of items, not thousands.
        let mut kept: Vec<usize> = Vec::new();
        let mut to_remove = Vec::new();
        for (i, item) in alive.iter().enumerate() {
            match kept
                .iter()
                .position(|&k| is_near_duplicate(&alive[k].kind, &item.kind))
            {
                Some(pos) if item.priority > alive[kept[pos]].priority => {
                    to_remove.push(alive[kept[pos]].id);
                    kept[pos] = i;
                }
                Some(_) => to_remove.push(item.id),
                None => kept.push(i),
            }
        }
        for id in to_remove {
            self.queue.remove(id).await?;
            removed += 1;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{FactCandidate, MaskedText, Provenance, Scope, SourceKind, UnmaskMap};
    use crate::knowledge::KnowledgeService;
    use crate::mocks::{CannedLlm, InMemoryStore, NoopMasker};
    use async_trait::async_trait;
    use proptest::prelude::*;

    /// LLM double that always fails — used to test graceful degradation.
    struct FailingLlm;

    #[async_trait]
    impl LlmClient for FailingLlm {
        async fn summarize(&self, _: &MaskedText) -> Result<String> {
            Err(AppError::External("offline".into()))
        }
        async fn classify(&self, _: &MaskedText) -> Result<Vec<String>> {
            Err(AppError::External("offline".into()))
        }
        async fn vision_extract(&self, _: &[u8]) -> Result<MaskedText> {
            Err(AppError::External("offline".into()))
        }
        async fn chat(&self, _: &str, _: &MaskedText) -> Result<String> {
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

    fn confirm_item_titled(title: &str) -> QueueItem {
        QueueItem {
            id: QueueItemId::new(),
            kind: QueueItemKind::Confirm {
                candidate: FactCandidate {
                    title: title.into(),
                    body: "make deploy".into(),
                    provenance: Provenance {
                        source: SourceKind::Session,
                        collected_at: Utc::now(),
                    },
                    suggested_scope: Scope::Company,
                    topics: vec![],
                    kind: Default::default(),
                    visibility: Default::default(),
                },
            },
            priority: 0,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    fn confirm_item() -> QueueItem {
        confirm_item_titled("deploy")
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
    async fn empty_answer_is_rejected_and_item_kept() {
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        let cid = svc.enqueue(confirm_item()).await.unwrap();
        assert!(matches!(
            svc.answer(cid, AnswerInput::Choice("  ".into())).await,
            Err(AppError::InvalidInput(_))
        ));
        let did = svc.enqueue(deepen_item("질문?")).await.unwrap();
        assert!(matches!(
            svc.answer(did, AnswerInput::Text("".into())).await,
            Err(AppError::InvalidInput(_))
        ));
        // both items must still be pending
        assert_eq!(svc.list(QueueSort::NewestFirst).await.unwrap().len(), 2);
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
    async fn confirm_and_deepen_same_text_do_not_collide() {
        // A Confirm titled "배포" and a Deepen asking "배포" are distinct (per-kind key).
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        svc.enqueue(confirm_item_titled("배포")).await.unwrap();
        svc.enqueue(deepen_item("배포")).await.unwrap();
        assert_eq!(svc.list(QueueSort::NewestFirst).await.unwrap().len(), 2);
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

    // ---- code-review fix regression tests -----------------------------------

    /// Masker that replaces "alice" with a placeholder, to verify unmasking.
    struct BracketMasker;
    impl Masker for BracketMasker {
        fn mask(&self, text: &str) -> (MaskedText, UnmaskMap) {
            let mut map = UnmaskMap::new();
            let masked = if text.contains("alice") {
                map.insert("[P1]".into(), "alice".into());
                text.replace("alice", "[P1]")
            } else {
                text.to_string()
            };
            (MaskedText { text: masked }, map)
        }
        fn unmask(&self, masked: &MaskedText, map: &UnmaskMap) -> String {
            let mut s = masked.text.clone();
            for (ph, orig) in map.entries() {
                s = s.replace(ph.as_str(), orig.as_str());
            }
            s
        }
    }

    /// LLM double whose classify echoes the (masked) input as a label.
    struct EchoClassifyLlm;
    #[async_trait]
    impl LlmClient for EchoClassifyLlm {
        async fn summarize(&self, i: &MaskedText) -> Result<String> {
            Ok(i.text.clone())
        }
        async fn classify(&self, i: &MaskedText) -> Result<Vec<String>> {
            Ok(vec![i.text.clone()])
        }
        async fn vision_extract(&self, _: &[u8]) -> Result<MaskedText> {
            Ok(MaskedText {
                text: String::new(),
            })
        }
        async fn chat(&self, _: &str, i: &MaskedText) -> Result<String> {
            Ok(i.text.clone())
        }
    }

    #[tokio::test]
    async fn confirm_text_is_treated_as_correction() {
        let (svc, kn) = service_with(Arc::new(CannedLlm));
        let id = svc.enqueue(confirm_item()).await.unwrap(); // candidate body "make deploy"
        let res = svc
            .answer(id, AnswerInput::Text("corrected body".into()))
            .await
            .unwrap();
        let fact = res.confirmed_fact.expect("fact created");
        assert_eq!(
            fact.body, "corrected body",
            "correction text must be applied"
        );
        assert_eq!(kn.get(fact.id).await.unwrap().body, "corrected body");
    }

    #[tokio::test]
    async fn enqueue_suppresses_a_paraphrase_not_only_an_exact_repeat() {
        // The failure this guards, measured on real output: 30 titles from six
        // sessions carried 13 distinct ideas and *zero* byte-identical pairs.
        // Exact-string suppression let every one of them through.
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        svc.enqueue(confirm_item_titled(
            "기술 작업은 AI에 맡기고 본인은 학습에만 집중하려 함",
        ))
        .await
        .unwrap();
        svc.enqueue(confirm_item_titled(
            "기술 작업은 AI에 맡기고 본인은 공부에만 집중하려 함",
        ))
        .await
        .unwrap();

        let listed = svc.list(QueueSort::NewestFirst).await.unwrap();
        assert_eq!(
            listed.len(),
            1,
            "paraphrase must not open a second question"
        );
    }

    #[tokio::test]
    async fn enqueue_still_keeps_two_genuinely_different_questions() {
        // The opposite failure is worse: a merged question is a fact the user
        // never gets asked about.
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        svc.enqueue(confirm_item_titled(
            "PDF 한글화에서 Mermaid 다이어그램 글자가 안 보이는 문제",
        ))
        .await
        .unwrap();
        svc.enqueue(confirm_item_titled(
            "세션 로그를 쌓아도 쓸 만한 정보가 안 나오는 문제",
        ))
        .await
        .unwrap();

        assert_eq!(svc.list(QueueSort::NewestFirst).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn expire_collapses_paraphrases_already_in_the_queue() {
        // Items enqueued before the near-duplicate rule existed are still in
        // the user's queue; the sweep is what clears them out.
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        for t in [
            "세션 데이터를 모아도 쓸 만한 정보가 안 나오는 문제",
            "세션 로그를 쌓아도 쓸 만한 정보가 안 나오는 문제",
        ] {
            svc.queue
                .put(&{
                    let mut i = confirm_item_titled(t);
                    InterviewService::finalize(&mut i);
                    i
                })
                .await
                .unwrap();
        }
        assert_eq!(
            svc.queue.list().await.unwrap().len(),
            2,
            "seeded past the gate"
        );

        let removed = svc.expire().await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(svc.list(QueueSort::NewestFirst).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn enqueue_ignores_expired_duplicate() {
        let (svc, _kn) = service_with(Arc::new(CannedLlm));
        let mut stale = confirm_item(); // title "deploy"
        stale.expires_at = Some(Utc::now() - Duration::days(1)); // expired, unswept
        svc.enqueue(stale).await.unwrap();
        let fresh_id = svc.enqueue(confirm_item()).await.unwrap(); // same key, fresh
        let listed = svc.list(QueueSort::NewestFirst).await.unwrap();
        assert_eq!(
            listed.len(),
            1,
            "fresh item must not be blocked by an expired dup"
        );
        assert_eq!(listed[0].id, fresh_id);
    }

    #[tokio::test]
    async fn dashboard_pending_excludes_expired() {
        let (svc, kn) = service_with(Arc::new(CannedLlm));
        let mut past = confirm_item_titled("old");
        past.expires_at = Some(Utc::now() - Duration::days(1));
        svc.enqueue(past).await.unwrap();
        svc.enqueue(confirm_item_titled("new")).await.unwrap();
        assert_eq!(kn.dashboard().await.unwrap().pending_queue, 1);
    }

    #[tokio::test]
    async fn follow_up_questions_are_unmasked() {
        let ups = derive_follow_ups(&BracketMasker, &EchoClassifyLlm, "alice moved").await;
        assert_eq!(ups.len(), 1);
        match &ups[0].kind {
            QueueItemKind::Deepen { question, .. } => {
                assert!(
                    question.contains("alice"),
                    "placeholder not unmasked: {question}"
                );
                assert!(!question.contains("[P1]"));
            }
            _ => panic!("expected deepen follow-up"),
        }
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
                        topics: vec![],
                        kind: Default::default(),
                        visibility: Default::default(),
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

        // Range invariant: scored priority stays within the expected band.
        #[test]
        fn prop_score_priority_in_range(item in arb_queue_item()) {
            let p = score_priority(&item);
            prop_assert!((100..=255).contains(&p));
        }

        // P6 (PBT-03): after expire(), the real queue contains no expired item.
        #[test]
        fn prop_expire_removes_all_past_due(offsets in prop::collection::vec(-3i64..3i64, 0..10)) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let (svc, _kn) = service_with(Arc::new(CannedLlm));
                let now = Utc::now();
                for (i, d) in offsets.iter().enumerate() {
                    let mut it = deepen_item(&format!("q{i}"));
                    it.expires_at = Some(now + Duration::days(*d));
                    svc.enqueue(it).await.unwrap();
                }
                svc.expire().await.unwrap();
                // Inspect the RAW stored queue (list() would filter expired itself).
                let check = Utc::now();
                for it in svc.queue.list().await.unwrap() {
                    prop_assert!(!is_expired(&it, check));
                }
                Ok(())
            })?;
        }
    }
}
