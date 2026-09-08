//! [`LlmGateway`] — the masking/transparency security boundary (Pattern P1/P2/P7).
//!
//! **Every text path to the cloud LLM goes through here.** The gateway only
//! accepts `MaskedText` (never a raw `String`), so there is no type-level path to
//! send un-masked text (BR-K1 / US-2.2 AC1). Each call is logged (US-2.3) and
//! retried with bounded exponential backoff (Q2=A / U2-NFR-REL2).
//!
//! Callers obtain `MaskedText` from U1 `Masker::mask` before calling the gateway.

use std::sync::Arc;

use crate::core::error::{AppError, Result};
use crate::core::traits::LlmClient;
use crate::core::types::{MaskedText, SourceKind};
use crate::processing::transfer_log::{TransferLog, TransferLogEntry, TransferOp};

/// Retry policy for outbound LLM calls (Q2=A). Timeout is enforced by the real
/// `LlmClient` (reqwest) at integration; here we bound the retry attempts.
#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        // 3 attempts, backoff 1s → 2s → 4s (the last delay is not slept after the
        // final attempt). Timeout (30s) lives in the LlmClient impl.
        Self {
            max_attempts: 3,
            base_backoff_ms: 1000,
        }
    }
}

fn preview(text: &str) -> String {
    const MAX: usize = 200;
    if text.len() <= MAX {
        text.to_string()
    } else {
        // Truncate on a char boundary.
        let mut end = MAX;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &text[..end])
    }
}

/// Guards all outbound LLM traffic.
pub struct LlmGateway {
    llm: Arc<dyn LlmClient>,
    log: Arc<TransferLog>,
    policy: RetryPolicy,
    target: String,
}

impl LlmGateway {
    pub fn new(llm: Arc<dyn LlmClient>, log: Arc<TransferLog>) -> Self {
        Self {
            llm,
            log,
            policy: RetryPolicy::default(),
            target: "cloud-llm".to_string(),
        }
    }

    pub fn with_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Log an outbound transfer (masked preview only — BR-T2).
    async fn record(&self, source: SourceKind, op: TransferOp, masked: &MaskedText) -> Result<()> {
        self.log
            .append(TransferLogEntry {
                at_rfc3339: chrono::Utc::now().to_rfc3339(),
                source,
                operation: op,
                masked_preview: preview(&masked.text),
                target: self.target.clone(),
            })
            .await
    }

    /// Run `op` with bounded retry. On exhaustion returns the last error so the
    /// caller can move the item to the pending queue (P3).
    async fn with_retry<T, F, Fut>(&self, mut op: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last: Option<AppError> = None;
        for attempt in 0..self.policy.max_attempts {
            match op().await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    // Auth errors are not retryable — surface immediately (BR-C3).
                    if matches!(&e, AppError::Locked) {
                        return Err(e);
                    }
                    last = Some(e);
                    let is_last = attempt + 1 == self.policy.max_attempts;
                    if !is_last {
                        // Backoff: base * 2^attempt. Sleep is cfg-gated so unit
                        // tests stay fast; timing is validated by policy math.
                        let _delay = self.policy.base_backoff_ms << attempt;
                        #[cfg(not(test))]
                        tokio::time::sleep(std::time::Duration::from_millis(_delay)).await;
                    }
                }
            }
        }
        Err(last.unwrap_or_else(|| AppError::External("llm call failed".into())))
    }

    /// Summarize masked text (logs the transfer). US-2.1 / US-2.2 / US-2.3.
    pub async fn summarize(&self, source: SourceKind, masked: &MaskedText) -> Result<String> {
        self.record(source, TransferOp::Summarize, masked).await?;
        let llm = self.llm.clone();
        let masked = masked.clone();
        self.with_retry(move || {
            let llm = llm.clone();
            let masked = masked.clone();
            async move { llm.summarize(&masked).await }
        })
        .await
    }

    /// Classify masked text (logs the transfer).
    pub async fn classify(&self, source: SourceKind, masked: &MaskedText) -> Result<Vec<String>> {
        self.record(source, TransferOp::Classify, masked).await?;
        let llm = self.llm.clone();
        let masked = masked.clone();
        self.with_retry(move || {
            let llm = llm.clone();
            let masked = masked.clone();
            async move { llm.classify(&masked).await }
        })
        .await
    }

    /// Extract text from an image via vision. The returned `MaskedText` is the
    /// contract (already masked). We log the operation with the *result* preview.
    pub async fn vision_extract(&self, source: SourceKind, image_png: &[u8]) -> Result<MaskedText> {
        let llm = self.llm.clone();
        let bytes = image_png.to_vec();
        let out = self
            .with_retry(move || {
                let llm = llm.clone();
                let bytes = bytes.clone();
                async move { llm.vision_extract(&bytes).await }
            })
            .await?;
        self.record(source, TransferOp::VisionExtract, &out).await?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::{CannedLlm, InMemoryStore};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn gateway() -> (LlmGateway, Arc<TransferLog>) {
        let log = Arc::new(TransferLog::new(Arc::new(InMemoryStore::default())));
        let gw = LlmGateway::new(Arc::new(CannedLlm), log.clone());
        (gw, log)
    }

    #[tokio::test]
    async fn summarize_logs_transfer() {
        let (gw, log) = gateway();
        let masked = MaskedText {
            text: "[NAME] deployed".into(),
        };
        let out = gw.summarize(SourceKind::Session, &masked).await.unwrap();
        assert!(out.contains("summary"));
        let entries = log.all().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, TransferOp::Summarize);
        assert_eq!(entries[0].masked_preview, "[NAME] deployed");
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        // LlmClient that fails twice, then succeeds.
        struct Flaky {
            calls: AtomicU32,
        }
        #[async_trait::async_trait]
        impl LlmClient for Flaky {
            async fn summarize(&self, input: &MaskedText) -> Result<String> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(AppError::External("transient".into()))
                } else {
                    Ok(format!("ok {}", input.text))
                }
            }
            async fn classify(&self, _i: &MaskedText) -> Result<Vec<String>> {
                Ok(vec![])
            }
            async fn vision_extract(&self, _i: &[u8]) -> Result<MaskedText> {
                Ok(MaskedText {
                    text: String::new(),
                })
            }
            async fn chat(&self, _s: &str, _i: &MaskedText) -> Result<String> {
                Ok(String::new())
            }
        }
        let log = Arc::new(TransferLog::new(Arc::new(InMemoryStore::default())));
        let gw = LlmGateway::new(
            Arc::new(Flaky {
                calls: AtomicU32::new(0),
            }),
            log,
        );
        let out = gw
            .summarize(SourceKind::Session, &MaskedText { text: "x".into() })
            .await
            .unwrap();
        assert_eq!(out, "ok x");
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts() {
        struct AlwaysFail;
        #[async_trait::async_trait]
        impl LlmClient for AlwaysFail {
            async fn summarize(&self, _i: &MaskedText) -> Result<String> {
                Err(AppError::External("down".into()))
            }
            async fn classify(&self, _i: &MaskedText) -> Result<Vec<String>> {
                Err(AppError::External("down".into()))
            }
            async fn vision_extract(&self, _i: &[u8]) -> Result<MaskedText> {
                Err(AppError::External("down".into()))
            }
            async fn chat(&self, _s: &str, _i: &MaskedText) -> Result<String> {
                Err(AppError::External("down".into()))
            }
        }
        let log = Arc::new(TransferLog::new(Arc::new(InMemoryStore::default())));
        let gw = LlmGateway::new(Arc::new(AlwaysFail), log);
        let r = gw
            .classify(SourceKind::Session, &MaskedText { text: "x".into() })
            .await;
        assert!(r.is_err());
    }
}
