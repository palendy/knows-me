//! Print the interview queue with each candidate's full body.
//!
//! `dump_facts` shows what was *confirmed*; a candidate the classifier marked
//! uncertain never reaches it, so "the pipeline did not extract X" and "the
//! pipeline extracted X and is waiting to be asked about it" look identical
//! from there. Other views clip titles to 80 characters, which hides the body
//! that actually says what the candidate claims.
//!
//! ```bash
//! KNOWSME_DATA_DIR=/tmp/vault-x cargo run --release --example dump_queue
//! ```
//!
//! The LLM here is the canned mock: listing the queue never calls out, and
//! wiring a real backend would make a read-only inspection able to spend money.
use knows_me_core::core::traits::{EncryptedStore, InterviewApi, KeyManager};
use knows_me_core::core::types::{QueueItemKind, QueueSort};
use knows_me_core::interview::InterviewService;
use knows_me_core::knowledge::KnowledgeService;
use knows_me_core::llm::RegexMasker;
use knows_me_core::mocks::CannedLlm;
use knows_me_core::security::{FileEncryptedStore, PasswordKeyManager};
use std::sync::Arc;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::var("KNOWSME_DATA_DIR").unwrap_or_else(|_| ".knowsme-demo".into());
    let pw = std::env::var("KNOWSME_DEMO_PASSWORD").unwrap_or_else(|_| "demo-password".into());
    let keys = Arc::new(PasswordKeyManager::new(&dir));
    keys.unlock(&pw)?;
    let store: Arc<dyn EncryptedStore> = Arc::new(FileEncryptedStore::new(
        std::path::Path::new(&dir).join("store"),
        keys,
    ));
    let knowledge = Arc::new(KnowledgeService::new(store.clone()));
    knowledge.build_index().await.ok();
    let svc = InterviewService::new(
        store,
        knowledge,
        Arc::new(RegexMasker::new()),
        Arc::new(CannedLlm),
    );
    for item in svc.list(QueueSort::PriorityDesc).await? {
        match &item.kind {
            QueueItemKind::Confirm { candidate } => println!(
                "--- 확인 [{:?}] «{}»\n{}\n{}\n",
                candidate.kind,
                candidate.topics.join(", "),
                candidate.title,
                candidate.body
            ),
            QueueItemKind::Deepen { question, .. } => println!("--- 심화\n{question}\n"),
        }
    }
    Ok(())
}
