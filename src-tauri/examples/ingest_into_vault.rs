//! Collect *named* session roots into an existing vault without disturbing the
//! collection cursor — for pulling specific sessions into the app's own vault.
//!
//! The normal path ([`ingest_roots`]) goes through `IngestionService`, which
//! persists the connector's two watermarks. Those advance monotonically: the
//! backfill frontier only ever moves *older*. So pointing a normal sync at an
//! old project directory drags that frontier down past every session in
//! between, and the app then treats all of them as already covered — the
//! backlog is silently discarded rather than collected later.
//!
//! Here the connector is driven directly with a null cursor (so every file
//! under the given roots is in scope) and the cursor it returns is dropped.
//! The vault gains the facts; the app's collection state is untouched.
//!
//! ```bash
//! read -rs KNOWSME_DEMO_PASSWORD && export KNOWSME_DEMO_PASSWORD
//! KNOWSME_SESSION_ROOTS=/abs/project-dir \
//!   KNOWSME_DATA_DIR="$HOME/Library/Application Support/app.knowsme.desktop" \
//!   cargo run --release --example ingest_into_vault
//! ```
//!
//! Close the app first: the encrypted store is files on disk with no lock, and
//! two writers are two writers.

use std::sync::Arc;

use knows_me_core::core::traits::{
    Connector, CredentialStore, EncryptedStore, InterviewApi, KeyManager, KnowledgeApi, Masker,
    ProcessingApi,
};
use knows_me_core::core::types::{FactFilter, QueueSort};
use knows_me_core::ingestion::connectors::SessionConnector;
use knows_me_core::interview::InterviewService;
use knows_me_core::knowledge::KnowledgeService;
use knows_me_core::llm::RegexMasker;
use knows_me_core::processing::service::AlwaysOnline;
use knows_me_core::processing::{PendingQueue, ProcessingService, TransferLog};
use knows_me_core::security::{FileEncryptedStore, PasswordKeyManager};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = std::env::var("KNOWSME_DATA_DIR").unwrap_or_else(|_| {
        eprintln!("KNOWSME_DATA_DIR is required (the vault to write into)");
        std::process::exit(2);
    });
    // No default: guessing a password against a real vault just fails with a
    // confusing error, and a wrong default silently creating a *second* vault
    // would be worse.
    let password = std::env::var("KNOWSME_DEMO_PASSWORD").unwrap_or_else(|_| {
        eprintln!("KNOWSME_DEMO_PASSWORD is required (the vault's unlock password)");
        eprintln!("  read -rs KNOWSME_DEMO_PASSWORD && export KNOWSME_DEMO_PASSWORD");
        std::process::exit(2);
    });
    std::fs::create_dir_all(&data_dir)?;

    println!("vault      : {data_dir}");
    println!(
        // `active_model_label` shares `build_client`'s selection logic, so this
        // reports the backend that actually runs. Guessing from env vars said
        // "(default anthropic)" while every call went to the local Claude CLI.
        "llm        : {}",
        knows_me_core::llm::active_model_label()
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
    // No `ProcessingSink` here: the sink exists to bridge the ingestion
    // service's push interface, and this example calls processing directly.
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
    let root_paths: Vec<std::path::PathBuf> = roots
        .iter()
        .filter_map(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .collect();
    let connector = SessionConnector::new(root_paths);

    // --- run ---------------------------------------------------------------
    let started = std::time::Instant::now();
    println!("\n수집 시작…");
    // `None` cursor = every file under these roots is in scope. The returned
    // cursor is deliberately dropped (see the module docs); nothing here writes
    // to the cursor store, so the app's own backfill position is preserved.
    let (items, _discarded_cursor) = connector
        .sync(None, &knows_me_core::core::traits::NoProgress)
        .await?;
    let collected = items.len();
    let report = processing.process(items).await?;

    println!("\n=== 수집 ===");
    println!("  수집 {collected}건 (커서 미변경)");
    println!("=== 가공 ===");
    println!(
        "  확정 사실 {} · 질문 {} · 걸러냄 {}",
        report.facts_created, report.queue_items_created, report.filtered
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
