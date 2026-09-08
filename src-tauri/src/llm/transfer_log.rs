//! Transfer transparency log (NFR-2, US-2.3).
//!
//! The cloud LLM is the single egress point for user data, and U1 owns it — so
//! recording *what left the device* belongs here. Only already-masked content is
//! ever recorded, and only a truncated preview.

use std::sync::Mutex;

use chrono::Utc;

use crate::core::types::{MaskedText, TransferRecord};

const PREVIEW_CHARS: usize = 200;

/// In-memory, append-only record of outbound cloud-LLM calls.
#[derive(Default)]
pub struct TransferLog {
    records: Mutex<Vec<TransferRecord>>,
}

impl TransferLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one masked text transfer (summarize/classify/chat).
    pub fn record_text(&self, purpose: &str, model: &str, masked: &MaskedText) {
        self.push(TransferRecord {
            at: Utc::now(),
            purpose: purpose.to_string(),
            model: model.to_string(),
            masked_preview: truncate(&masked.text, PREVIEW_CHARS),
            bytes_sent: masked.text.len(),
        });
    }

    /// Record one image transfer (vision). Images cannot be text-masked, so the
    /// preview only notes the byte size.
    pub fn record_image(&self, model: &str, bytes: usize) {
        self.push(TransferRecord {
            at: Utc::now(),
            purpose: "vision".to_string(),
            model: model.to_string(),
            masked_preview: format!("[image, {bytes} bytes]"),
            bytes_sent: bytes,
        });
    }

    /// A snapshot of all recorded transfers, oldest first.
    pub fn list(&self) -> Vec<TransferRecord> {
        self.records.lock().expect("transfer log poisoned").clone()
    }

    fn push(&self, rec: TransferRecord) {
        self.records
            .lock()
            .expect("transfer log poisoned")
            .push(rec);
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_masked_preview_only() {
        let log = TransferLog::new();
        log.record_text(
            "summarize",
            "claude-opus-5",
            &MaskedText {
                text: "safe masked summary".into(),
            },
        );
        let recs = log.list();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].purpose, "summarize");
        assert_eq!(recs[0].masked_preview, "safe masked summary");
        assert_eq!(recs[0].bytes_sent, "safe masked summary".len());
    }

    #[test]
    fn preview_truncates_long_text() {
        let log = TransferLog::new();
        let long = "x".repeat(500);
        log.record_text("chat", "m", &MaskedText { text: long });
        let preview = &log.list()[0].masked_preview;
        assert!(preview.chars().count() <= PREVIEW_CHARS + 1); // + ellipsis
        assert!(preview.ends_with('…'));
    }
}
