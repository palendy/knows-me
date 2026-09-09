//! U1 shared LLM gateway: masking + prompt construction + cloud client + the
//! transfer-transparency log. Every path to the cloud passes through here.
//!
//! - [`masker::RegexMasker`] — strip/restore identifiers (US-2.2, R1).
//! - [`prompts`] — pure prompt builders / response parsers.
//! - [`transfer_log::TransferLog`] — records what left the device (NFR-2).
//! - [`client`] — real Anthropic client under the `llm-http` feature.

pub mod claude_cli;
pub mod client;
pub mod masker;
pub mod prompts;
pub mod transfer_log;

pub use claude_cli::{ClaudeCliConfig, ClaudeCliLlm};
pub use masker::RegexMasker;
pub use transfer_log::TransferLog;

use std::sync::Arc;

use crate::core::traits::LlmClient;

/// The provider `LLM_PROVIDER` selects when it is unset.
const DEFAULT_PROVIDER: &str = "claude-cli";

/// Which backend the environment selects, lowercased.
///
/// [`build_client`] and [`active_model_label`] both go through this so the
/// settings screen cannot describe a different backend than the one running.
fn selected_provider() -> String {
    std::env::var("LLM_PROVIDER")
        .unwrap_or_else(|_| DEFAULT_PROVIDER.to_string())
        .to_ascii_lowercase()
}

/// Whether a provider name means "drive the local Claude Code install".
fn is_claude_cli(provider: &str) -> bool {
    provider == "claude-cli" || provider == "claude"
}

/// A human-readable description of the LLM the app is *actually* configured to
/// use right now, for the settings screen's "current environment" panel.
///
/// This follows [`build_client`]'s selection so the label never drifts from
/// what really runs. It does not build a client or hit the network.
pub fn active_model_label() -> String {
    model_label(&selected_provider())
}

/// The label for one named provider. Split out from [`active_model_label`] so
/// it can be tested per provider without mutating process-wide environment,
/// which would race the other tests in the same binary.
fn model_label(provider: &str) -> String {
    // The CLI backend runs in every build — it needs no feature and no key —
    // so it is resolved before the `llm-http` split, exactly as in
    // `build_client`. Reporting "offline" here would be a plain lie: the
    // default build does reach Anthropic, through the owner's own CLI.
    if is_claude_cli(provider) {
        return format!("{} (로컬 Claude Code)", ClaudeCliConfig::from_env().model);
    }

    #[cfg(not(feature = "llm-http"))]
    {
        "오프라인 (로컬 응답)".to_string()
    }

    #[cfg(feature = "llm-http")]
    {
        match provider {
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
/// `LLM_PROVIDER` chooses the backend. The default is `claude-cli`, which
/// drives the locally installed Claude Code: everyone using this project
/// already has it, so extraction runs on an account they already have rather
/// than a second API subscription.
///
/// The HTTP backends (`anthropic`, `openai`, and OpenAI-compatible gateways
/// such as OpenRouter) still exist and are faster; they need the `llm-http`
/// feature and their own key.
///
/// Selection detail:
/// - Without the `llm-http` feature the core builds fully offline, so this
///   always returns the deterministic [`CannedLlm`](crate::mocks::CannedLlm).
///   The app still launches and every screen is reachable — cloud calls just
///   return canned strings instead of hitting the network.
/// - With `llm-http`, `LLM_PROVIDER` may instead name an HTTP backend
///   (`anthropic` or `openai`). If that provider's API key is missing we fall
///   back to `CannedLlm` rather than failing to start — the vault must still
///   open.
pub fn build_client(transfer_log: Arc<TransferLog>) -> Arc<dyn LlmClient> {
    let provider = selected_provider();

    // The CLI backend needs no HTTP client and no API key — it drives the
    // Claude Code install the owner already has — so it works in the default
    // build, unlike the two HTTP clients behind `llm-http`.
    if is_claude_cli(&provider) {
        return Arc::new(claude_cli::ClaudeCliLlm::new(
            claude_cli::ClaudeCliConfig::from_env(),
            transfer_log,
        ));
    }

    #[cfg(not(feature = "llm-http"))]
    {
        eprintln!(
            "[llm] provider `{provider}` needs the llm-http feature; using offline canned client"
        );
        Arc::new(crate::mocks::CannedLlm)
    }

    #[cfg(feature = "llm-http")]
    {
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

#[cfg(all(test, not(feature = "llm-http")))]
mod tests {
    use super::*;

    #[test]
    fn the_default_build_reports_the_local_cli_not_offline() {
        // The default provider is the local CLI, which runs without the
        // `llm-http` feature. Saying "offline" here would tell the owner
        // nothing leaves the device while their sessions are being sent to
        // Anthropic through their own Claude Code.
        let label = active_model_label();
        assert!(
            label.contains("로컬 Claude Code"),
            "default build must name the CLI backend, got {label:?}"
        );
    }

    #[test]
    fn an_http_provider_without_the_feature_reports_offline() {
        // `llm-http` is off, so `build_client` falls back to the canned client
        // for any HTTP provider — the label has to say the same thing.
        assert_eq!(
            model_label("openai"),
            "오프라인 (로컬 응답)",
            "an unusable HTTP provider must not be advertised as live"
        );
    }

    #[test]
    fn both_spellings_of_the_cli_provider_are_recognized() {
        // `build_client` accepts either; a label that only knew one would
        // report "offline" for a backend that is actually running.
        for provider in ["claude-cli", "claude", "CLAUDE-CLI"] {
            assert!(
                model_label(&provider.to_ascii_lowercase()).contains("로컬 Claude Code"),
                "{provider} must resolve to the CLI backend"
            );
        }
    }
}
