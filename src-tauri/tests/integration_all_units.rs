//! Cross-unit integration — the Build and Test stage in miniature.
//!
//! The per-unit suites all pass against mocks. This file asks a different
//! question: do the four units work when wired to each other's **real**
//! implementations? It builds the stack the desktop app would build —
//!
//!   U1 PasswordKeyManager + FileEncryptedStore + RegexMasker
//!     -> U3 KnowledgeService + InterviewService
//!       -> U4 QueryService + PersonaService + LocalApiServer
//!
//! — over a real encrypted store on disk, and drives it end to end.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use knows_me_core::core::error::{AppError, Result};
use knows_me_core::core::traits::KeyManager;
use knows_me_core::core::traits::ProcessingApi;
use knows_me_core::core::traits::{
    EncryptedStore, InterviewApi, KnowledgeApi, LlmClient, Masker, PersonaApi,
};
use knows_me_core::core::types::{
    AnswerInput, Fact, FactCandidate, FactId, FactMetadata, GraphFilter, MaskedText, Provenance,
    QueueItem, QueueItemId, QueueItemKind, QueueSort, RawItem, Scope, SourceKind,
};
use knows_me_core::interview::InterviewService;
use knows_me_core::knowledge::KnowledgeService;
use knows_me_core::llm::RegexMasker;
use knows_me_core::persona::{LocalApiServer, PersonaService, QueryService, NO_CONTEXT_REPLY};
use knows_me_core::processing::service::AlwaysOnline;
use knows_me_core::processing::{PendingQueue, ProcessingService, TransferLog};
use knows_me_core::security::{FileEncryptedStore, PasswordKeyManager};

// ---------------------------------------------------------------------------
// Test doubles for the one thing that must not be real: the network.
// ---------------------------------------------------------------------------

/// Echoes back what it was given, so assertions can inspect exactly what the
/// gateway received. Standing in for the cloud LLM — the only component a test
/// must not use for real.
#[derive(Default)]
struct EchoLlm {
    seen: std::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl LlmClient for EchoLlm {
    async fn summarize(&self, input: &MaskedText) -> Result<String> {
        Ok(input.text.clone())
    }
    async fn classify(&self, _input: &MaskedText) -> Result<Vec<String>> {
        Ok(vec!["general".into()])
    }
    async fn vision_extract(&self, _png: &[u8]) -> Result<MaskedText> {
        Ok(MaskedText {
            text: String::new(),
        })
    }
    async fn chat(&self, system: &str, input: &MaskedText) -> Result<String> {
        self.seen.lock().unwrap().push(system.to_string());
        self.seen.lock().unwrap().push(input.text.clone());
        Ok(format!("답변: {}", input.text))
    }
}

// ---------------------------------------------------------------------------
// Stack assembly — the same wiring the desktop shell would perform.
// ---------------------------------------------------------------------------

struct Stack {
    knowledge: Arc<KnowledgeService>,
    interview: Arc<InterviewService>,
    query: QueryService,
    persona: Arc<PersonaService>,
    llm: Arc<EchoLlm>,
    _dir: tempfile::TempDir,
}

async fn unlocked_stack() -> Stack {
    let dir = tempfile::tempdir().expect("tempdir");

    // --- U1: real key manager + real AES-GCM encrypted store on disk --------
    let keys = Arc::new(PasswordKeyManager::new(dir.path()));
    keys.setup("correct horse battery staple")
        .expect("password setup");
    keys.unlock("correct horse battery staple").expect("unlock");

    let store: Arc<dyn EncryptedStore> = Arc::new(FileEncryptedStore::new(
        dir.path().join("store"),
        keys.clone(),
    ));
    let masker: Arc<dyn Masker> = Arc::new(RegexMasker::new());
    let llm = Arc::new(EchoLlm::default());
    let llm_dyn: Arc<dyn LlmClient> = llm.clone();

    // --- U3: real knowledge + interview services ----------------------------
    let knowledge = Arc::new(KnowledgeService::new(store.clone()));
    let knowledge_dyn: Arc<dyn KnowledgeApi> = knowledge.clone();
    let interview = Arc::new(InterviewService::new(
        store.clone(),
        knowledge_dyn.clone(),
        masker.clone(),
        llm_dyn.clone(),
    ));

    // --- U4: read views + persona over the real stack -----------------------
    let query = QueryService::new(knowledge_dyn.clone());
    let persona = Arc::new(PersonaService::new(knowledge_dyn, masker, llm_dyn));

    Stack {
        knowledge,
        interview,
        query,
        persona,
        llm,
        _dir: dir,
    }
}

fn fact(title: &str, body: &str, links: Vec<FactId>) -> Fact {
    Fact {
        id: FactId::new(),
        title: title.into(),
        body: body.into(),
        links,
        metadata: FactMetadata {
            provenance: Provenance {
                source: SourceKind::Session,
                collected_at: Utc::now(),
            },
            confirmed: true,
            scope: Scope::Company,
            confirmed_at: Some(Utc::now()),
        },
    }
}

// ---------------------------------------------------------------------------
// U1 + U3 + U4
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn read_views_serve_facts_written_through_the_real_encrypted_store() {
    let s = unlocked_stack().await;

    let b = fact("코드 리뷰 규칙", "PR 은 1인 승인 후 머지", vec![]);
    let a = fact("배포 절차", "main 머지 후 make deploy", vec![b.id]);
    s.knowledge.upsert(b).await.unwrap();
    s.knowledge.upsert(a).await.unwrap();

    // US-5.1
    let dash = s.query.dashboard().await.unwrap();
    assert_eq!(dash.collected_count, 2);

    // US-5.2 — most-connected fact leads.
    let mini = s.query.minihome(Some(9)).await.unwrap();
    assert_eq!(mini.highlights.len(), 2);
    assert_eq!(mini.highlights[0].title, "배포 절차");

    // US-5.3 — the link becomes an edge.
    let graph = s.query.graph(GraphFilter::default()).await.unwrap();
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn persona_grounds_answers_in_facts_confirmed_through_the_interview_queue() {
    let s = unlocked_stack().await;

    // A fact enters the way it really does: processing enqueues a candidate,
    // the owner confirms it, U3 promotes it into the knowledge base.
    let item = QueueItem {
        id: QueueItemId::new(),
        kind: QueueItemKind::Confirm {
            candidate: FactCandidate {
                title: "배포 절차".into(),
                body: "main 머지 후 make deploy 로 배포한다".into(),
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
    let id = s.interview.enqueue(item).await.unwrap();
    assert_eq!(
        s.interview
            .list(QueueSort::PriorityDesc)
            .await
            .unwrap()
            .len(),
        1
    );

    let answered = s
        .interview
        .answer(id, AnswerInput::Choice("네".into()))
        .await
        .unwrap();
    assert!(
        answered.confirmed_fact.is_some(),
        "confirming a candidate must produce a fact"
    );

    // U4 now answers from it.
    let reply = s
        .persona
        .chat("배포 절차 알려줘".into(), vec![])
        .await
        .unwrap();
    assert_ne!(reply.text, NO_CONTEXT_REPLY, "persona should be grounded");
    assert!(
        reply.text.contains("make deploy"),
        "grounding text should reach the answer, got: {}",
        reply.text
    );

    // ...and the dashboard reflects the queue draining.
    let dash = s.query.dashboard().await.unwrap();
    assert_eq!(dash.collected_count, 1);
    assert_eq!(dash.pending_queue, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn identifiers_are_masked_by_the_real_masker_before_leaving_the_device() {
    let s = unlocked_stack().await;

    s.knowledge
        .upsert(fact(
            "연락처",
            "급하면 hong@example.com 또는 010-1234-5678 로 연락",
            vec![],
        ))
        .await
        .unwrap();

    let reply = s
        .persona
        .chat("연락처 알려줘".into(), vec![])
        .await
        .unwrap();

    // Everything the gateway saw, system prompt included.
    let seen = s.llm.seen.lock().unwrap().join("\n");
    assert!(!seen.is_empty(), "the gateway should have been called");
    assert!(
        !seen.contains("hong@example.com"),
        "raw email reached the gateway:\n{seen}"
    );
    assert!(
        !seen.contains("010-1234-5678"),
        "raw phone number reached the gateway:\n{seen}"
    );

    // The owner still gets the real values back — unmasking is local.
    assert!(
        reply.text.contains("hong@example.com"),
        "restoration failed, got: {}",
        reply.text
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn read_views_answer_with_no_llm_wired_in_at_all() {
    // NFR-3 / U4-NFR-A1: the read path must not depend on the network. Here it
    // is proven structurally — QueryService is constructed without an LlmClient,
    // so there is nothing for it to call even if it wanted to.
    let s = unlocked_stack().await;
    s.knowledge
        .upsert(fact("오프라인 사실", "네트워크 없이 조회된다", vec![]))
        .await
        .unwrap();

    let query = QueryService::new(s.knowledge.clone() as Arc<dyn KnowledgeApi>);
    assert_eq!(query.dashboard().await.unwrap().collected_count, 1);
    assert_eq!(query.minihome(None).await.unwrap().highlights.len(), 1);
    assert_eq!(
        query
            .graph(GraphFilter::default())
            .await
            .unwrap()
            .nodes
            .len(),
        1
    );

    // No LLM call was made by any of the three.
    assert!(s.llm.seen.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn persona_says_it_cannot_answer_before_anything_is_confirmed() {
    let s = unlocked_stack().await;

    let reply = s
        .persona
        .chat("아무거나 물어봄".into(), vec![])
        .await
        .unwrap();

    assert_eq!(reply.text, NO_CONTEXT_REPLY);
    assert!(
        s.llm.seen.lock().unwrap().is_empty(),
        "an ungrounded question must not reach the cloud (BR-P4)"
    );
}

// ---------------------------------------------------------------------------
// Local API over the real stack
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn local_api_drafts_over_the_real_stack_and_refuses_non_loopback_hosts() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let s = unlocked_stack().await;
    s.knowledge
        .upsert(fact("배포 절차", "main 머지 후 make deploy", vec![]))
        .await
        .unwrap();

    let persona: Arc<dyn PersonaApi> = s.persona.clone();
    let mut handle = LocalApiServer::start(persona, 0).await.unwrap();
    let port = handle.port();

    let request = move |host: &'static str, body: &'static str| {
        tokio::task::spawn_blocking(move || {
            let mut c = TcpStream::connect(("127.0.0.1", port)).expect("connect");
            let req = format!(
                "POST /draft HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            c.write_all(req.as_bytes()).unwrap();
            let mut raw = String::new();
            c.read_to_string(&mut raw).unwrap();
            let status: u16 = raw.split_whitespace().nth(1).unwrap().parse().unwrap();
            (
                status,
                raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string(),
            )
        })
    };

    let (status, body) = request(
        "127.0.0.1",
        r#"{"kind":"Email","prompt":"배포 일정 공유 메일"}"#,
    )
    .await
    .unwrap();
    assert_eq!(status, 200);
    assert!(
        body.contains("make deploy"),
        "draft should be grounded: {body}"
    );

    let (status, _) = request(
        "evil.example.com",
        r#"{"kind":"Email","prompt":"배포 일정 공유 메일"}"#,
    )
    .await
    .unwrap();
    assert_eq!(status, 403, "non-loopback Host must be refused (D7)");

    handle.stop().await;
}

// ---------------------------------------------------------------------------
// Security posture of the assembled stack
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn locking_the_vault_blocks_reads_across_every_unit() {
    let dir = tempfile::tempdir().unwrap();
    let keys = Arc::new(PasswordKeyManager::new(dir.path()));
    keys.setup("pw").unwrap();
    keys.unlock("pw").unwrap();
    let store: Arc<dyn EncryptedStore> = Arc::new(FileEncryptedStore::new(
        dir.path().join("store"),
        keys.clone(),
    ));

    let knowledge = Arc::new(KnowledgeService::new(store));
    knowledge
        .upsert(fact("잠금 전 사실", "본문", vec![]))
        .await
        .unwrap();

    let query = QueryService::new(knowledge.clone() as Arc<dyn KnowledgeApi>);
    assert_eq!(query.dashboard().await.unwrap().collected_count, 1);

    keys.lock();

    // A fresh service cannot read the store without the key.
    let cold = KnowledgeService::new(Arc::new(FileEncryptedStore::new(
        dir.path().join("store"),
        keys,
    )) as Arc<dyn EncryptedStore>);
    let err = QueryService::new(Arc::new(cold) as Arc<dyn KnowledgeApi>)
        .dashboard()
        .await;
    assert!(
        matches!(err, Err(AppError::Locked)),
        "locked vault must refuse reads, got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn facts_are_encrypted_at_rest() {
    let dir = tempfile::tempdir().unwrap();
    let keys = Arc::new(PasswordKeyManager::new(dir.path()));
    keys.setup("pw").unwrap();
    keys.unlock("pw").unwrap();
    let store: Arc<dyn EncryptedStore> =
        Arc::new(FileEncryptedStore::new(dir.path().join("store"), keys));
    let knowledge = KnowledgeService::new(store);

    knowledge
        .upsert(fact("비밀 제목", "아주 민감한 본문 내용", vec![]))
        .await
        .unwrap();

    // Walk everything written under the data dir and assert the plaintext is
    // nowhere in it.
    let mut checked = 0usize;
    for entry in walk(dir.path()) {
        let bytes = std::fs::read(&entry).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("아주 민감한 본문 내용"),
            "plaintext body found on disk in {entry:?}"
        );
        assert!(
            !text.contains("비밀 제목"),
            "plaintext title found on disk in {entry:?}"
        );
        checked += 1;
    }
    assert!(checked > 0, "expected the store to have written something");
}

fn walk(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&p) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(path);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// All four units: U2 ingests -> U3 stores/queues -> U4 answers
// ---------------------------------------------------------------------------

/// Returns labels that route a raw item to a confirmed fact rather than to the
/// interview queue, so the whole chain runs in one test.
struct DecisiveLlm;

#[async_trait]
impl LlmClient for DecisiveLlm {
    async fn summarize(&self, input: &MaskedText) -> Result<String> {
        Ok(input.text.clone())
    }
    async fn classify(&self, _input: &MaskedText) -> Result<Vec<String>> {
        Ok(vec!["certain".into(), "company".into()])
    }
    async fn vision_extract(&self, _png: &[u8]) -> Result<MaskedText> {
        Ok(MaskedText {
            text: String::new(),
        })
    }
    async fn chat(&self, _system: &str, input: &MaskedText) -> Result<String> {
        Ok(format!("답변: {}", input.text))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_raw_item_travels_from_processing_all_the_way_to_a_persona_answer() {
    let dir = tempfile::tempdir().unwrap();
    let keys = Arc::new(PasswordKeyManager::new(dir.path()));
    keys.setup("pw").unwrap();
    keys.unlock("pw").unwrap();
    let store: Arc<dyn EncryptedStore> =
        Arc::new(FileEncryptedStore::new(dir.path().join("store"), keys));

    let masker: Arc<dyn Masker> = Arc::new(RegexMasker::new());
    let llm: Arc<dyn LlmClient> = Arc::new(DecisiveLlm);
    // NOTE: two `TransferLog` types exist after the merge — `llm::TransferLog`
    // (U1, in-memory, feeds the `list_transfers` command) and
    // `processing::TransferLog` (U2, persisted through the encrypted store).
    // U2's pipeline takes the latter. Consolidating them is an integration
    // follow-up, not something this test can paper over.
    let transfer_log = Arc::new(TransferLog::new(store.clone()));

    // U3
    let knowledge = Arc::new(KnowledgeService::new(store.clone()));
    let knowledge_dyn: Arc<dyn KnowledgeApi> = knowledge.clone();
    let interview: Arc<dyn InterviewApi> = Arc::new(InterviewService::new(
        store.clone(),
        knowledge_dyn.clone(),
        masker.clone(),
        llm.clone(),
    ));

    // U2
    let processing = ProcessingService::new(
        masker.clone(),
        llm.clone(),
        transfer_log.clone(),
        knowledge_dyn.clone(),
        interview.clone(),
        Arc::new(PendingQueue::new(store)),
        Arc::new(AlwaysOnline),
    );

    // U4
    let query = QueryService::new(knowledge_dyn.clone());
    let persona = PersonaService::new(knowledge_dyn, masker, llm);

    // A session transcript arrives, carrying an identifier that must never
    // leave the device in the clear.
    let report = processing
        .process(vec![RawItem {
            source: SourceKind::Session,
            external_id: "session-001".into(),
            collected_at: Utc::now(),
            text: Some("배포는 main 머지 후 make deploy 로 한다. 문의는 ops@example.com".into()),
            image_png: None,
        }])
        .await
        .unwrap();

    assert!(
        report.facts_created + report.queue_items_created > 0,
        "processing produced nothing: {report:?}"
    );

    // Whatever route it took, the owner ends up with something to look at.
    let dash = query.dashboard().await.unwrap();
    assert!(
        dash.collected_count > 0 || dash.pending_queue > 0,
        "neither a fact nor a queue item reached the owner"
    );

    // The transparency log recorded the cloud call (US-2.3 / NFR-2).
    let entries = transfer_log.all().await.unwrap();
    assert!(
        !entries.is_empty(),
        "an LLM call must be recorded in the transfer log"
    );

    // And nothing the persona sends out carries the raw address.
    let _ = persona.chat("배포 절차".into(), vec![]).await.unwrap();
    for e in transfer_log.all().await.unwrap() {
        let rendered = format!("{e:?}");
        assert!(
            !rendered.contains("ops@example.com"),
            "raw identifier recorded in the transfer log: {rendered}"
        );
    }
}
