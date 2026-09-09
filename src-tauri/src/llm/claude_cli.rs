//! LLM backend that drives the locally installed `claude` CLI.
//!
//! Everyone using this project already has Claude Code, so extraction can run
//! on the subscription they already pay for instead of a second API account.
//! There is no API key here — the CLI carries its own auth.
//!
//! Two consequences worth knowing before choosing it:
//! - **Latency.** Each call spawns an agent session; measured ~12s against
//!   Haiku 4.5, versus well under a second for a direct API call. A 30-item
//!   collection run therefore takes minutes, not seconds.
//! - **Same egress.** The text still reaches Anthropic, so the masking contract
//!   applies exactly as it does to the HTTP clients: callers pass
//!   [`MaskedText`], never raw content.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::core::error::{AppError, Result};
use crate::core::traits::LlmClient;
use crate::core::types::MaskedText;
use crate::llm::prompts;
use crate::llm::transfer_log::TransferLog;

/// Where the CLI lives and which model it should use.
#[derive(Clone, Debug)]
pub struct ClaudeCliConfig {
    pub binary: String,
    pub model: String,
    pub timeout: Duration,
}

impl Default for ClaudeCliConfig {
    fn default() -> Self {
        Self {
            binary: "claude".into(),
            // Extraction quality is the reason to choose this backend at all;
            // override with CLAUDE_CLI_MODEL when speed matters more.
            model: "claude-sonnet-5".into(),
            timeout: Duration::from_secs(180),
        }
    }
}

impl ClaudeCliConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(b) = std::env::var("CLAUDE_CLI_BINARY") {
            cfg.binary = b;
        }
        if let Ok(m) = std::env::var("CLAUDE_CLI_MODEL") {
            cfg.model = m;
        }
        if let Some(secs) = std::env::var("CLAUDE_CLI_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            cfg.timeout = Duration::from_secs(secs);
        }
        cfg
    }
}

pub struct ClaudeCliLlm {
    config: ClaudeCliConfig,
    transfer_log: Arc<TransferLog>,
}

impl ClaudeCliLlm {
    pub fn new(config: ClaudeCliConfig, transfer_log: Arc<TransferLog>) -> Self {
        Self {
            config,
            transfer_log,
        }
    }

    /// Run one non-interactive turn and return stdout.
    ///
    /// The instruction goes in the prompt rather than `--append-system-prompt`:
    /// that flag *appends* to Claude Code's own coding-agent system prompt, and
    /// a schema stated there loses to it — the first attempt came back with
    /// invented enum values. Stated as the request itself, the schema holds.
    ///
    /// Tools are disabled and the turn count pinned to one so a classification
    /// call cannot wander into reading files.
    async fn run(&self, instruction: &str, input: &str) -> Result<String> {
        let mut child = Command::new(&self.config.binary)
            .arg("-p")
            .args(["--model", &self.config.model])
            .args(["--allowed-tools", ""])
            .args(["--max-turns", "1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                AppError::External(format!(
                    "could not start `{}` ({e}); install Claude Code or set \
                     CLAUDE_CLI_BINARY",
                    self.config.binary
                ))
            })?;

        let prompt = format!("{instruction}\n\n{input}");
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| AppError::External(format!("claude stdin: {e}")))?;
            stdin
                .shutdown()
                .await
                .map_err(|e| AppError::External(format!("claude stdin: {e}")))?;
        }

        let output = tokio::time::timeout(self.config.timeout, child.wait_with_output())
            .await
            .map_err(|_| {
                AppError::External(format!(
                    "claude CLI timed out after {}s",
                    self.config.timeout.as_secs()
                ))
            })?
            .map_err(|e| AppError::External(format!("claude CLI failed: {e}")))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::External(format!(
                "claude CLI exited with {}: {}",
                output.status,
                err.trim().chars().take(200).collect::<String>()
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return Err(AppError::External(
                "claude CLI returned no output; is it signed in? (`claude` once interactively)"
                    .into(),
            ));
        }
        Ok(text)
    }
}

#[async_trait]
impl LlmClient for ClaudeCliLlm {
    async fn summarize(&self, input: &MaskedText) -> Result<String> {
        self.transfer_log
            .record_text("summarize", &self.config.model, input);
        self.run(prompts::SUMMARIZE_SYSTEM, &input.text).await
    }

    async fn classify(&self, input: &MaskedText) -> Result<Vec<String>> {
        self.transfer_log
            .record_text("classify", &self.config.model, input);
        let raw = self.run(prompts::CLASSIFY_SYSTEM, &input.text).await?;
        Ok(prompts::parse_labels(&raw))
    }

    async fn vision_extract(&self, _image_png: &[u8]) -> Result<MaskedText> {
        // The CLI takes a prompt on stdin; there is no path for image bytes.
        // Image sources need one of the HTTP backends.
        Err(AppError::External(
            "the claude CLI backend cannot read images; use LLM_PROVIDER=anthropic for vision"
                .into(),
        ))
    }

    async fn chat(&self, system: &str, input: &MaskedText) -> Result<String> {
        self.transfer_log
            .record_text("chat", &self.config.model, input);
        self.run(system, &input.text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_missing_binary_says_what_to_do_about_it() {
        let llm = ClaudeCliLlm::new(
            ClaudeCliConfig {
                binary: "definitely-not-a-real-binary".into(),
                ..Default::default()
            },
            Arc::new(TransferLog::new()),
        );

        let err = llm
            .summarize(&MaskedText {
                text: "본문".into(),
            })
            .await
            .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("CLAUDE_CLI_BINARY"), "got: {msg}");
    }

    #[test]
    fn the_model_is_overridable_without_touching_code() {
        std::env::set_var("CLAUDE_CLI_MODEL", "claude-haiku-4-5");
        assert_eq!(ClaudeCliConfig::from_env().model, "claude-haiku-4-5");
        std::env::remove_var("CLAUDE_CLI_MODEL");
    }
}
