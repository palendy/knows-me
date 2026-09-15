//! Smoke-test the configured LLM backend end to end — summarize, classify,
//! chat — without a vault or a GUI. Use it to confirm an LLM URL (LM Studio,
//! Ollama, OpenRouter, …) actually answers before opening the app.
//!
//! ```bash
//! # Reads the same .env the app reads.
//! cargo run --features llm-http --example llm_probe
//!
//! # Or override inline:
//! LLM_PROVIDER=openai OPENAI_BASE_URL=http://localhost:1234/v1 OPENAI_MODEL=google/gemma-4-12b \
//!   cargo run --features llm-http --example llm_probe
//! ```

use std::sync::Arc;
use std::time::Instant;

use knows_me_core::core::types::MaskedText;
use knows_me_core::llm::{active_model_label, build_client, TransferLog};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    println!("backend: {}", active_model_label());

    let log = Arc::new(TransferLog::new());
    let llm = build_client(log.clone());

    let sample = MaskedText {
        text: "오늘 세션에서 나는 knows-me 프로젝트의 빌드 문서를 다시 썼다. \
               테스트는 `cargo test`와 `npm test`로 돌리고, 데스크탑 앱은 `npx tauri dev`로 띄운다. \
               LLM은 LM Studio를 URL로 연결해서 쓰기로 했다."
            .to_string(),
    };

    let t = Instant::now();
    let summary = llm.summarize(&sample).await?;
    println!(
        "\n[summarize] ({:.1}s)\n{summary}",
        t.elapsed().as_secs_f32()
    );

    let t = Instant::now();
    let labels = llm.classify(&sample).await?;
    println!(
        "\n[classify] ({:.1}s)\n{labels:?}",
        t.elapsed().as_secs_f32()
    );

    let t = Instant::now();
    let reply = llm
        .chat(
            "You answer in one short Korean sentence.",
            &MaskedText {
                text: "이 프로젝트는 어떻게 띄우나요?".to_string(),
            },
        )
        .await?;
    println!("\n[chat] ({:.1}s)\n{reply}", t.elapsed().as_secs_f32());

    println!("\ntransfer log entries: {}", log.list().len());
    Ok(())
}
