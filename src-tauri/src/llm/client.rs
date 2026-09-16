//! Cloud LLM client (`LlmClient`) — the single egress point for user data.
//!
//! The real implementation ([`AnthropicLlm`]) talks to the Anthropic Messages
//! API over HTTPS and is compiled only under the `llm-http` feature, so the core
//! library builds and tests fully offline. Without the feature, units use the
//! canned client from [`crate::mocks::CannedLlm`].
//!
//! Callers MUST pass already-masked text (US-2.2); every call is recorded to the
//! [`TransferLog`](crate::llm::transfer_log::TransferLog) for transparency (NFR-2).

/// Token budget for one LLM call — an upper bound on the reply, not a spend.
///
/// Both the classification and the extraction call ask for this. The answers
/// themselves are small (a handful of labels; a title plus a few sentences),
/// so the whole budget exists for **reasoning models**, which emit an internal
/// reasoning block *before* the answer and are billed the budget in that order.
/// When the budget runs out mid-reasoning the answer never arrives: the call
/// comes back truncated, or with `content: null`.
///
/// Measured against a local LM Studio `google/gemma-4-12b` with thinking on:
/// reasoning alone used 1021 of a 1024 budget, and later exhausted 4096 on a
/// classification. 16384 is headroom for that class of model. Unused headroom
/// costs nothing — the provider bills the tokens actually produced.
///
/// **Lower it when the model rejects the request.** Some providers 400 when
/// `max_tokens` exceeds the model's own maximum output (a model capped at
/// 8192 will not accept 16384), so a site whose gateway serves such a model
/// sets [`MAX_TOKENS_ENV`] rather than rebuilding.
#[cfg(feature = "llm-http")]
const DEFAULT_MAX_TOKENS: u32 = 16_384;

/// Environment override for [`DEFAULT_MAX_TOKENS`], for a model whose maximum
/// output is smaller than the default.
#[cfg(feature = "llm-http")]
const MAX_TOKENS_ENV: &str = "LLM_MAX_TOKENS";

/// The per-call token budget: [`DEFAULT_MAX_TOKENS`], or `LLM_MAX_TOKENS` when
/// it parses to a non-zero number. A malformed value is ignored rather than
/// failing the call — an unusable budget would stop every extraction, and the
/// default is always a workable answer.
#[cfg(feature = "llm-http")]
fn max_tokens() -> u32 {
    std::env::var(MAX_TOKENS_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_TOKENS)
}

/// Normalize a configured base URL so both `https://host` and `https://host/v1`
/// work.
///
/// Each client appends its own versioned path (`/v1/messages`,
/// `/v1/chat/completions`). OpenAI-compatible gateways publish their endpoint
/// *with* the `/v1` — OpenRouter documents `https://openrouter.ai/api/v1` — so
/// pasting the documented URL produced `.../v1/v1/chat/completions` and a bare
/// 404 with nothing pointing at the cause.
#[cfg(feature = "llm-http")]
fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

#[cfg(feature = "llm-http")]
pub use http_impl::{AnthropicLlm, LlmConfig};

#[cfg(feature = "llm-http")]
pub use openai_impl::{OpenAiConfig, OpenAiLlm};

#[cfg(feature = "llm-http")]
mod http_impl {
    use std::sync::Arc;

    use async_trait::async_trait;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use serde_json::{json, Value};

    use crate::core::error::{AppError, Result};
    use crate::core::traits::LlmClient;
    use crate::core::types::MaskedText;
    use crate::llm::prompts;
    use crate::llm::transfer_log::TransferLog;

    use super::{max_tokens, normalize_base_url};

    const API_VERSION: &str = "2023-06-01";
    const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
    const DEFAULT_MODEL: &str = "claude-opus-5";

    /// Connection/config for the Anthropic Messages API.
    #[derive(Clone)]
    pub struct LlmConfig {
        pub api_key: String,
        pub model: String,
        pub base_url: String,
        /// Extra headers every request carries (`LLM_EXTRA_HEADERS`). Applied
        /// last, so a gateway that needs its own auth header can override the
        /// one built in.
        pub extra_headers: Vec<(String, String)>,
    }

    impl LlmConfig {
        /// Build from the environment: `ANTHROPIC_API_KEY` (required) and
        /// optional `ANTHROPIC_MODEL` / `ANTHROPIC_BASE_URL`.
        pub fn from_env() -> Result<Self> {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| AppError::External("ANTHROPIC_API_KEY not set".into()))?;
            Ok(Self {
                api_key,
                model: std::env::var("ANTHROPIC_MODEL")
                    .unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
                base_url: normalize_base_url(
                    &std::env::var("ANTHROPIC_BASE_URL")
                        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
                ),
                extra_headers: crate::llm::extra_headers_from_env(),
            })
        }
    }

    pub struct AnthropicLlm {
        client: reqwest::Client,
        config: LlmConfig,
        transfer_log: Arc<TransferLog>,
    }

    impl AnthropicLlm {
        pub fn new(config: LlmConfig, transfer_log: Arc<TransferLog>) -> Self {
            Self {
                client: reqwest::Client::new(),
                config,
                transfer_log,
            }
        }

        /// POST a single-user-message request and return the concatenated text.
        async fn call(&self, system: &str, content: Value, max_tokens: u32) -> Result<String> {
            let body = json!({
                "model": self.config.model,
                "max_tokens": max_tokens,
                "system": system,
                "messages": [{ "role": "user", "content": content }],
            });

            let mut req = self
                .client
                .post(format!("{}/v1/messages", self.config.base_url))
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json");
            for (name, value) in &self.config.extra_headers {
                req = req.header(name.as_str(), value.as_str());
            }
            let resp = req
                .json(&body)
                .send()
                .await
                .map_err(|e| AppError::External(format!("LLM request failed: {e}")))?;

            let status = resp.status();
            let payload: Value = resp
                .json()
                .await
                .map_err(|e| AppError::External(format!("LLM response decode failed: {e}")))?;

            if !status.is_success() {
                let msg = payload["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error");
                return Err(AppError::External(format!("LLM API {status}: {msg}")));
            }

            let text: String = payload["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("");
            Ok(text)
        }
    }

    #[async_trait]
    impl LlmClient for AnthropicLlm {
        fn backend_label(&self) -> String {
            format!("{} (Anthropic)", self.config.model)
        }

        async fn summarize(&self, input: &MaskedText) -> Result<String> {
            self.transfer_log
                .record_text("summarize", &self.config.model, input);
            self.call(prompts::SUMMARIZE_SYSTEM, json!(input.text), max_tokens())
                .await
        }

        async fn classify(&self, input: &MaskedText) -> Result<Vec<String>> {
            self.transfer_log
                .record_text("classify", &self.config.model, input);
            let raw = self
                // Classification output is a few short labels, but reasoning
                // models spend the budget on internal reasoning before emitting
                // any of them — 128 tokens leaves nothing for the answer.
                .call(prompts::CLASSIFY_SYSTEM, json!(input.text), max_tokens())
                .await?;
            Ok(prompts::parse_labels(&raw))
        }

        async fn vision_extract(&self, image_png: &[u8]) -> Result<MaskedText> {
            self.transfer_log
                .record_image(&self.config.model, image_png.len());
            let content = json!([
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": B64.encode(image_png),
                    }
                },
                { "type": "text", "text": "Extract the text and visual context from this image as plain text." }
            ]);
            let text = self
                .call(
                    "You extract text and visual context from images as plain text.",
                    content,
                    2048,
                )
                .await?;
            // NOTE: vision output is not yet masked; the Processing unit (U2)
            // must mask it before any downstream text call or storage.
            Ok(MaskedText { text })
        }

        async fn chat(&self, system: &str, input: &MaskedText) -> Result<String> {
            self.transfer_log
                .record_text("chat", &self.config.model, input);
            self.call(system, json!(input.text), 4096).await
        }
    }
}

/// OpenAI-compatible client (`POST /v1/chat/completions`, `Authorization: Bearer`).
///
/// Talks the OpenAI Chat Completions schema, so it also serves any gateway that
/// mirrors it (Azure OpenAI, OpenRouter, and OpenAI-compatible local servers) by
/// pointing `OPENAI_BASE_URL` at them. Same masking/transfer-log contract as the
/// Anthropic client: callers pass already-masked text and every call is logged.
#[cfg(feature = "llm-http")]
pub(crate) mod openai_impl {
    use std::sync::Arc;

    use async_trait::async_trait;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use serde_json::{json, Value};

    use crate::core::error::{AppError, Result};
    use crate::core::traits::LlmClient;
    use crate::core::types::MaskedText;
    use crate::llm::prompts;
    use crate::llm::transfer_log::TransferLog;

    use super::{max_tokens, normalize_base_url};

    const DEFAULT_BASE_URL: &str = "https://api.openai.com";
    const DEFAULT_MODEL: &str = "gpt-4o";

    /// Connection/config for the OpenAI Chat Completions API.
    #[derive(Clone)]
    pub struct OpenAiConfig {
        /// Bearer token. Empty means "send no `Authorization` header" — local
        /// OpenAI-compatible servers (LM Studio, Ollama, vLLM, …) run without
        /// one, and the app must not demand a key that does not exist.
        pub api_key: String,
        pub model: String,
        pub base_url: String,
        /// Extra headers every request carries (`LLM_EXTRA_HEADERS`). Applied
        /// after the built-in ones, so a gateway that authenticates with its
        /// own header (Azure's `api-key`, a corporate routing header) can
        /// replace the `Authorization` this client would otherwise send.
        pub extra_headers: Vec<(String, String)>,
    }

    impl OpenAiConfig {
        /// Build from the environment: `OPENAI_MODEL`, `OPENAI_BASE_URL`, and
        /// `OPENAI_API_KEY`.
        ///
        /// The key is required only when talking to the default host
        /// (`api.openai.com`), which rejects unauthenticated calls anyway. Any
        /// other base URL — a local LM Studio/Ollama server, a self-hosted
        /// gateway — may leave it unset; the request then carries no
        /// `Authorization` header.
        pub fn from_env() -> Result<Self> {
            let base_url = normalize_base_url(
                &std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
            );
            let api_key = std::env::var("OPENAI_API_KEY")
                .map(|k| k.trim().to_string())
                .unwrap_or_default();
            if api_key.is_empty() && !Self::allows_missing_key(&base_url) {
                return Err(AppError::External(
                    "OPENAI_API_KEY not set (required for api.openai.com; a local \
                     OpenAI-compatible server may leave it blank)"
                        .into(),
                ));
            }
            Ok(Self {
                api_key,
                model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
                base_url,
                extra_headers: crate::llm::extra_headers_from_env(),
            })
        }

        /// Whether `base_url` points somewhere that can run without a key:
        /// anything other than OpenAI's own host.
        pub(crate) fn allows_missing_key(base_url: &str) -> bool {
            normalize_base_url(base_url) != DEFAULT_BASE_URL
        }
    }

    pub struct OpenAiLlm {
        client: reqwest::Client,
        config: OpenAiConfig,
        transfer_log: Arc<TransferLog>,
    }

    impl OpenAiLlm {
        pub fn new(config: OpenAiConfig, transfer_log: Arc<TransferLog>) -> Self {
            Self {
                client: reqwest::Client::new(),
                config,
                transfer_log,
            }
        }

        /// POST a chat-completions request (system + one user message) and return
        /// the assistant text. `content` is the user message content (string or
        /// the OpenAI content-parts array for vision).
        async fn call(&self, system: &str, content: Value, max_tokens: u32) -> Result<String> {
            // Reasoning-era models (o1/o3/o4, gpt-5) reject the legacy
            // `max_tokens` field and require `max_completion_tokens`; chat models
            // (gpt-4o, …) still take `max_tokens`. Pick the key by model family so
            // pointing OPENAI_MODEL at a newer model doesn't 400.
            let token_key = if uses_completion_tokens(&self.config.model) {
                "max_completion_tokens"
            } else {
                "max_tokens"
            };
            let mut body = json!({
                "model": self.config.model,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": content },
                ],
            });
            body[token_key] = json!(max_tokens);

            let mut req = self
                .client
                .post(format!("{}/v1/chat/completions", self.config.base_url))
                .header("content-type", "application/json");
            // Keyless local servers get no Authorization header at all; some
            // (LM Studio) accept any bearer, others reject a malformed one.
            if !self.config.api_key.is_empty() {
                req = req.header("authorization", format!("Bearer {}", self.config.api_key));
            }
            for (name, value) in &self.config.extra_headers {
                req = req.header(name.as_str(), value.as_str());
            }
            let resp = req
                .json(&body)
                .send()
                .await
                .map_err(|e| AppError::External(format!("LLM request failed: {e}")))?;

            let status = resp.status();
            let payload: Value = resp
                .json()
                .await
                .map_err(|e| AppError::External(format!("LLM response decode failed: {e}")))?;

            if !status.is_success() {
                let msg = payload["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error");
                return Err(AppError::External(format!("LLM API {status}: {msg}")));
            }

            // A response cut off at the budget is a partial answer. Passing it
            // upward stores half a sentence as a fact; failing here parks the
            // item in the pending queue, where it is retried instead of lost.
            if Self::was_truncated(&payload) {
                return Err(AppError::External(format!(
                    "LLM response hit the {max_tokens}-token limit and was cut off (model {})",
                    self.config.model
                )));
            }

            let text = Self::extract_text(&payload);
            if text.trim().is_empty() {
                // A 200 with empty content is not a usable answer. Reasoning
                // models (Gemini Flash, o-series) spend the token budget on
                // internal reasoning first and return `content: null` when it
                // runs out — silently passing "" upward makes classification
                // yield no labels and every item look uncertain.
                return Err(AppError::External(format!(
                    "LLM returned an empty completion (model {}; the token budget may have been \
                     consumed by reasoning — raise max_tokens)",
                    self.config.model
                )));
            }
            Ok(text)
        }

        /// Whether generation stopped because the budget ran out.
        pub(super) fn was_truncated(payload: &Value) -> bool {
            payload["choices"]
                .as_array()
                .and_then(|c| c.first())
                .and_then(|c| c["finish_reason"].as_str())
                .is_some_and(|r| r == "length")
        }

        /// Pull `choices[0].message.content` out of a chat-completions response.
        /// Split out so tests can pin the parse without a network round-trip.
        fn extract_text(payload: &Value) -> String {
            payload["choices"]
                .as_array()
                .and_then(|c| c.first())
                .and_then(|c| c["message"]["content"].as_str())
                .unwrap_or("")
                .to_string()
        }
    }

    /// Whether a model belongs to the reasoning families that require
    /// `max_completion_tokens` instead of the legacy `max_tokens`.
    fn uses_completion_tokens(model: &str) -> bool {
        let m = model.to_ascii_lowercase();
        m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("gpt-5")
    }

    #[async_trait]
    impl LlmClient for OpenAiLlm {
        fn backend_label(&self) -> String {
            format!(
                "{} ({})",
                self.config.model,
                crate::llm::openai_gateway_name(&self.config.base_url)
            )
        }

        async fn summarize(&self, input: &MaskedText) -> Result<String> {
            self.transfer_log
                .record_text("summarize", &self.config.model, input);
            self.call(prompts::SUMMARIZE_SYSTEM, json!(input.text), max_tokens())
                .await
        }

        async fn classify(&self, input: &MaskedText) -> Result<Vec<String>> {
            self.transfer_log
                .record_text("classify", &self.config.model, input);
            let raw = self
                // Classification output is a few short labels, but reasoning
                // models spend the budget on internal reasoning before emitting
                // any of them — 128 tokens leaves nothing for the answer.
                .call(prompts::CLASSIFY_SYSTEM, json!(input.text), max_tokens())
                .await?;
            Ok(prompts::parse_labels(&raw))
        }

        async fn vision_extract(&self, image_png: &[u8]) -> Result<MaskedText> {
            self.transfer_log
                .record_image(&self.config.model, image_png.len());
            let data_url = format!("data:image/png;base64,{}", B64.encode(image_png));
            let content = json!([
                { "type": "text", "text": "Extract the text and visual context from this image as plain text." },
                { "type": "image_url", "image_url": { "url": data_url } },
            ]);
            let text = self
                .call(
                    "You extract text and visual context from images as plain text.",
                    content,
                    2048,
                )
                .await?;
            // NOTE: vision output is not yet masked; the Processing unit (U2)
            // must mask it before any downstream text call or storage.
            Ok(MaskedText { text })
        }

        async fn chat(&self, system: &str, input: &MaskedText) -> Result<String> {
            self.transfer_log
                .record_text("chat", &self.config.model, input);
            self.call(system, json!(input.text), 4096).await
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn extract_text_pulls_first_choice_message() {
            let payload = json!({
                "choices": [
                    { "message": { "role": "assistant", "content": "hello there" } }
                ]
            });
            assert_eq!(OpenAiLlm::extract_text(&payload), "hello there");
        }

        #[test]
        fn extract_text_is_empty_on_malformed_payload() {
            assert_eq!(OpenAiLlm::extract_text(&json!({})), "");
            assert_eq!(OpenAiLlm::extract_text(&json!({ "choices": [] })), "");
            assert_eq!(
                OpenAiLlm::extract_text(&json!({ "choices": [ { "message": {} } ] })),
                ""
            );
        }

        #[test]
        fn a_key_is_only_mandatory_for_openai_itself() {
            // LM Studio / Ollama / a self-hosted gateway run without a key.
            assert!(OpenAiConfig::allows_missing_key("http://localhost:1234/v1"));
            assert!(OpenAiConfig::allows_missing_key("http://172.18.144.1:1234"));
            assert!(OpenAiConfig::allows_missing_key(
                "https://openrouter.ai/api/v1"
            ));
            // api.openai.com rejects unauthenticated calls, so demand one up front.
            assert!(!OpenAiConfig::allows_missing_key("https://api.openai.com"));
            assert!(!OpenAiConfig::allows_missing_key(
                "https://api.openai.com/v1/"
            ));
        }

        #[test]
        fn budget_defaults_and_can_be_lowered_for_a_smaller_model() {
            // The default is the reasoning-model headroom. The override exists
            // for a gateway whose model 400s on a budget above its own maximum
            // output, so it must actually take effect — and a junk value must
            // fall back rather than stop every extraction.
            use super::super::{max_tokens, DEFAULT_MAX_TOKENS, MAX_TOKENS_ENV};

            // This test mutates process-wide state, so it restores what it found.
            let prior = std::env::var(MAX_TOKENS_ENV).ok();
            std::env::remove_var(MAX_TOKENS_ENV);
            assert_eq!(max_tokens(), DEFAULT_MAX_TOKENS);
            assert_eq!(DEFAULT_MAX_TOKENS, 16_384);

            std::env::set_var(MAX_TOKENS_ENV, "8192");
            assert_eq!(max_tokens(), 8192);
            std::env::set_var(MAX_TOKENS_ENV, "  4096  ");
            assert_eq!(max_tokens(), 4096, "값 주변 공백은 무시한다");

            for junk in ["", "0", "많이", "-1", "8192tokens"] {
                std::env::set_var(MAX_TOKENS_ENV, junk);
                assert_eq!(
                    max_tokens(),
                    DEFAULT_MAX_TOKENS,
                    "쓸 수 없는 값 {junk:?}은 기본값으로 되돌아가야 한다"
                );
            }

            match prior {
                Some(v) => std::env::set_var(MAX_TOKENS_ENV, v),
                None => std::env::remove_var(MAX_TOKENS_ENV),
            }
        }

        #[test]
        fn token_key_matches_model_family() {
            // Chat models keep the legacy field.
            assert!(!uses_completion_tokens("gpt-4o"));
            assert!(!uses_completion_tokens("gpt-4o-mini"));
            assert!(!uses_completion_tokens("gpt-4-turbo"));
            // Reasoning-era models require max_completion_tokens.
            assert!(uses_completion_tokens("o1"));
            assert!(uses_completion_tokens("o1-mini"));
            assert!(uses_completion_tokens("o3-mini"));
            assert!(uses_completion_tokens("o4-mini"));
            assert!(uses_completion_tokens("gpt-5"));
            assert!(uses_completion_tokens("GPT-5-mini")); // case-insensitive
        }
    }
}

#[cfg(all(test, feature = "llm-http"))]
mod truncation_tests {
    use super::openai_impl::OpenAiLlm;
    use serde_json::json;

    #[test]
    fn a_response_cut_at_the_budget_is_not_a_complete_answer() {
        // Storing a half-sentence as a fact is worse than failing and retrying.
        let cut = json!({"choices":[{"finish_reason":"length","message":{"content":"이를 위해 데이터"}}]});
        assert!(OpenAiLlm::was_truncated(&cut));

        let whole =
            json!({"choices":[{"finish_reason":"stop","message":{"content":"완결된 문장."}}]});
        assert!(!OpenAiLlm::was_truncated(&whole));

        // A provider that omits the field must not be read as truncated.
        let silent = json!({"choices":[{"message":{"content":"본문"}}]});
        assert!(!OpenAiLlm::was_truncated(&silent));
    }
}

#[cfg(all(test, feature = "llm-http"))]
mod normalize_tests {
    use super::normalize_base_url;

    #[test]
    fn accepts_both_documented_forms_of_a_gateway_url() {
        // OpenRouter documents the /v1 form; OpenAI documents the bare host.
        assert_eq!(
            normalize_base_url("https://openrouter.ai/api/v1"),
            "https://openrouter.ai/api"
        );
        assert_eq!(
            normalize_base_url("https://openrouter.ai/api/v1/"),
            "https://openrouter.ai/api"
        );
        assert_eq!(
            normalize_base_url("https://api.openai.com"),
            "https://api.openai.com"
        );
        assert_eq!(
            normalize_base_url("  https://api.openai.com/  "),
            "https://api.openai.com"
        );
    }

    #[test]
    fn does_not_strip_a_v1_that_is_part_of_a_host_or_path_segment() {
        assert_eq!(
            normalize_base_url("https://gw.example.com/openai-v1"),
            "https://gw.example.com/openai-v1"
        );
    }
}
