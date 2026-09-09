//! Ingest a *specified* set of transcript roots, for diagnosing extraction.
//!
//! [`ingest_sessions`](ingest_sessions) scans wherever the owner's transcripts
//! actually live and takes them a batch at a time, which is right for the app
//! but useless for answering "why did *this* session yield nothing about X?" —
//! the session in question may simply not have been reached yet. This variant
//! takes the roots as an argument so one transcript can be run through the real
//! pipeline and the extracted facts read directly.
//!
//! ```bash
//! KNOWSME_SESSION_ROOTS=~/.claude/projects/<one-project-dir> \
//!   KNOWSME_DATA_DIR=/tmp/vault-x \
//!   cargo run --release --example ingest_roots
//! ```
//!
//! Roots are colon-separated. Writes to `$KNOWSME_DATA_DIR` (default
//! `./.knowsme-roots`), never the real app data directory.

use std::sync::Arc;

use knows_me_core::core::traits::{
    CredentialStore, EncryptedStore, IngestionApi, InterviewApi, KeyManager, KnowledgeApi, Masker,
};
use knows_me_core::core::types::{FactFilter, QueueSort, SourceKind};
use knows_me_core::ingestion::connectors::SessionConnector;
use knows_me_core::ingestion::{ConnectorRegistry, IngestionCursorStore, IngestionService};
use knows_me_core::interview::InterviewService;
use knows_me_core::knowledge::KnowledgeService;
use knows_me_core::llm::RegexMasker;
use knows_me_core::processing::service::AlwaysOnline;
use knows_me_core::processing::{PendingQueue, ProcessingService, ProcessingSink, TransferLog};
use knows_me_core::security::{FileEncryptedStore, PasswordKeyManager};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = std::env::var("KNOWSME_DATA_DIR").unwrap_or_else(|_| ".knowsme-roots".into());
    let password =
        std::env::var("KNOWSME_DEMO_PASSWORD").unwrap_or_else(|_| "demo-password".into());
    std::fs::create_dir_all(&data_dir)?;

    println!("vault      : {data_dir}");
    println!(
        "llm        : provider={} model={}",
        std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "(default anthropic)".into()),
        std::env::var("OPENAI_MODEL")
            .or_else(|_| std::env::var("ANTHROPIC_MODEL"))
            .unwrap_or_else(|_| "(default)".into())
    );

    // --- U1 ----------------------------------------------------------------
    let keys = Arc::new(PasswordKeyManager::new(&data_dir));
    // setup() on an existing vault fails; unlock() covers the repeat run.
    let _ = keys.setup(&password);
    keys.unlock(&password)?;

    let store: Arc<dyn EncryptedStore> = Arc::new(FileEncryptedStore::new(
        std::path::Path::new(&data_dir).join("store"),
        keys.clone(),
    ));
    let credentials: Arc<dyn CredentialStore> = Arc::new(FileEncryptedStore::new(
        std::path::Path::new(&data_dir).join("store"),
        keys,
    ));
    let _ = &credentials; // registered connectors that need it are added below
    let masker: Arc<dyn Masker> = Arc::new(RegexMasker::new());
    let transfer_log = Arc::new(TransferLog::new(store.clone()));
    let llm = knows_me_core::llm::build_client(Arc::new(knows_me_core::llm::TransferLog::new()));

    // --- U3 ----------------------------------------------------------------
    let knowledge_svc = Arc::new(KnowledgeService::new(store.clone()));
    knowledge_svc.build_index().await.ok();
    let knowledge: Arc<dyn KnowledgeApi> = knowledge_svc.clone();
    let interview: Arc<dyn InterviewApi> = Arc::new(InterviewService::new(
        store.clone(),
        knowledge.clone(),
        masker.clone(),
        llm.clone(),
    ));

    // --- U2 ----------------------------------------------------------------
    let processing = Arc::new(ProcessingService::new(
        masker,
        llm,
        transfer_log.clone(),
        knowledge.clone(),
        interview.clone(),
        Arc::new(PendingQueue::new(store.clone())),
        Arc::new(AlwaysOnline),
    ));
    let sink = Arc::new(ProcessingSink::new(processing));

    let mut registry = ConnectorRegistry::new();
    // The whole point of this example: explicit roots instead of auto-detection.
    // `SourceConfig` is the connector's own override channel, so this exercises
    // the same code path the app uses when the owner points it elsewhere —
    // rather than faking `$HOME`, which would also relocate the Claude CLI's
    // credentials and break the LLM backend.
    let roots: Vec<serde_json::Value> = std::env::var("KNOWSME_SESSION_ROOTS")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::Value::String(s.to_string()))
        .collect();
    if roots.is_empty() {
        eprintln!("KNOWSME_SESSION_ROOTS is required (colon-separated absolute paths)");
        std::process::exit(2);
    }
    println!("roots      : {} 개", roots.len());
    let config = knows_me_core::core::types::SourceConfig(serde_json::json!({ "roots": roots }));
    registry.register(Arc::new(SessionConnector::from_config(Some(&config))));
    let ingestion = IngestionService::new(
        Arc::new(registry),
        Arc::new(IngestionCursorStore::new(store)),
        sink.clone(),
    );

    // --- run ---------------------------------------------------------------
    let started = std::time::Instant::now();
    println!("\n수집 시작…");
    let report = ingestion
        .trigger(
            Some(SourceKind::Session),
            &knows_me_core::core::traits::NoProgress,
        )
        .await?;
    let processed = sink.take();

    println!("\n=== 수집 ===");
    println!(
        "  수집 {} · 건너뜀 {} · 오류 {}",
        report.collected, report.skipped, report.errors
    );
    println!("=== 가공 ===");
    println!(
        "  확정 사실 {} · 질문 {} · 걸러냄 {}",
        processed.facts_created, processed.queue_items_created, processed.filtered
    );
    println!("  소요 {:.1}초", started.elapsed().as_secs_f64());

    let dashboard = knowledge.dashboard().await?;
    println!("\n=== 지식베이스 ===");
    println!(
        "  사실 {} · 대기 질문 {}",
        dashboard.collected_count, dashboard.pending_queue
    );

    let facts = knowledge
        .search(String::new(), FactFilter::default())
        .await?;
    for f in facts.iter().take(10) {
        println!("  · [{:?}] {}", f.scope, f.title);
    }

    let queue = interview.list(QueueSort::PriorityDesc).await?;
    if !queue.is_empty() {
        println!("\n=== 인터뷰 대기열 (상위 5) ===");
        for item in queue.iter().take(5) {
            match &item.kind {
                knows_me_core::core::types::QueueItemKind::Confirm { candidate } => {
                    println!("  · 확인: {}", candidate.title)
                }
                knows_me_core::core::types::QueueItemKind::Deepen { question, .. } => {
                    println!("  · 심화: {question}")
                }
            }
        }
    }

    let transfers = transfer_log.all().await?;
    println!("\n=== 전송 투명성 로그 ===");
    println!("  외부 전송 {}건 기록됨", transfers.len());

    Ok(())
}
