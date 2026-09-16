//! U1 shared LLM gateway: masking + prompt construction + cloud client + the
//! transfer-transparency log. Every path to the cloud passes through here.
//!
//! - [`masker::RegexMasker`] — strip/restore identifiers (US-2.2, R1).
//! - [`prompts`] — pure prompt builders / response parsers.
//! - [`transfer_log::TransferLog`] — records what left the device (NFR-2).
//! - [`client`] — real Anthropic client under the `llm-http` feature.

pub mod claude_cli;
pub mod claude_discovery;
pub mod client;
pub mod masker;
pub mod prompts;
pub mod transfer_log;

pub use claude_cli::{ClaudeCliConfig, ClaudeCliLlm};
pub use claude_discovery::{discover as discover_claude_installs, ClaudeInstall};
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

/// What [`build_client`] *would* select from the current environment.
///
/// This is a prediction, not a report: it reads the same environment the build
/// reads, but it cannot see a build that fell back (missing key, feature off).
/// Use it only where no client exists yet — before the vault is unlocked, or in
/// the startup banner. Once a client is live, ask it:
/// [`LlmClient::backend_label`](crate::core::traits::LlmClient::backend_label).
pub fn active_model_label() -> String {
    model_label(&selected_provider())
}

/// Extra HTTP headers every outbound LLM request should carry, parsed from
/// `LLM_EXTRA_HEADERS` (one `Name: Value` per line).
///
/// Some gateways need a header the app has no field for — an Azure-style
/// `api-key`, a corporate gateway's routing header, OpenRouter's `X-Title`.
/// Malformed lines are dropped here rather than failing the build: the value
/// is validated up front by [`parse_extra_headers`] when it is saved, so
/// anything wrong at this point came from a hand-edited environment.
pub fn extra_headers_from_env() -> Vec<(String, String)> {
    std::env::var("LLM_EXTRA_HEADERS")
        .ok()
        .map(|raw| parse_extra_headers(&raw).unwrap_or_default())
        .unwrap_or_default()
}

/// Parse a `Name: Value` header block (one per line).
///
/// Blank lines and `#` comments are skipped. Errors name the offending line so
/// the settings screen can say what to fix instead of failing at call time
/// with a transport error.
pub fn parse_extra_headers(raw: &str) -> crate::core::error::Result<Vec<(String, String)>> {
    use crate::core::error::AppError;

    let mut out = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = i + 1;
        let Some((name, value)) = line.split_once(':') else {
            return Err(AppError::InvalidInput(format!(
                "{lineno}번째 줄: `이름: 값` 형식이 아닙니다 — {line:?}"
            )));
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "{lineno}번째 줄: 헤더 이름이 비어 있습니다"
            )));
        }
        // An HTTP field name is a token: letters, digits and a few symbols.
        // Reject anything else here, where the message can point at the line,
        // rather than in the transport where it surfaces as a build failure.
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.!#$%&'*+^`|~".contains(c))
        {
            return Err(AppError::InvalidInput(format!(
                "{lineno}번째 줄: 헤더 이름에 쓸 수 없는 문자가 있습니다 — {name:?}"
            )));
        }
        if value.is_empty() {
            return Err(AppError::InvalidInput(format!(
                "{lineno}번째 줄: `{name}` 의 값이 비어 있습니다"
            )));
        }
        // Header values are latin-1 on the wire; a value pasted through a
        // Korean IME dies inside the transport with an opaque error.
        if !value.is_ascii() {
            return Err(AppError::InvalidInput(format!(
                "{lineno}번째 줄: `{name}` 값에 ASCII가 아닌 문자가 있습니다 — 값을 다시 복사해 붙여넣으세요"
            )));
        }
        out.push((name.to_string(), value.to_string()));
    }
    Ok(out)
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
            // Mirror `OpenAiConfig::from_env` exactly: a key is mandatory only
            // for api.openai.com, and a local server (LM Studio, Ollama) runs
            // without one — so the label must not report "키 미설정" for a
            // backend that is live.
            "openai" => match client::OpenAiConfig::from_env() {
                Ok(cfg) => format!("{} ({})", cfg.model, openai_gateway_name(&cfg.base_url)),
                Err(_) => "오프라인 (키 미설정)".to_string(),
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

/// A short name for where an OpenAI-compatible request actually goes, so the
/// settings screen names the request path and not just the model: OpenAI
/// itself, OpenRouter, or the host:port of a local/self-hosted server.
#[cfg(feature = "llm-http")]
pub(crate) fn openai_gateway_name(base_url: &str) -> String {
    let lower = base_url.to_ascii_lowercase();
    if lower.contains("api.openai.com") {
        return "OpenAI".to_string();
    }
    if lower.contains("openrouter") {
        return "OpenRouter".to_string();
    }
    // Strip the scheme and any path so "http://localhost:1234/v1" reads as
    // "localhost:1234" — the thing the owner typed into LM Studio.
    let host = base_url
        .trim()
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or(base_url);
    if host.is_empty() {
        "OpenAI 호환".to_string()
    } else {
        host.to_string()
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
    let client = build_client_inner(transfer_log);
    // One line per build, naming what the calls will really hit. When this
    // disagrees with what the owner picked, the reason is on the line above
    // (a config that could not be used).
    eprintln!("[llm] client built: {}", client.backend_label());
    client
}

fn build_client_inner(transfer_log: Arc<TransferLog>) -> Arc<dyn LlmClient> {
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

#[cfg(all(test, feature = "llm-http"))]
mod gateway_name_tests {
    use super::openai_gateway_name;

    #[test]
    fn names_the_place_a_request_goes() {
        assert_eq!(openai_gateway_name("https://api.openai.com"), "OpenAI");
        assert_eq!(
            openai_gateway_name("https://openrouter.ai/api"),
            "OpenRouter"
        );
        // A local server is named by what the owner typed: its host and port.
        assert_eq!(
            openai_gateway_name("http://localhost:1234"),
            "localhost:1234"
        );
        assert_eq!(
            openai_gateway_name("http://172.18.144.1:1234/v1"),
            "172.18.144.1:1234"
        );
        assert_eq!(openai_gateway_name(""), "OpenAI 호환");
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

#[cfg(test)]
mod extra_header_tests {
    use super::parse_extra_headers;

    #[test]
    fn parses_a_name_value_block() {
        let raw = "  api-key: abc123  \n\n# 주석은 무시\nX-Title: knows-me\n";
        let got = parse_extra_headers(raw).unwrap();
        assert_eq!(
            got,
            vec![
                ("api-key".to_string(), "abc123".to_string()),
                ("X-Title".to_string(), "knows-me".to_string()),
            ]
        );
    }

    #[test]
    fn a_value_may_contain_colons() {
        // Bearer tokens and URLs both carry colons; only the first splits.
        let got = parse_extra_headers("Authorization: Bearer a:b:c").unwrap();
        assert_eq!(got, vec![("Authorization".into(), "Bearer a:b:c".into())]);
    }

    #[test]
    fn empty_input_is_no_headers_not_an_error() {
        assert!(parse_extra_headers("").unwrap().is_empty());
        assert!(parse_extra_headers("\n  \n# only a comment\n")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_bad_line_says_which_line_and_why() {
        // Every message names the line number, because the field is a block and
        // "형식이 잘못됨" alone leaves the owner rereading all of it.
        let err = parse_extra_headers("api-key abc").unwrap_err().to_string();
        assert!(err.contains("1번째 줄"), "{err}");

        let err = parse_extra_headers("ok: 1\n: novalue")
            .unwrap_err()
            .to_string();
        assert!(err.contains("2번째 줄") && err.contains("이름"), "{err}");

        let err = parse_extra_headers("X-Empty:   ").unwrap_err().to_string();
        assert!(err.contains("값이 비어"), "{err}");

        let err = parse_extra_headers("바보: 1").unwrap_err().to_string();
        assert!(err.contains("쓸 수 없는 문자"), "{err}");

        // A value pasted through an IME would die inside the transport.
        let err = parse_extra_headers("X-Token: 값").unwrap_err().to_string();
        assert!(err.contains("ASCII"), "{err}");
    }
}

#[cfg(test)]
mod backend_label_tests {
    use crate::core::traits::LlmClient;

    #[test]
    fn a_canned_fallback_says_so_instead_of_naming_a_model() {
        // The point of the label: a client that is not talking to anything must
        // not be describable as the model the owner chose.
        let label = crate::mocks::CannedLlm.backend_label();
        assert!(label.contains("오프라인"), "{label}");
        assert!(!label.contains("gpt"), "{label}");
    }

    #[test]
    fn the_cli_client_reports_its_model() {
        let llm = super::ClaudeCliLlm::new(
            super::ClaudeCliConfig {
                model: "claude-haiku-4-5".into(),
                ..Default::default()
            },
            std::sync::Arc::new(super::TransferLog::new()),
        );
        assert_eq!(llm.backend_label(), "claude-haiku-4-5 (로컬 Claude Code)");
    }

    #[cfg(feature = "llm-http")]
    #[test]
    fn the_openai_client_reports_model_and_gateway() {
        use super::client::{OpenAiConfig, OpenAiLlm};
        let llm = OpenAiLlm::new(
            OpenAiConfig {
                api_key: String::new(),
                model: "google/gemma-4-12b".into(),
                base_url: "http://172.18.144.1:1234".into(),
                extra_headers: Vec::new(),
            },
            std::sync::Arc::new(super::TransferLog::new()),
        );
        assert_eq!(
            llm.backend_label(),
            "google/gemma-4-12b (172.18.144.1:1234)"
        );
    }
}
