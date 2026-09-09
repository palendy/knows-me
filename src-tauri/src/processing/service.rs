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
use crate::ingestion::service::RawItemSink;
use crate::llm::prompts;
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

/// What the pipeline already knows, read once per item.
///
/// `hint` is the model-facing form (mask-safe, capped, de-duplicated); `facts`
/// is every confirmed title, used locally to decide whether a Confirm candidate
/// is worth asking about at all.
struct KnownTitles {
    hint: Option<String>,
    facts: Vec<String>,
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

        // 3. Summarize once, then classify each entry the summary contains.
        //    A session used to get exactly one slot, so a transcript that used
        //    one term two hundred times could leave nothing about it behind —
        //    the slot went to whatever ranked first.
        //    The summarizer sees each session on its own, so left alone it writes
        //    a fresh sentence for an idea it has already recorded five times —
        //    measured: 30 titles from six sessions, 13 distinct ideas, zero
        //    byte-identical pairs. Showing it what is already on file turns
        //    "write a title" into "reuse one if it fits", which is what makes
        //    duplicate suppression downstream able to fire at all. Same move
        //    that fixed label fragmentation for the classifier below; the
        //    summarizer is what invents the *title*, and it was getting nothing.
        let known = self.known_titles().await;
        let summarize_input = match &known.hint {
            Some(hint) => MaskedText {
                text: format!("{}{hint}", masked.text),
            },
            None => masked.clone(),
        };
        let summary_masked = self.gateway.summarize(raw.source, &summarize_input).await?;
        let mut entries = prompts::split_facts(&summary_masked);
        if entries.is_empty() {
            // Local dev only: the summarizer often rules the fake Gmail fixtures
            // "nothing durable" (receipts, newsletters), which would drop them
            // before routing. Keep them by falling back to the masked text so
            // every fixture becomes at least one fact on the dashboard. Release
            // builds preserve the real "found nothing → filtered" decision.
            #[cfg(debug_assertions)]
            if raw.source == crate::core::types::SourceKind::Gmail && !masked.text.trim().is_empty()
            {
                entries = vec![masked.text.clone()];
            }
        }
        if entries.is_empty() {
            // The summarizer found nothing durable; that is a decision, not a
            // failure, and the report should say so.
            report.filtered += 1;
            return Ok(());
        }

        // Each entry is classified on its own, so the model cannot know what an
        // earlier item called the same subject — `gemini-api` here,
        // `gemini-api-limits` there — and the topic pages fragment into
        // near-synonyms that each look like a one-off. Showing it the labels
        // already in use turns "invent a label" into "reuse one if it fits".
        // The vocabulary is built from prior masked outputs, so appending it to
        // masked input keeps the egress contract intact. Read once per item.
        let vocab_hint = match self.existing_topics().await {
            Some(vocab) if !vocab.is_empty() => Some(format!(
                "\n\n[topics already in use — reuse one of these when it fits: {}]",
                vocab.join(", ")
            )),
            _ => None,
        };

        for entry_masked in entries {
            let classify_input = MaskedText {
                text: match &vocab_hint {
                    Some(hint) => format!("{entry_masked}{hint}"),
                    None => entry_masked.clone(),
                },
            };
            let labels = self.gateway.classify(raw.source, &classify_input).await?;

            // 4. Local unmask for the stored body (US-2.2 AC2). For the image
            //    path there is no reverse map, so the entry is used as-is.
            let body = match &unmap {
                Some(map) => self.masker.unmask(&MaskedText { text: entry_masked }, map),
                None => entry_masked,
            };

            // 5. Route.
            match route(&labels, &body, raw) {
                ProcessingDecision::Store(cand) => {
                    self.knowledge.upsert(fact_from(cand)).await?;
                    report.facts_created += 1;
                }
                ProcessingDecision::Confirm(cand) => {
                    // Queue-side suppression only compares against items still
                    // pending, so an idea the user already answered comes back
                    // as a brand-new question the next time any session
                    // mentions it. If it is already a confirmed fact, there is
                    // nothing left to ask.
                    if known
                        .facts
                        .iter()
                        .any(|t| crate::core::text::is_near_duplicate(t, &cand.title))
                    {
                        report.filtered += 1;
                    } else {
                        self.interview.enqueue(confirm_item(cand)).await?;
                        report.queue_items_created += 1;
                    }
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
        }
        // 6. `unmap` drops here.
        Ok(())
    }

    /// Re-process everything parked in the pending queue (called on reconnection,
    /// e.g. from the U1 Scheduler tick — Q3=A).
    /// Titles already on file, for the summarizer to reuse instead of coining a
    /// synonym — plus the plain fact-title list the Confirm route checks against.
    ///
    /// Egress: a stored title is *unmasked* text, so it cannot simply be pasted
    /// into a [`MaskedText`]. Each candidate is run back through the masker and
    /// kept only if nothing was masked — i.e. it is already safe to send. That
    /// also removes a second hazard: placeholders are numbered per masking call,
    /// so a hint carrying `«NAME_1»` would be unmasked with *this* item's map
    /// and silently attributed to the wrong person.
    async fn known_titles(&self) -> KnownTitles {
        const MAX_HINTS: usize = 30;

        let facts: Vec<String> = self
            .knowledge
            .search(String::new(), crate::core::types::FactFilter::default())
            .await
            .map(|f| f.into_iter().map(|s| s.title).collect())
            .unwrap_or_default();
        // Pending questions first: they are the surface the duplication is
        // visible on, and there are only ever a few dozen of them.
        let pending: Vec<String> = self
            .interview
            .list(crate::core::types::QueueSort::PriorityDesc)
            .await
            .map(|items| {
                items
                    .into_iter()
                    .filter_map(|i| match i.kind {
                        crate::core::types::QueueItemKind::Confirm { candidate } => {
                            Some(candidate.title)
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut shown: Vec<String> = Vec::new();
        for title in pending.iter().chain(facts.iter()) {
            if shown.len() >= MAX_HINTS {
                break;
            }
            let title = title.trim();
            if title.is_empty() {
                continue;
            }
            let (masked, map) = self.masker.mask(title);
            if !map.is_empty() || masked.text != title {
                continue; // carries an identifier; never leaves the machine
            }
            if shown
                .iter()
                .any(|k| crate::core::text::is_near_duplicate(k, title))
            {
                continue; // do not spend the budget saying the same thing twice
            }
            shown.push(title.to_string());
        }

        // With a large vault this is a sample, not an index — which is why the
        // queue still needs its own near-duplicate rule behind this.
        let hint = (!shown.is_empty()).then(|| {
            format!(
                "\n\n[titles already recorded — if an item below means the same \
                 thing as one of these, reuse that title EXACTLY instead of \
                 writing a new one:\n- {}]",
                shown.join("\n- ")
            )
        });
        KnownTitles { hint, facts }
    }

    /// The most-used topic labels, for the classifier to reuse.
    ///
    /// Capped so the hint stays a hint: a few dozen labels is a vocabulary, a
    /// few hundred is a second document. Failure to read is not fatal — the
    /// classifier simply invents labels as it did before.
    async fn existing_topics(&self) -> Option<Vec<String>> {
        const MAX_HINTS: usize = 40;
        let pages = self
            .knowledge
            .topics(crate::core::types::FactFilter::default())
            .await
            .ok()?;
        Some(pages.into_iter().take(MAX_HINTS).map(|p| p.topic).collect())
    }

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

/// Bridges Ingestion (U2) to Processing (U2).
///
/// `IngestionService` speaks [`RawItemSink`], which returns nothing;
/// `ProcessingApi::process` returns a [`ProcessReport`] the UI wants to show
/// ("N facts, M questions"). This adapter accumulates those reports so a
/// triggered sync can report what actually came of it.
pub struct ProcessingSink {
    processing: Arc<ProcessingService>,
    totals: std::sync::Mutex<ProcessReport>,
}

impl ProcessingSink {
    pub fn new(processing: Arc<ProcessingService>) -> Self {
        Self {
            processing,
            totals: std::sync::Mutex::new(ProcessReport::default()),
        }
    }

    /// Totals accumulated since the last [`ProcessingSink::take`].
    pub fn take(&self) -> ProcessReport {
        std::mem::take(&mut self.totals.lock().expect("totals mutex poisoned"))
    }
}

#[async_trait]
impl RawItemSink for ProcessingSink {
    async fn accept(&self, items: Vec<RawItem>) -> Result<()> {
        let report = self.processing.process(items).await?;
        let mut totals = self.totals.lock().expect("totals mutex poisoned");
        totals.facts_created += report.facts_created;
        totals.queue_items_created += report.queue_items_created;
        totals.filtered += report.filtered;
        Ok(())
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
            topics: c.topics,
            kind: c.kind,
            visibility: c.visibility,
            category: None,
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
    use crate::core::types::{Provenance, QueueSort};
    use crate::mocks::{
        CannedLlm, InMemoryInterview, InMemoryKnowledge, InMemoryStore, NoopMasker,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

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

    /// Records what the summarizer was handed, and answers `classify` with a
    /// caller-chosen label so a test can pick the route.
    struct Recording {
        seen: Mutex<Vec<String>>,
        label: &'static str,
        summary: &'static str,
    }
    #[async_trait::async_trait]
    impl LlmClient for Recording {
        async fn summarize(&self, input: &MaskedText) -> Result<String> {
            self.seen.lock().unwrap().push(input.text.clone());
            Ok(self.summary.to_string())
        }
        async fn classify(&self, _i: &MaskedText) -> Result<Vec<String>> {
            Ok(vec![self.label.to_string()])
        }
        async fn vision_extract(&self, _i: &[u8]) -> Result<MaskedText> {
            Ok(MaskedText {
                text: String::new(),
            })
        }
        async fn chat(&self, _s: &str, _i: &MaskedText) -> Result<String> {
            Ok(String::new())
        }
    }

    fn service_with(
        llm: Arc<dyn LlmClient>,
        masker: Arc<dyn Masker>,
    ) -> (
        ProcessingService,
        Arc<InMemoryKnowledge>,
        Arc<InMemoryInterview>,
    ) {
        let store = Arc::new(InMemoryStore::default());
        let knowledge = Arc::new(InMemoryKnowledge::default());
        let interview = Arc::new(InMemoryInterview::default());
        let svc = ProcessingService::new(
            masker,
            llm,
            Arc::new(TransferLog::new(store.clone())),
            knowledge.clone(),
            interview.clone(),
            Arc::new(PendingQueue::new(store)),
            Arc::new(Toggle(AtomicBool::new(true))),
        );
        (svc, knowledge, interview)
    }

    fn confirmed(title: &str) -> crate::core::types::Fact {
        crate::core::types::Fact {
            id: crate::core::types::FactId::new(),
            title: title.into(),
            body: "b".into(),
            links: vec![],
            metadata: crate::core::types::FactMetadata {
                provenance: Provenance {
                    source: SourceKind::Session,
                    collected_at: Utc::now(),
                },
                confirmed: true,
                scope: crate::core::types::Scope::Personal,
                confirmed_at: Some(Utc::now()),
                topics: vec![],
                kind: Default::default(),
                visibility: Default::default(),
                category: None,
            },
        }
    }

    #[tokio::test]
    async fn the_summarizer_is_shown_the_titles_already_on_file() {
        // Without this the model coins a synonym per session and every
        // duplicate filter downstream compares two strings that never match.
        let llm = Arc::new(Recording {
            seen: Mutex::new(vec![]),
            label: "general",
            summary: "무언가\n본문",
        });
        let (svc, knowledge, _iv) = service_with(llm.clone(), Arc::new(NoopMasker));
        knowledge
            .upsert(confirmed("검증 경로가 무너지는 게 내 진짜 걱정"))
            .await
            .unwrap();

        svc.process(vec![raw("a", "오늘 한 일")]).await.unwrap();

        let sent = llm.seen.lock().unwrap().join("");
        assert!(
            sent.contains("검증 경로가 무너지는 게 내 진짜 걱정"),
            "summarizer never saw the recorded title:\n{sent}"
        );
        assert!(
            sent.contains("오늘 한 일"),
            "the item itself must still be there"
        );
    }

    #[tokio::test]
    async fn a_title_carrying_an_identifier_is_never_sent_as_a_hint() {
        // Stored titles are unmasked. Pasting one into the model's input would
        // walk PII straight past the masking gateway — and a placeholder from
        // another masking call would be unmasked with *this* item's map and
        // attributed to the wrong person.
        let llm = Arc::new(Recording {
            seen: Mutex::new(vec![]),
            label: "general",
            summary: "무언가\n본문",
        });
        let (svc, knowledge, _iv) =
            service_with(llm.clone(), Arc::new(crate::llm::RegexMasker::new()));
        knowledge
            .upsert(confirmed("jane.doe@example.com 에게 배포 알림을 보낸다"))
            .await
            .unwrap();
        knowledge
            .upsert(confirmed("배포는 금요일에 하지 않는다"))
            .await
            .unwrap();

        svc.process(vec![raw("a", "오늘 한 일")]).await.unwrap();

        let sent = llm.seen.lock().unwrap().join("");
        assert!(!sent.contains("jane.doe@example.com"), "leaked:\n{sent}");
        assert!(
            !sent.contains("EMAIL_1"),
            "placeholder from another map:\n{sent}"
        );
        assert!(
            sent.contains("배포는 금요일에 하지 않는다"),
            "clean title dropped too"
        );
    }

    #[tokio::test]
    async fn a_candidate_already_confirmed_is_not_queued_again() {
        // Queue-side suppression only sees items still pending, so answering a
        // question used to guarantee it would be asked again from the next
        // session that mentioned it.
        let llm = Arc::new(Recording {
            seen: Mutex::new(vec![]),
            label: "uncertain",
            summary: "기술 작업은 AI에 맡기고 본인은 공부에만 집중하려 함\n본문",
        });
        let (svc, knowledge, interview) = service_with(llm, Arc::new(NoopMasker));
        knowledge
            .upsert(confirmed(
                "기술 작업은 AI에 맡기고 본인은 학습에만 집중하려 함",
            ))
            .await
            .unwrap();

        let r = svc.process(vec![raw("a", "오늘 한 일")]).await.unwrap();

        assert_eq!(
            r.queue_items_created, 0,
            "already answered — nothing to ask"
        );
        assert_eq!(r.filtered, 1);
        assert!(interview
            .list(QueueSort::PriorityDesc)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn an_unrelated_candidate_is_still_queued() {
        let llm = Arc::new(Recording {
            seen: Mutex::new(vec![]),
            label: "uncertain",
            summary: "PDF 한글화에서 Mermaid 글자가 안 보이는 문제\n본문",
        });
        let (svc, knowledge, _iv) = service_with(llm, Arc::new(NoopMasker));
        knowledge
            .upsert(confirmed(
                "기술 작업은 AI에 맡기고 본인은 학습에만 집중하려 함",
            ))
            .await
            .unwrap();

        let r = svc.process(vec![raw("a", "오늘 한 일")]).await.unwrap();
        assert_eq!(r.queue_items_created, 1);
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
    async fn fake_gmail_fixtures_all_reach_the_dashboard() {
        // Local-dev contract: every fake Gmail message must show up on the
        // dashboard (Store), never silently drop or wait in the queue — the
        // router has a debug-only override for SourceKind::Gmail.
        use crate::ingestion::connectors::gmail_fixtures::fake_gmail_items;
        let items = fake_gmail_items();
        let n = items.len();
        assert!(n > 0, "fixtures must not be empty");

        let (svc, knowledge, _iv, _p, _t) = service(true);
        let r = svc.process(items).await.unwrap();

        // No fixture is dropped or parked; each becomes at least one fact.
        assert_eq!(r.filtered, 0, "no fake Gmail item should be filtered");
        assert_eq!(r.queue_items_created, 0, "none should go to the queue");
        assert!(
            r.facts_created >= n,
            "every fixture ({n}) should yield a fact, got {}",
            r.facts_created
        );
        assert_eq!(
            knowledge.dashboard().await.unwrap().collected_count,
            r.facts_created
        );
    }

    #[tokio::test]
    async fn fake_gmail_survives_a_nothing_summary() {
        // The real Claude summarizer often calls receipts/newsletters "NOTHING",
        // which would drop them before routing. In dev, the Gmail fallback keeps
        // each fixture as a fact anyway so the dashboard is never near-empty.
        use crate::ingestion::connectors::gmail_fixtures::fake_gmail_items;
        let llm = Arc::new(Recording {
            seen: Mutex::new(vec![]),
            label: "general",
            summary: "NOTHING",
        });
        let (svc, knowledge, _iv) = service_with(llm, Arc::new(NoopMasker));

        let items = fake_gmail_items();
        let n = items.len();
        let r = svc.process(items).await.unwrap();

        assert_eq!(r.filtered, 0, "Gmail must not be filtered even on NOTHING");
        assert_eq!(r.facts_created, n, "every fixture kept as a fact");
        assert_eq!(knowledge.dashboard().await.unwrap().collected_count, n);
    }

    #[tokio::test]
    async fn a_nothing_summary_still_drops_non_gmail() {
        // The fallback is Gmail-only: a Session item the summarizer rejects is
        // still filtered, so the dev shim can't mask real "nothing" decisions.
        let llm = Arc::new(Recording {
            seen: Mutex::new(vec![]),
            label: "general",
            summary: "NOTHING",
        });
        let (svc, knowledge, _iv) = service_with(llm, Arc::new(NoopMasker));

        let r = svc.process(vec![raw("s1", "just noise")]).await.unwrap();
        assert_eq!(r.filtered, 1);
        assert_eq!(r.facts_created, 0);
        assert_eq!(knowledge.dashboard().await.unwrap().collected_count, 0);
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
