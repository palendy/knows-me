//! Cloud LLM client (`LlmClient`) — the single egress point for user data.
//!
//! The real implementation ([`AnthropicLlm`]) talks to the Anthropic Messages
//! API over HTTPS and is compiled only under the `llm-http` feature, so the core
//! library builds and tests fully offline. Without the feature, units use the
//! canned client from [`crate::mocks::CannedLlm`].
//!
//! Callers MUST pass already-masked text (US-2.2); every call is recorded to the
//! [`TransferLog`](crate::llm::transfer_log::TransferLog) for transparency (NFR-2).

/// Token budget for classification calls.
///
/// The labels themselves need a handful of tokens; the headroom is for
/// reasoning models that emit an internal reasoning block before the answer.
/// At Flash-tier pricing the unused headroom costs nothing, and without it the
/// call comes back with `content: null`.
#[cfg(feature = "llm-http")]
const CLASSIFY_MAX_TOKENS: u32 = 1024;

/// Token budget for extraction calls.
///
/// The output is a title plus a few sentences, but on a reasoning model the
/// budget is spent on reasoning first and the answer gets what is left — at
/// 1024 the summary came back cut mid-sentence, and a half-sentence stored as a
/// fact is worse than no fact.
#[cfg(feature = "llm-http")]
const SUMMARIZE_MAX_TOKENS: u32 = 4096;

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

    use super::{normalize_base_url, CLASSIFY_MAX_TOKENS, SUMMARIZE_MAX_TOKENS};

    const API_VERSION: &str = "2023-06-01";
    const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
    const DEFAULT_MODEL: &str = "claude-opus-5";

    /// Connection/config for the Anthropic Messages API.
    #[derive(Clone)]
    pub struct LlmConfig {
        pub api_key: String,
        pub model: String,
        pub base_url: String,
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

            let resp = self
                .client
                .post(format!("{}/v1/messages", self.config.base_url))
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
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
        async fn summarize(&self, input: &MaskedText) -> Result<String> {
            self.transfer_log
                .record_text("summarize", &self.config.model, input);
            self.call(
                prompts::SUMMARIZE_SYSTEM,
                json!(input.text),
                SUMMARIZE_MAX_TOKENS,
            )
            .await
        }

        async fn classify(&self, input: &MaskedText) -> Result<Vec<String>> {
            self.transfer_log
                .record_text("classify", &self.config.model, input);
            let raw = self
                // Classification output is a few short labels, but reasoning
                // models spend the budget on internal reasoning before emitting
                // any of them — 128 tokens leaves nothing for the answer.
                .call(
                    prompts::CLASSIFY_SYSTEM,
                    json!(input.text),
                    CLASSIFY_MAX_TOKENS,
                )
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

    use super::{normalize_base_url, CLASSIFY_MAX_TOKENS, SUMMARIZE_MAX_TOKENS};

    const DEFAULT_BASE_URL: &str = "https://api.openai.com";
    const DEFAULT_MODEL: &str = "gpt-4o";

    /// Connection/config for the OpenAI Chat Completions API.
    #[derive(Clone)]
    pub struct OpenAiConfig {
        pub api_key: String,
        pub model: String,
        pub base_url: String,
    }

    impl OpenAiConfig {
        /// Build from the environment: `OPENAI_API_KEY` (required) and optional
        /// `OPENAI_MODEL` / `OPENAI_BASE_URL`.
        pub fn from_env() -> Result<Self> {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| AppError::External("OPENAI_API_KEY not set".into()))?;
            Ok(Self {
                api_key,
                model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
                base_url: normalize_base_url(
                    &std::env::var("OPENAI_BASE_URL")
                        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
                ),
            })
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

            let resp = self
                .client
                .post(format!("{}/v1/chat/completions", self.config.base_url))
                .header("authorization", format!("Bearer {}", self.config.api_key))
                .header("content-type", "application/json")
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
        async fn summarize(&self, input: &MaskedText) -> Result<String> {
            self.transfer_log
                .record_text("summarize", &self.config.model, input);
            self.call(
                prompts::SUMMARIZE_SYSTEM,
                json!(input.text),
                SUMMARIZE_MAX_TOKENS,
            )
            .await
        }

        async fn classify(&self, input: &MaskedText) -> Result<Vec<String>> {
            self.transfer_log
                .record_text("classify", &self.config.model, input);
            let raw = self
                // Classification output is a few short labels, but reasoning
                // models spend the budget on internal reasoning before emitting
                // any of them — 128 tokens leaves nothing for the answer.
                .call(
                    prompts::CLASSIFY_SYSTEM,
                    json!(input.text),
                    CLASSIFY_MAX_TOKENS,
                )
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
