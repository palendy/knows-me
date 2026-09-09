//! Print the knowledge base as kinds, topics and topic pages, so the shape of
//! what was extracted can be judged rather than inferred.

use std::sync::Arc;

use knows_me_core::core::traits::{EncryptedStore, KeyManager, KnowledgeApi};
use knows_me_core::core::types::FactFilter;
use knows_me_core::knowledge::KnowledgeService;
use knows_me_core::security::{FileEncryptedStore, PasswordKeyManager};

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
    let svc = Arc::new(KnowledgeService::new(store));
    svc.build_index().await.ok();
    let kn: Arc<dyn KnowledgeApi> = svc;

    let summaries = kn.search(String::new(), FactFilter::default()).await?;
    println!("사실 {}개\n", summaries.len());
    for s in &summaries {
        println!(
            "  [{:>10}] {:60} «{}»",
            format!("{:?}", s.kind),
            s.title.chars().take(58).collect::<String>(),
            s.topics.join(", ")
        );
    }

    println!("\n=== 주제 페이지 (반복 많은 순) ===");
    for p in kn.topics(FactFilter::default()).await? {
        println!(
            "  {:24} 언급 {:2} · 걸림 {:2} · 최근 {}",
            p.topic,
            p.mentions,
            p.concerns,
            p.last_seen
                .map(|d| d.format("%m-%d").to_string())
                .unwrap_or_else(|| "-".into())
        );
    }
    Ok(())
}
