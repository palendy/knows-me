//! Dump every intermediate value for a few real transcripts.
//!
//! ```bash
//! set -a && . ./.env && set +a
//! cargo run --release --features llm-http --example debug_pipeline
//! ```

use std::sync::Arc;

use knows_me_core::core::traits::{Connector, Masker};
use knows_me_core::core::types::Cursor;
use knows_me_core::ingestion::connectors::SessionConnector;
use knows_me_core::llm::{prompts, RegexMasker};

fn clip(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let n: usize = std::env::var("DEBUG_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let connector = SessionConnector::from_config(None);
    let (items, _next) = connector
        .sync(None::<Cursor>, &knows_me_core::core::traits::NoProgress)
        .await?;
    println!("커넥터가 돌려준 항목: {}개\n", items.len());

    let masker = RegexMasker::new();
    let llm = knows_me_core::llm::build_client(Arc::new(knows_me_core::llm::TransferLog::new()));

    for (i, raw) in items.iter().take(n).enumerate() {
        let text = raw.text.clone().unwrap_or_default();
        println!(
            "═══════════════ [{}] {} ═══════════════",
            i + 1,
            raw.external_id
        );
        println!("추출 길이: {} chars", text.chars().count());
        println!("--- 앞 300자 ---\n{}", clip(&text, 300));
        println!(
            "--- 뒤 300자 ---\n{}",
            clip(&text.chars().rev().collect::<String>(), 300)
                .chars()
                .rev()
                .collect::<String>()
        );

        let (masked, _map) = masker.mask(&text);
        println!("\n마스킹 후 길이: {} chars", masked.text.chars().count());

        let summary = llm.summarize(&masked).await?;
        println!("\n▶ SUMMARIZE 결과:\n{summary}");

        let labels = llm.classify(&masked).await?;
        println!("\n▶ CLASSIFY 결과 (raw labels): {labels:?}");

        let decision = knows_me_core::processing::route(&labels, &summary, raw);
        println!(
            "\n▶ ROUTE 결정: {}",
            match &decision {
                knows_me_core::processing::ProcessingDecision::Store(c) =>
                    format!("Store  scope={:?}  title={}", c.suggested_scope, c.title),
                knows_me_core::processing::ProcessingDecision::Confirm(c) =>
                    format!("Confirm  scope={:?}  title={}", c.suggested_scope, c.title),
                knows_me_core::processing::ProcessingDecision::Deepen { question, .. } =>
                    format!("Deepen  q={question}"),
                knows_me_core::processing::ProcessingDecision::Drop { reason } =>
                    format!("Drop  ({reason})"),
            }
        );
        println!();
    }

    println!("=== 라우터가 인식하는 제어 라벨 ===");
    println!("noise / one-off / needs-context / uncertain / company / personal");
    println!("=== CLASSIFY 프롬프트가 요구하는 것 ===");
    println!("{}", prompts::CLASSIFY_SYSTEM);
    Ok(())
}
