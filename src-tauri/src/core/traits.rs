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
    /// Remove a stored credential (disconnect). Idempotent — deleting an absent
    /// credential succeeds.
    async fn delete(&self, source: SourceKind) -> Result<()>;
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

/// Receives incremental progress during a sync so the UI can show a real
/// (done/total) bar instead of an indeterminate spinner. `total` is 0 when the
/// connector doesn't know the count yet.
pub trait ProgressReporter: Send + Sync {
    fn progress(&self, source: SourceKind, done: usize, total: usize);
}

/// A no-op reporter for callers/connectors that don't report progress.
pub struct NoProgress;
impl ProgressReporter for NoProgress {
    fn progress(&self, _source: SourceKind, _done: usize, _total: usize) {}
}

/// A pluggable source connector. New sources are added by implementing this
/// trait — no other layer changes.
#[async_trait]
pub trait Connector: Send + Sync {
    fn id(&self) -> SourceKind;
    /// Incremental sync from the given cursor; returns new items + next cursor.
    /// `progress` is called as work advances (connectors that fetch item-by-item
    /// report each step; others may ignore it).
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        progress: &dyn ProgressReporter,
    ) -> Result<(Vec<RawItem>, Cursor)>;

    /// How many items this source still has beyond what `sync` just returned.
    ///
    /// Defaults to zero for sources that collect everything in one pass.
    async fn remaining(&self, _cursor: Option<Cursor>) -> Result<usize> {
        Ok(0)
    }
    /// Apply a source-specific configuration (e.g. which directories to scan).
    ///
    /// Defaults to ignoring it, so a connector with nothing to configure needs
    /// no code. Takes `&self` because connectors live behind `Arc` in the
    /// registry and are reconfigured while the app runs — an implementation
    /// that stores config keeps it behind its own lock.
    fn configure(&self, _config: &SourceConfig) -> Result<()> {
        Ok(())
    }

    /// Whether this source supports manual upload.
    fn supports_manual(&self) -> bool;
}

/// Orchestrates connectors and scheduling.
#[async_trait]
pub trait IngestionApi: Send + Sync {
    async fn configure(&self, source: SourceKind, config: SourceConfig) -> Result<()>;

    /// Like `trigger`, but keeps going until the source has nothing left.
    ///
    /// Connectors bound one pass so a single manual sync stays predictable;
    /// this is the caller that wants the whole backlog drained. `max_passes`
    /// is a safety bound, not a target.
    async fn trigger_all(
        &self,
        source: Option<SourceKind>,
        progress: &dyn ProgressReporter,
        max_passes: usize,
    ) -> Result<IngestReport>;
    /// Run ingestion for one source (or all if `None`), reporting progress as it
    /// goes so the caller can drive a UI progress bar.
    async fn trigger(
        &self,
        source: Option<SourceKind>,
        progress: &dyn ProgressReporter,
    ) -> Result<IngestReport>;
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
    /// Subjects assembled across observations, most significant first.
    ///
    /// A knowledge base that can only return individual observations can say
    /// what happened but not what matters; significance lives in repetition and
    /// recency, which only exist across items.
    async fn topics(&self, filter: FactFilter) -> Result<Vec<TopicPage>>;
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
    /// Answer as the owner's persona.
    ///
    /// `history` carries prior turns, oldest first, so follow-ups resolve
    /// against what was already said. Callers that have no history pass an
    /// empty slice.
    async fn chat(&self, prompt: String, history: Vec<ChatTurn>) -> Result<PersonaReply>;
    async fn draft(&self, req: DraftRequest) -> Result<Draft>;
}
