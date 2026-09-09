//! U1 shared LLM gateway: masking + prompt construction + cloud client + the
//! transfer-transparency log. Every path to the cloud passes through here.
//!
//! - [`masker::RegexMasker`] — strip/restore identifiers (US-2.2, R1).
//! - [`prompts`] — pure prompt builders / response parsers.
//! - [`transfer_log::TransferLog`] — records what left the device (NFR-2).
//! - [`client`] — real Anthropic client under the `llm-http` feature.

pub mod client;
pub mod masker;
pub mod prompts;
pub mod transfer_log;

pub use masker::RegexMasker;
pub use transfer_log::TransferLog;

use std::sync::Arc;

use crate::core::traits::LlmClient;

/// A human-readable description of the LLM the app is *actually* configured to
/// use right now, for the settings screen's "current environment" panel.
///
/// This mirrors [`build_client`]'s selection logic so the label never drifts
/// from what really runs: offline (canned) vs. the provider + model resolved
/// from the environment. It does not build a client or hit the network.
pub fn active_model_label() -> String {
    #[cfg(not(feature = "llm-http"))]
    {
        "오프라인 (로컬 응답)".to_string()
    }

    #[cfg(feature = "llm-http")]
    {
        let provider = std::env::var("LLM_PROVIDER")
            .unwrap_or_else(|_| "anthropic".to_string())
            .to_ascii_lowercase();
        match provider.as_str() {
            "openai" => match std::env::var("OPENAI_API_KEY") {
                Ok(k) if !k.trim().is_empty() => {
                    let model =
                        std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());
                    let base = std::env::var("OPENAI_BASE_URL").unwrap_or_default();
                    // Name the gateway when it's OpenRouter so the owner knows
                    // the request path, not just the model.
                    if base.contains("openrouter") {
                        format!("{model} (OpenRouter)")
                    } else {
                        format!("{model} (OpenAI)")
                    }
                }
                _ => "오프라인 (키 미설정)".to_string(),
            },
            _ => match std::env::var("ANTHROPIC_API_KEY") {
                Ok(k) if !k.trim().is_empty() => {
                    let model = std::env::var("ANTHROPIC_MODEL")
                        .unwrap_or_else(|_| "claude-opus-5".to_string());
                    format!("{model} (Anthropic)")
                }
                _ => "오프라인 (키 미설정)".to_string(),
            },
        }
    }
}

/// Build the cloud LLM client used by Interview (U3) and Persona (U4).
///
/// Selection is a compile-time floor plus a runtime knob:
/// - Without the `llm-http` feature the core builds fully offline, so this
///   always returns the deterministic [`CannedLlm`](crate::mocks::CannedLlm).
///   The app still launches and every screen is reachable — cloud calls just
///   return canned strings instead of hitting the network.
/// - With `llm-http`, `LLM_PROVIDER` chooses the backend (`anthropic` default,
///   or `openai`). If the selected provider's API key is missing we fall back to
///   `CannedLlm` rather than failing to start — the vault must still open.
pub fn build_client(transfer_log: Arc<TransferLog>) -> Arc<dyn LlmClient> {
    #[cfg(not(feature = "llm-http"))]
    {
        let _ = transfer_log;
        Arc::new(crate::mocks::CannedLlm)
    }

    #[cfg(feature = "llm-http")]
    {
        let provider = std::env::var("LLM_PROVIDER")
            .unwrap_or_else(|_| "anthropic".to_string())
            .to_ascii_lowercase();
        match provider.as_str() {
            "openai" => match client::OpenAiConfig::from_env() {
                Ok(cfg) => Arc::new(client::OpenAiLlm::new(cfg, transfer_log)),
                Err(e) => {
                    eprintln!("[llm] OpenAI config unavailable ({e}); using offline canned client");
                    Arc::new(crate::mocks::CannedLlm)
                }
            },
            _ => match client::LlmConfig::from_env() {
                Ok(cfg) => Arc::new(client::AnthropicLlm::new(cfg, transfer_log)),
                Err(e) => {
                    eprintln!(
                        "[llm] Anthropic config unavailable ({e}); using offline canned client"
                    );
                    Arc::new(crate::mocks::CannedLlm)
                }
            },
        }
    }
}

/// Human-readable label for the LLM actually in effect, for the settings screen.
///
/// Mirrors [`build_client`]'s selection (same provider + key fallback) so the
/// "AI 모델" line reflects what a cloud call would really hit — not the static
/// `AppConfig::llm_model` default, which is provider-agnostic boilerplate.
pub fn active_model_label() -> String {
    #[cfg(not(feature = "llm-http"))]
    {
        "오프라인 (canned)".to_string()
    }

    #[cfg(feature = "llm-http")]
    {
        let provider = std::env::var("LLM_PROVIDER")
            .unwrap_or_else(|_| "anthropic".to_string())
            .to_ascii_lowercase();
        match provider.as_str() {
            "openai" => match std::env::var("OPENAI_API_KEY") {
                Ok(_) => {
                    let model = std::env::var("OPENAI_MODEL")
                        .unwrap_or_else(|_| "claude-opus-5".to_string());
                    format!("{model} (OpenAI 호환)")
                }
                Err(_) => "오프라인 (canned)".to_string(),
            },
            _ => match std::env::var("ANTHROPIC_API_KEY") {
                Ok(_) => {
                    let model = std::env::var("ANTHROPIC_MODEL")
                        .unwrap_or_else(|_| "claude-opus-5".to_string());
                    format!("{model} (Anthropic)")
                }
                Err(_) => "오프라인 (canned)".to_string(),
            },
        }
    }
}

#[cfg(all(test, not(feature = "llm-http")))]
mod tests {
    use super::active_model_label;

    #[test]
    fn offline_build_reports_offline() {
        // Without llm-http the app never hits the cloud, so the settings label
        // must say so regardless of any environment variables.
        assert_eq!(active_model_label(), "오프라인 (canned)");
    }
}
