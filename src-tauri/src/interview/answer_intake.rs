//! `AnswerIntake` — turn answers into confirmed facts and derive follow-ups.
//!
//! Pure/near-pure helpers used by `InterviewService::answer` (FD Q9):
//! - Confirm + affirmative → a confirmed fact from the candidate.
//! - Deepen + text → a confirmed fact from the answer + bounded follow-up
//!   questions derived via `Masker` → `LlmClient` (mask-first, KR-7).
//!
//! Follow-up generation is best-effort: on any LLM error it returns an empty
//! list so the answer's fact is still saved (graceful degradation, P-4/NFR-3).

use chrono::Utc;

use crate::core::traits::{LlmClient, Masker};
use crate::core::types::{
    Fact, FactCandidate, FactId, FactMetadata, MaskedText, Provenance, QueueItem, QueueItemId,
    QueueItemKind, Scope, SourceKind,
};

/// Max follow-up questions generated per answered deepen item (IR-6 bound).
const MAX_FOLLOW_UPS: usize = 3;

/// Choices that mean "no / reject"; everything else counts as affirmative.
fn is_negative(choice: &str) -> bool {
    matches!(
        choice.trim().to_lowercase().as_str(),
        "no" | "n" | "reject" | "false" | "x" | "아니오" | "아니요" | "아니" | "아님"
    )
}

/// Interpret a `Choice` answer to a Confirm item as affirmative or not.
/// An empty/whitespace choice is NOT affirmative (avoids confirming on a blank).
pub fn is_affirmative(choice: &str) -> bool {
    !choice.trim().is_empty() && !is_negative(choice)
}

/// Build a confirmed fact from an accepted candidate.
pub fn fact_from_candidate(c: FactCandidate) -> Fact {
    Fact {
        id: FactId::new(),
        title: c.title,
        body: c.body,
        links: vec![],
        metadata: FactMetadata {
            provenance: c.provenance,
            confirmed: true,
            scope: c.suggested_scope,
            confirmed_at: Some(Utc::now()),
            visibility: Default::default(),
            category: None,
        },
    }
}

/// Build a confirmed fact from a free-text answer to a deepen question.
///
/// NOTE (U1 contract coordination): interview-originated facts have no natural
/// `SourceKind`. Until a `SourceKind::Interview` variant is added to the shared
/// contract, provenance source is set to `Session` as a documented placeholder.
pub fn fact_from_deepen(question: &str, answer: &str) -> Fact {
    let title: String = question.trim().chars().take(80).collect();
    Fact {
        id: FactId::new(),
        title,
        body: answer.to_string(),
        links: vec![],
        metadata: FactMetadata {
            provenance: Provenance {
                source: SourceKind::Session, // TODO(U1): SourceKind::Interview
                collected_at: Utc::now(),
            },
            confirmed: true,
            scope: Scope::Unknown,
            confirmed_at: Some(Utc::now()),
            visibility: Default::default(),
            category: None,
        },
    }
}

/// Derive up to [`MAX_FOLLOW_UPS`] follow-up deepen questions from an answer.
/// Masks before any cloud call (KR-7); returns `[]` on LLM failure (P-4).
pub async fn derive_follow_ups(
    masker: &dyn Masker,
    llm: &dyn LlmClient,
    answer: &str,
) -> Vec<QueueItem> {
    let (masked, map) = masker.mask(answer);
    let labels = match llm.classify(&masked).await {
        Ok(labels) => labels,
        Err(_) => return Vec::new(), // graceful degradation — fact still persists
    };
    labels
        .into_iter()
        .take(MAX_FOLLOW_UPS)
        .map(|label| {
            // Restore any mask placeholders so questions show real terms (e.g. a
            // name) rather than tokens like "[PERSON_1]".
            let restored = masker.unmask(&MaskedText { text: label }, &map);
            QueueItem {
                id: QueueItemId::new(),
                kind: QueueItemKind::Deepen {
                    question: format!("{restored}에 대해 더 알려주세요"),
                    hypothesis: None,
                },
                priority: 0, // scored on enqueue
                created_at: Utc::now(),
                expires_at: None, // TTL applied on enqueue
            }
        })
        .collect()
}
