//! U4 — Interface & Persona.
//!
//! Stories: US-5.1 (dashboard), US-5.2 (mini-home), US-5.3 (knowledge graph),
//! US-6.1 (persona chat), US-6.2 (local API drafting).
//!
//! Unit boundary: this module **consumes** U1 contracts (`Masker`, `LlmClient`,
//! shared types) and U3's [`KnowledgeApi`](crate::core::traits::KnowledgeApi)
//! read side. It never writes through `KnowledgeApi` and never adds types to
//! `core::types` — U4-derived types live here.
//!
//! Two deliberate structural guarantees:
//! - [`QueryService`] is constructed **without** an `LlmClient`, so the three
//!   read views cannot depend on the network (NFR-3, U4-NFR-A1).
//! - Everything variable that reaches the cloud goes through a single
//!   `Masker::mask` call in [`PersonaService`] (NFR-2, BR-P2).

pub mod context;
pub mod local_api;
pub mod query;
pub mod selection;

#[cfg(test)]
mod properties;
#[cfg(test)]
pub mod testgen;

use crate::core::types::{FactId, Scope};

pub use context::{render_prompt, select_context};
pub use local_api::{LocalApiHandle, LocalApiServer, DEFAULT_PORT};
pub use query::QueryService;
pub use selection::select_highlights;
pub use service::PersonaService;

pub mod service;

// ---------------------------------------------------------------------------
// U4-derived types
// ---------------------------------------------------------------------------

/// One confirmed fact selected as grounding for a persona answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextEntry {
    pub id: FactId,
    pub title: String,
    pub body: String,
    pub scope: Scope,
    /// Query-relevance score; higher sorts first.
    pub relevance: u32,
}

/// A snapshot of the confirmed context a persona answer is grounded in.
///
/// Invariants (see `business-rules.md` BR-P1/P5/P7):
/// - every entry comes from a fact with `metadata.confirmed == true`
/// - `entries.len() <= ContextSelection::max_facts`
/// - no `FactId` appears twice
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonaContext {
    pub entries: Vec<ContextEntry>,
    /// How many confirmed facts existed before selection narrowed them down.
    pub total_confirmed: usize,
}

impl PersonaContext {
    /// No grounding available — callers must not call the LLM (BR-P4).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Policy for narrowing confirmed facts down to a bounded context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextSelection {
    /// Hard upper bound on facts placed in the prompt (U4-NFR-S2).
    pub max_facts: usize,
    /// Optional scope restriction.
    pub scope: Option<Scope>,
}

impl Default for ContextSelection {
    fn default() -> Self {
        Self {
            max_facts: DEFAULT_MAX_FACTS,
            scope: None,
        }
    }
}

/// Default context size. Bounds prompt cost regardless of knowledge-base size.
pub const DEFAULT_MAX_FACTS: usize = 12;

/// How many candidate facts are fetched in full before relevance is refined.
/// Bounds the read amplification of `search` -> `get` (U4-NFR-P5).
pub const DEFAULT_FETCH_CAP: usize = 64;

/// Maximum accepted length of an owner prompt / draft request (U4-NFR-P4).
pub const MAX_PROMPT_CHARS: usize = 4000;

/// The exact reply returned when there is no confirmed context (BR-P4).
pub const NO_CONTEXT_REPLY: &str =
    "확정된 맥락이 없어 답변할 수 없습니다. 인터뷰 Queue에서 질문에 답하면 페르소나가 근거로 쓸 사실이 쌓입니다.";

/// A prompt ready for the LLM gateway.
///
/// `system` is a **static template** that contains no owner data, so only
/// `user_document` needs masking — one `mask()` call, one `UnmaskMap`, no
/// placeholder-namespace collision between two independent masking passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonaPrompt {
    /// Persona instructions + output-format directive. Contains no owner data.
    pub system: String,
    /// Context block + owner question. This is the part that gets masked.
    pub user_document: String,
}
