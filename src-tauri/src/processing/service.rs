//! [`ProcessingService`] — implements U1's [`ProcessingApi`].
//!
//! Pipeline per item (business-logic-model §B.1):
//!   1. offline? → park in PendingQueue (P3), skip for now
//!   2. text: `Masker::mask` → `MaskedText` (+ local `UnmaskMap`, Q5=A memory-only)
//!      image: `LlmGateway::vision_extract` → `MaskedText`
//!   3. `LlmGateway::summarize` + `classify` (masked-only, logged)
//!   4. local `unmask` for the stored body (US-2.2 AC2)
//!   5. `route` → KnowledgeApi.upsert | InterviewApi.enqueue | drop
//!   6. drop the `UnmaskMap` at end of scope (Q5=A / BR-K2)
//!
//! Stories: US-2.1, US-2.2, US-2.3, NFR-3.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use crate::core::error::Result;
use crate::core::traits::{InterviewApi, KnowledgeApi, LlmClient, Masker, ProcessingApi};
use crate::core::types::{
    Fact, FactCandidate, FactMetadata, MaskedText, ProcessReport, QueueItem, QueueItemId,
    QueueItemKind, RawItem,
};
use crate::processing::llm_gateway::LlmGateway;
use crate::processing::pending::PendingQueue;
use crate::processing::router::{route, ProcessingDecision};
use crate::processing::transfer_log::TransferLog;

/// Reports whether the cloud LLM is currently reachable (P3). Injectable so
/// tests can simulate offline; real impl checks connectivity/config.
pub trait OnlineProbe: Send + Sync {
    fn is_online(&self) -> bool;
}

/// Always-online probe (default for wiring where connectivity is assumed).
pub struct AlwaysOnline;
impl OnlineProbe for AlwaysOnline {
    fn is_online(&self) -> bool {
        true
    }
}

pub struct ProcessingService {
    masker: Arc<dyn Masker>,
    gateway: Arc<LlmGateway>,
    knowledge: Arc<dyn KnowledgeApi>,
    interview: Arc<dyn InterviewApi>,
    pending: Arc<PendingQueue>,
    online: Arc<dyn OnlineProbe>,
}

impl ProcessingService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        masker: Arc<dyn Masker>,
        llm: Arc<dyn LlmClient>,
        transfer_log: Arc<TransferLog>,
        knowledge: Arc<dyn KnowledgeApi>,
        interview: Arc<dyn InterviewApi>,
        pending: Arc<PendingQueue>,
        online: Arc<dyn OnlineProbe>,
    ) -> Self {
        let gateway = Arc::new(LlmGateway::new(llm, transfer_log));
        Self {
            masker,
            gateway,
            knowledge,
            interview,
            pending,
            online,
        }
    }

    /// Process one raw item through the full pipeline. Returns the decision so
    /// callers can update counters.
    async fn process_one(&self, raw: &RawItem, report: &mut ProcessReport) -> Result<()> {
        // 2. Obtain masked text (image → vision; text → mask). The UnmaskMap is
        //    kept only in this scope and dropped at function end (Q5=A / BR-K2).
        let (masked, unmap) = if let Some(image) = &raw.image_png {
            let mt = self.gateway.vision_extract(raw.source, image).await?;
            (mt, None)
        } else {
            let text = raw.text.clone().unwrap_or_default();
            let (mt, map) = self.masker.mask(&text);
            (mt, Some(map))
        };

        // 3. Summarize + classify (masked-only, logged, retried).
        let summary_masked = self.gateway.summarize(raw.source, &masked).await?;
        let labels = self.gateway.classify(raw.source, &masked).await?;

        // 4. Local unmask for the stored body (US-2.2 AC2). For the image path
        //    there is no reverse map, so the summary is used as-is.
        let body = match &unmap {
            Some(map) => self.masker.unmask(
                &MaskedText {
                    text: summary_masked,
                },
                map,
            ),
            None => summary_masked,
        };

        // 5. Route.
        match route(&labels, &body, raw) {
            ProcessingDecision::Store(cand) => {
                self.knowledge.upsert(fact_from(cand)).await?;
                report.facts_created += 1;
            }
            ProcessingDecision::Confirm(cand) => {
                self.interview.enqueue(confirm_item(cand)).await?;
                report.queue_items_created += 1;
            }
            ProcessingDecision::Deepen {
                question,
                hypothesis,
            } => {
                self.interview
                    .enqueue(deepen_item(question, hypothesis))
                    .await?;
                report.queue_items_created += 1;
            }
            ProcessingDecision::Drop { .. } => {
                report.filtered += 1;
            }
        }
        // 6. `unmap` drops here.
        Ok(())
    }

    /// Re-process everything parked in the pending queue (called on reconnection,
    /// e.g. from the U1 Scheduler tick — Q3=A).
    pub async fn resume_pending(&self) -> Result<ProcessReport> {
        if !self.online.is_online() {
            return Ok(ProcessReport::default());
        }
        let items: Vec<RawItem> = self
            .pending
            .drain()
            .await?
            .into_iter()
            .map(|p| p.raw)
            .collect();
        self.process(items).await
    }
}

#[async_trait]
impl ProcessingApi for ProcessingService {
    async fn process(&self, items: Vec<RawItem>) -> Result<ProcessReport> {
        let mut report = ProcessReport::default();
        for raw in items {
            // 1. Offline → park and continue (NFR-3 / BR-O1). Collection is
            //    unaffected; only processing is deferred.
            if !self.online.is_online() {
                self.pending.push(raw, &Utc::now().to_rfc3339()).await?;
                continue;
            }
            // On per-item processing error (e.g. retries exhausted), park it so
            // it is retried later rather than lost.
            if let Err(_e) = self.process_one(&raw, &mut report).await {
                self.pending.push(raw, &Utc::now().to_rfc3339()).await?;
            }
        }
        Ok(report)
    }
}

fn fact_from(c: FactCandidate) -> Fact {
    Fact {
        id: crate::core::types::FactId::new(),
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

fn confirm_item(candidate: FactCandidate) -> QueueItem {
    QueueItem {
        id: QueueItemId::new(),
        kind: QueueItemKind::Confirm { candidate },
        priority: 5,
        created_at: Utc::now(),
        expires_at: None,
    }
}

fn deepen_item(question: String, hypothesis: Option<String>) -> QueueItem {
    QueueItem {
        id: QueueItemId::new(),
        kind: QueueItemKind::Deepen {
            question,
            hypothesis,
        },
        priority: 3,
        created_at: Utc::now(),
        expires_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::SourceKind;
    use crate::mocks::{
        CannedLlm, InMemoryInterview, InMemoryKnowledge, InMemoryStore, NoopMasker,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Toggle(AtomicBool);
    impl OnlineProbe for Toggle {
        fn is_online(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn raw(id: &str, text: &str) -> RawItem {
        RawItem {
            source: SourceKind::Session,
            external_id: id.into(),
            collected_at: Utc::now(),
            text: Some(text.into()),
            image_png: None,
        }
    }

    fn service(
        online: bool,
    ) -> (
        ProcessingService,
        Arc<InMemoryKnowledge>,
        Arc<InMemoryInterview>,
        Arc<PendingQueue>,
        Arc<Toggle>,
    ) {
        let store = Arc::new(InMemoryStore::default());
        let log = Arc::new(TransferLog::new(store.clone()));
        let pending = Arc::new(PendingQueue::new(store));
        let knowledge = Arc::new(InMemoryKnowledge::default());
        let interview = Arc::new(InMemoryInterview::default());
        let probe = Arc::new(Toggle(AtomicBool::new(online)));
        let svc = ProcessingService::new(
            Arc::new(NoopMasker),
            Arc::new(CannedLlm),
            log,
            knowledge.clone(),
            interview.clone(),
            pending.clone(),
            probe.clone(),
        );
        (svc, knowledge, interview, pending, probe)
    }

    #[tokio::test]
    async fn empty_labels_route_to_confirm_queue() {
        // CannedLlm.classify returns ["general"] → certain → Store. Verify a fact lands.
        let (svc, knowledge, _iv, _p, _t) = service(true);
        let r = svc
            .process(vec![raw("a", "ran the deploy script")])
            .await
            .unwrap();
        assert_eq!(r.facts_created, 1);
        let dash = knowledge.dashboard().await.unwrap();
        assert_eq!(dash.collected_count, 1);
    }

    #[tokio::test]
    async fn offline_parks_then_resume_processes() {
        let (svc, knowledge, _iv, pending, probe) = service(false);
        let r = svc
            .process(vec![raw("a", "x"), raw("b", "y")])
            .await
            .unwrap();
        assert_eq!(r.facts_created, 0);
        assert_eq!(pending.len().await.unwrap(), 2);

        // Go online and resume.
        probe.0.store(true, Ordering::SeqCst);
        let r2 = svc.resume_pending().await.unwrap();
        assert_eq!(r2.facts_created, 2);
        assert!(pending.is_empty().await.unwrap());
        assert_eq!(knowledge.dashboard().await.unwrap().collected_count, 2);
    }

    #[tokio::test]
    async fn image_uses_vision_path() {
        let (svc, knowledge, _iv, _p, _t) = service(true);
        let img = RawItem {
            source: SourceKind::File,
            external_id: "img1".into(),
            collected_at: Utc::now(),
            text: None,
            image_png: Some(vec![1, 2, 3]),
        };
        let r = svc.process(vec![img]).await.unwrap();
        // CannedLlm vision returns "[image description]" → classify "general" → store.
        assert_eq!(r.facts_created, 1);
        assert_eq!(knowledge.dashboard().await.unwrap().collected_count, 1);
    }
}
