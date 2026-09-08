//! Service interfaces (contracts). Every unit codes against these traits so the
//! four units can be developed in parallel and integrated later.
//!
//! Ownership:
//! - U1 provides: [`EncryptedStore`], [`CredentialStore`], [`KeyManager`],
//!   [`Masker`], [`LlmClient`].
//! - U2 provides: [`Connector`], [`IngestionApi`], [`ProcessingApi`].
//! - U3 provides: [`KnowledgeApi`], [`InterviewApi`].
//! - U4 provides: [`PersonaApi`] (consumes `KnowledgeApi` + `LlmClient` + `Masker`).

use async_trait::async_trait;

use crate::core::error::Result;
use crate::core::types::*;

// ---------------------------------------------------------------------------
// U1 — Security & shared LLM gateway
// ---------------------------------------------------------------------------

/// Encrypted key/value storage. All persistence flows through this gate so data
/// at rest is always encrypted. `ns` is a logical namespace (e.g. "facts").
#[async_trait]
pub trait EncryptedStore: Send + Sync {
    async fn put(&self, ns: &str, key: &str, bytes: &[u8]) -> Result<()>;
    async fn get(&self, ns: &str, key: &str) -> Result<Option<Vec<u8>>>;
    async fn list(&self, ns: &str) -> Result<Vec<String>>;
    async fn delete(&self, ns: &str, key: &str) -> Result<()>;
}

/// Encrypted storage for external-system credentials.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    async fn store(&self, source: SourceKind, cred: Credential) -> Result<()>;
    async fn load(&self, source: SourceKind) -> Result<Option<Credential>>;
}

/// Password-based key lifecycle. The plaintext key is never persisted.
pub trait KeyManager: Send + Sync {
    /// First-run: derive a key from the password and persist verifier/salt.
    fn setup(&self, password: &str) -> Result<()>;
    /// Re-derive the key from the password and unlock the app.
    fn unlock(&self, password: &str) -> Result<KeyHandle>;
    /// Drop the in-memory key.
    fn lock(&self);
    fn is_unlocked(&self) -> bool;
}

/// Removes identifiers/secrets before cloud transmission and restores them
/// locally. Pure and synchronous.
pub trait Masker: Send + Sync {
    /// Returns masked text plus the local-only reverse mapping.
    fn mask(&self, text: &str) -> (MaskedText, UnmaskMap);
    /// Restores original values using the reverse mapping (local only).
    fn unmask(&self, masked: &MaskedText, map: &UnmaskMap) -> String;
}

/// Cloud LLM gateway. Callers MUST pass already-masked input for text paths.
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn summarize(&self, input: &MaskedText) -> Result<String>;
    async fn classify(&self, input: &MaskedText) -> Result<Vec<String>>;
    async fn vision_extract(&self, image_png: &[u8]) -> Result<MaskedText>;
    async fn chat(&self, system: &str, input: &MaskedText) -> Result<String>;
}

// ---------------------------------------------------------------------------
// U2 — Ingestion & Processing
// ---------------------------------------------------------------------------

/// A pluggable source connector. New sources are added by implementing this
/// trait — no other layer changes.
#[async_trait]
pub trait Connector: Send + Sync {
    fn id(&self) -> SourceKind;
    /// Incremental sync from the given cursor; returns new items + next cursor.
    async fn sync(&self, cursor: Option<Cursor>) -> Result<(Vec<RawItem>, Cursor)>;
    /// Whether this source supports manual upload.
    fn supports_manual(&self) -> bool;
}

/// Orchestrates connectors and scheduling.
#[async_trait]
pub trait IngestionApi: Send + Sync {
    async fn configure(&self, source: SourceKind, config: SourceConfig) -> Result<()>;
    /// Run ingestion for one source (or all if `None`).
    async fn trigger(&self, source: Option<SourceKind>) -> Result<IngestReport>;
}

/// Masks, summarizes/classifies raw items into facts / queue items.
#[async_trait]
pub trait ProcessingApi: Send + Sync {
    async fn process(&self, items: Vec<RawItem>) -> Result<ProcessReport>;
}

// ---------------------------------------------------------------------------
// U3 — Knowledge & Interview
// ---------------------------------------------------------------------------

/// The file-based "LLM wiki" knowledge store (facts + history + search + graph).
#[async_trait]
pub trait KnowledgeApi: Send + Sync {
    async fn upsert(&self, fact: Fact) -> Result<FactId>;
    async fn get(&self, id: FactId) -> Result<Fact>;
    async fn links(&self, id: FactId) -> Result<Vec<FactId>>;
    async fn history(&self, id: FactId) -> Result<Vec<FactChange>>;
    async fn search(&self, query: String, filter: FactFilter) -> Result<Vec<FactSummary>>;
    async fn graph(&self, filter: GraphFilter) -> Result<GraphDto>;
    async fn dashboard(&self) -> Result<DashboardDto>;
}

/// The interview queue (confirm/deepen items) and answer intake.
#[async_trait]
pub trait InterviewApi: Send + Sync {
    async fn enqueue(&self, item: QueueItem) -> Result<QueueItemId>;
    async fn list(&self, sort: QueueSort) -> Result<Vec<QueueItem>>;
    async fn answer(&self, id: QueueItemId, answer: AnswerInput) -> Result<AnswerResult>;
    /// Remove expired/superseded items; returns count removed.
    async fn expire(&self) -> Result<usize>;
}

// ---------------------------------------------------------------------------
// U4 — Persona
// ---------------------------------------------------------------------------

/// Persona avatar: chat + "act on my behalf" drafting.
#[async_trait]
pub trait PersonaApi: Send + Sync {
    async fn chat(&self, prompt: String) -> Result<PersonaReply>;
    async fn draft(&self, req: DraftRequest) -> Result<Draft>;
}
