//! Shared domain types. These are the vocabulary every unit speaks.
//!
//! Design notes:
//! - IDs are UUID newtypes for type safety.
//! - Timestamps use `chrono::DateTime<Utc>`.
//! - Secret-bearing types (`KeyHandle`, `UnmaskMap`, `Credential`) are NOT
//!   serialized casually and never leave the device unencrypted.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// Identifier for a stored fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactId(pub Uuid);

impl FactId {
    pub fn new() -> Self {
        FactId(Uuid::new_v4())
    }
}

impl Default for FactId {
    fn default() -> Self {
        Self::new()
    }
}

/// Identifier for an interview queue item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueueItemId(pub Uuid);

impl QueueItemId {
    pub fn new() -> Self {
        QueueItemId(Uuid::new_v4())
    }
}

impl Default for QueueItemId {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Where a piece of data came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    Session,
    Notion,
    Gmail,
    File,
}

/// Company vs personal classification for a fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Company,
    Personal,
    Unknown,
}

/// Policy governing what may be sent to the cloud LLM.
/// Default reflects the confirmed decision (mask/minimize before sending).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransferPolicy {
    /// Mask identifiers/secrets and minimize before sending (default).
    #[default]
    MaskAndMinimize,
    /// Send as-is (convenience; not recommended).
    AllowAll,
    /// Never call the cloud LLM (fully local).
    LocalOnlyNoLlm,
}

// ---------------------------------------------------------------------------
// Ingestion
// ---------------------------------------------------------------------------

/// Opaque incremental-sync cursor for a source connector.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cursor(pub String);

/// A raw, un-processed item pulled from a source (before masking/summarization).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawItem {
    pub source: SourceKind,
    /// Stable id in the source system (used for idempotent, incremental sync).
    pub external_id: String,
    pub collected_at: DateTime<Utc>,
    /// Text content, if any.
    pub text: Option<String>,
    /// Raw PNG bytes for image inputs (vision extraction happens in Processing).
    pub image_png: Option<Vec<u8>>,
}

/// Free-form, source-specific configuration (auth handled separately via Vault).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceConfig(pub serde_json::Value);

/// Summary of an ingestion run.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct IngestReport {
    pub collected: usize,
    pub skipped: usize,
    pub errors: usize,
}

// ---------------------------------------------------------------------------
// Processing / masking
// ---------------------------------------------------------------------------

/// Text that has been masked and is safe(r) to send to the cloud LLM.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaskedText {
    pub text: String,
}

/// Reverse mapping used to restore masked tokens locally.
/// Never serialized/persisted in plaintext and never sent off-device.
#[derive(Clone, Debug, Default)]
pub struct UnmaskMap {
    entries: Vec<(String, String)>,
}

impl UnmaskMap {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, placeholder: String, original: String) {
        self.entries.push((placeholder, original));
    }
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Summary of a processing run.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ProcessReport {
    pub facts_created: usize,
    pub queue_items_created: usize,
    pub filtered: usize,
}

// ---------------------------------------------------------------------------
// Knowledge
// ---------------------------------------------------------------------------

/// Where/when a fact was collected.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    pub source: SourceKind,
    pub collected_at: DateTime<Utc>,
}

/// Metadata attached to every fact.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactMetadata {
    pub provenance: Provenance,
    pub confirmed: bool,
    pub scope: Scope,
    pub confirmed_at: Option<DateTime<Utc>>,
}

/// A confirmed unit of context. Persisted as one document (wiki page).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fact {
    pub id: FactId,
    pub title: String,
    pub body: String,
    /// Links to other facts (the "wiki" graph edges).
    pub links: Vec<FactId>,
    pub metadata: FactMetadata,
}

/// A single change entry in a fact's history (never overwritten).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactChange {
    pub changed_at: DateTime<Utc>,
    pub before: Option<String>,
    pub after: String,
    pub note: Option<String>,
}

/// A processing-derived candidate fact awaiting confirmation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactCandidate {
    pub title: String,
    pub body: String,
    pub provenance: Provenance,
    pub suggested_scope: Scope,
}

/// Lightweight fact projection for lists/search results.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactSummary {
    pub id: FactId,
    pub title: String,
    pub scope: Scope,
    pub confirmed: bool,
}

/// Filter for fact search.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FactFilter {
    pub scope: Option<Scope>,
}

/// Filter for graph queries.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GraphFilter {
    pub scope: Option<Scope>,
}

// ---------------------------------------------------------------------------
// Interview queue
// ---------------------------------------------------------------------------

/// The two kinds of interview items.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum QueueItemKind {
    /// Confirm-style: verify a candidate fact is correct.
    Confirm { candidate: FactCandidate },
    /// Deepen-style: an open question that drills into gathered context.
    Deepen {
        question: String,
        hypothesis: Option<String>,
    },
}

/// An item awaiting the owner's answer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: QueueItemId,
    pub kind: QueueItemKind,
    /// Higher = shown first.
    pub priority: u8,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// How to sort the queue for display.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum QueueSort {
    PriorityDesc,
    NewestFirst,
}

/// The owner's answer to a queue item.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AnswerInput {
    /// Selected one of the offered choices.
    Choice(String),
    /// Free-text answer.
    Text(String),
    /// Skipped for now.
    Skip,
}

/// Result of answering a queue item.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnswerResult {
    /// A confirmed fact, if the answer produced one.
    pub confirmed_fact: Option<Fact>,
    /// Follow-up questions generated from the answer.
    pub follow_ups: Vec<QueueItem>,
}

// ---------------------------------------------------------------------------
// Interface DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DashboardDto {
    pub collected_count: usize,
    pub pending_queue: usize,
    pub recent_facts: Vec<FactSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: FactId,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: FactId,
    pub to: FactId,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GraphDto {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MiniHomeDto {
    pub highlights: Vec<FactSummary>,
}

// ---------------------------------------------------------------------------
// Persona
// ---------------------------------------------------------------------------

/// Who said a turn in a persona conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    Owner,
    Persona,
}

/// One prior turn, replayed so follow-up questions resolve.
///
/// Without this the persona answers every question cold: "그거 더 자세히"
/// has no referent, which is most of what a real conversation is made of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: ChatRole,
    pub text: String,
}

/// A fact the persona used as grounding, named so the owner can check it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactRef {
    pub id: FactId,
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersonaReply {
    pub text: String,
    /// The confirmed facts this answer was grounded in. Empty when the persona
    /// declined to answer for lack of context.
    #[serde(default)]
    pub sources: Vec<FactRef>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum DraftKind {
    Email,
    Message,
    Post,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DraftRequest {
    pub kind: DraftKind,
    pub prompt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub text: String,
}

// ---------------------------------------------------------------------------
// Security
// ---------------------------------------------------------------------------

/// An opaque, in-memory handle to the derived 256-bit encryption key.
///
/// The key material is wrapped in [`Zeroizing`] so it is wiped from memory when
/// the last handle is dropped, and is exposed only to same-crate crypto
/// (`security::vault`) — never serialized, logged, or sent off-device.
/// `Arc` lets the unlocked key be shared cheaply by the store/vault without
/// copying the secret.
#[derive(Clone)]
pub struct KeyHandle(Arc<Zeroizing<[u8; 32]>>);

impl KeyHandle {
    /// Wrap raw 32-byte key material. Crate-internal: real key derivation lives
    /// in U1's [`crate::security::key_manager`].
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        KeyHandle(Arc::new(Zeroizing::new(bytes)))
    }

    /// Borrow the raw key bytes for same-crate AEAD operations. Deliberately not
    /// `pub` — no other unit can read the key.
    pub(crate) fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    /// Construct a handle backed by a random key, for tests/mocks that need a
    /// real (but throwaway) key without going through password setup.
    pub fn new_for_test() -> Self {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        KeyHandle::from_bytes(bytes)
    }
}

/// An external system credential (OAuth token / API key), stored encrypted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credential(pub serde_json::Value);

// ---------------------------------------------------------------------------
// App configuration & transfer transparency
// ---------------------------------------------------------------------------

/// Persisted, non-secret application configuration held in `AppState`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    /// What may be sent to the cloud LLM (default: mask & minimize).
    pub transfer_policy: TransferPolicy,
    /// Whether the local persona API server is enabled (U4 owns the server).
    pub server_enabled: bool,
    /// Cloud LLM model id used by the shared gateway.
    pub llm_model: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            transfer_policy: TransferPolicy::default(),
            server_enabled: false,
            llm_model: "claude-opus-5".to_string(),
        }
    }
}

/// A record of one outbound cloud-LLM call, for transfer transparency (NFR-2,
/// US-2.3). Only *masked* content is recorded, and only a truncated preview.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferRecord {
    pub at: DateTime<Utc>,
    /// "summarize" | "classify" | "vision" | "chat".
    pub purpose: String,
    pub model: String,
    /// Truncated, already-masked preview of what left the device.
    pub masked_preview: String,
    pub bytes_sent: usize,
}
