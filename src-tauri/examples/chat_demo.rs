//! Multi-turn persona conversation against the vault built by `ingest_sessions`.
//!
//! ```bash
//! set -a && . ./.env && set +a
//! KNOWSME_DATA_DIR=$PWD/.knowsme-demo \
//!   cargo run --release --features llm-http --example chat_demo
//! ```

use std::sync::Arc;

use knows_me_core::core::traits::{EncryptedStore, KeyManager, KnowledgeApi, Masker, PersonaApi};
use knows_me_core::core::types::{ChatRole, ChatTurn};
use knows_me_core::knowledge::KnowledgeService;
use knows_me_core::llm::RegexMasker;
use knows_me_core::persona::PersonaService;
use knows_me_core::security::{FileEncryptedStore, PasswordKeyManager};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = std::env::var("KNOWSME_DATA_DIR").unwrap_or_else(|_| ".knowsme-demo".into());
    let password =
        std::env::var("KNOWSME_DEMO_PASSWORD").unwrap_or_else(|_| "demo-password".into());

    let keys = Arc::new(PasswordKeyManager::new(&data_dir));
    keys.unlock(&password)?;
    let store: Arc<dyn EncryptedStore> = Arc::new(FileEncryptedStore::new(
        std::path::Path::new(&data_dir).join("store"),
        keys,
    ));

    let knowledge_svc = Arc::new(KnowledgeService::new(store));
    knowledge_svc.build_index().await.ok();
    let knowledge: Arc<dyn KnowledgeApi> = knowledge_svc;
    let masker: Arc<dyn Masker> = Arc::new(RegexMasker::new());
    let llm = knows_me_core::llm::build_client(Arc::new(knows_me_core::llm::TransferLog::new()));
    let persona = PersonaService::new(knowledge, masker, llm);

    let turns = [
        "내가 요즘 뭘 만들고 있지?",
        "그거 어떻게 진행하고 있어?",
        "내가 일할 때 지키는 원칙이 뭐야?",
    ];

    let mut history: Vec<ChatTurn> = Vec::new();
    for q in turns {
        println!("\n나 › {q}");
        let reply = persona.chat(q.to_string(), history.clone()).await?;
        println!("페르소나 › {}", reply.text);
        if reply.sources.is_empty() {
            println!("   (근거 없음)");
        } else {
            let titles: Vec<&str> = reply.sources.iter().map(|s| s.title.as_str()).collect();
            println!("   근거 {}개: {}", titles.len(), titles.join(" / "));
        }
        history.push(ChatTurn {
            role: ChatRole::Owner,
            text: q.to_string(),
        });
        history.push(ChatTurn {
            role: ChatRole::Persona,
            text: reply.text,
        });
    }
    Ok(())
}
