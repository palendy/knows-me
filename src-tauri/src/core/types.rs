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

/// What kind of thing a stored item is.
///
/// Without this every item reads the same way, and "what am I worried about"
/// has no answer distinguishable from "what did I do" — which is the difference
/// between a knowledge base and an activity log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactKind {
    /// Unclassified.
    #[default]
    Note,
    /// How the owner works — a procedure, rule, or convention they follow.
    Practice,
    /// What the owner prefers, likes, or insists on.
    Preference,
    /// Something the owner is building or running.
    Project,
    /// Friction: something unresolved, repeatedly returned to, or expressed as
    /// frustration. This is what "걱정" retrieves.
    Concern,
    /// A term or idea the owner uses with a specific meaning in their own work.
    ///
    /// "아바타 카드가 뭐야?" is answerable only if what the owner *means* by a
    /// term is stored — a session can use a term two hundred times and leave
    /// nothing behind unless meaning is a thing the extractor looks for.
    Concept,
}

/// Company vs personal classification for a fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Company,
    Personal,
    Unknown,
}

/// Access classification — *who* may see a fact. Orthogonal to [`Scope`] (which
/// classifies the *topic*, company vs personal). Default is `Private`: nothing is
/// shared with anyone until the owner explicitly marks it `Shared`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    /// Owner (and the owner's own agent) only.
    #[default]
    Private,
    /// Exposable to granted consumers, scoped by the fact's `category`.
    Shared,
}

/// A sharing category — the grant unit. Free-form, but stored/compared only in
/// normalized form so `Deploy` and `deploy` cannot become distinct grants (a
/// silent authorization miss). Construct only via [`Category::parse`].
/// See `docs/mcp-contract.md` §2.1.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Category(String);

impl Category {
    /// Normalize `raw` and return a valid `Category`, or `InvalidInput`.
    ///
    /// Rules: trim + lowercase; whitespace and `_` become a single `-`; keep only
    /// `a-z 0-9 - .` and Hangul; collapse repeated `-` and trim leading/trailing
    /// `-`; final length must be 1..=64 chars.
    pub fn parse(raw: &str) -> crate::core::error::Result<Self> {
        fn is_hangul(c: char) -> bool {
            matches!(c, '\u{AC00}'..='\u{D7A3}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}')
        }
        let mut out = String::with_capacity(raw.len());
        let mut prev_dash = false;
        for ch in raw.trim().to_lowercase().chars() {
            if ch.is_whitespace() || ch == '_' || ch == '-' {
                if !prev_dash {
                    out.push('-');
                    prev_dash = true;
                }
            } else if ch.is_ascii_alphanumeric() || ch == '.' || is_hangul(ch) {
                out.push(ch);
                prev_dash = false;
            }
            // disallowed characters are dropped
        }
        let trimmed = out.trim_matches('-');
        let count = trimmed.chars().count();
        if count == 0 || count > 64 {
            return Err(crate::core::error::AppError::InvalidInput(
                "category".into(),
            ));
        }
        Ok(Category(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IngestReport {
    pub collected: usize,
    pub skipped: usize,
    pub errors: usize,
    /// Items left for a later run.
    ///
    /// A run is capped, so pressing collect on a long history keeps returning
    /// new items — correct, but indistinguishable from re-collecting the same
    /// ones unless the remaining count is shown.
    #[serde(default)]
    pub remaining: usize,
    /// Human-readable cause for each contained error (P5), so the UI can show
    /// *why* a source failed rather than a bare "1 error" count. One entry per
    /// counted error, newest last.
    pub error_messages: Vec<String>,
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
    /// Access classification (who may see this fact). Default `Private`.
    #[serde(default)]
    pub visibility: Visibility,
    /// Grant unit for sharing (e.g. "deploy", "workstyle"). `None` = uncategorized.
    /// A fact must carry a category to be reachable once `visibility == Shared`.
    #[serde(default)]
    pub category: Option<Category>,
    /// Subject tags, used to group observations into topic pages. A precursor
    /// to `category`: topics are what the classifier proposes, a category is
    /// what the owner grants on.
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub kind: FactKind,
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
    /// Normalized topic tags from classification.
    ///
    /// These are what let separate observations be recognized as being about
    /// the same thing — a knowledge base without them is a list of unrelated
    /// session summaries, which is what it reads like.
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub kind: FactKind,
    #[serde(default)]
    pub visibility: Visibility,
}

/// Lightweight fact projection for lists/search results.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactSummary {
    pub id: FactId,
    pub title: String,
    pub scope: Scope,
    #[serde(default)]
    pub kind: FactKind,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub visibility: Visibility,
    pub confirmed: bool,
}

/// Normalize a topic tag so the same subject always compares equal.
///
/// Delegates to [`Category::parse`] rather than restating its rules: topics are
/// what the classifier proposes and categories are what the owner grants on, so
/// the two must normalize identically or a topic could not be promoted to a
/// category without silently changing which facts it covers.
pub fn normalize_topic(raw: &str) -> Option<String> {
    Category::parse(raw).ok().map(|c| c.as_str().to_string())
}

/// Filter for fact search.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FactFilter {
    pub scope: Option<Scope>,
    /// Restrict to a single category (U3-internal search). `None` = no filter.
    /// Not an MCP argument — see `docs/mcp-contract.md` §3.2.
    #[serde(default)]
    pub category: Option<Category>,
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

/// A subject, assembled from every observation that mentions it.
///
/// Computed on read rather than stored: derived state that is persisted has to
/// be kept in sync, and a stale topic page is worse than none. At personal
/// scale the aggregation is cheap enough to redo per request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopicPage {
    pub topic: String,
    /// How many observations mention it — repetition is what makes a subject
    /// matter, and a single mention is not an interest.
    pub mentions: usize,
    /// Observations classified as friction. This is what "무엇을 걱정하나"
    /// actually retrieves.
    pub concerns: usize,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    /// The facts behind it, newest first.
    pub facts: Vec<FactSummary>,
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
///
/// The LLM fields mirror the environment variables the `llm` gateway reads
/// (`LLM_PROVIDER`, `*_MODEL`, `*_BASE_URL`). They are applied to the process
/// environment on unlock so the existing `from_env` selection path picks them
/// up unchanged — see [`AppConfig::apply_to_env`]. The API key is a secret and
/// is never stored here; it lives in the encrypted store under the `llm`
/// namespace (owned by the desktop shell).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    /// What may be sent to the cloud LLM (default: mask & minimize).
    pub transfer_policy: TransferPolicy,
    /// Whether the local persona API server is enabled (U4 owns the server).
    pub server_enabled: bool,
    /// Which LLM backend to drive: `claude-cli`, `anthropic`, or `openai`.
    #[serde(default = "default_provider")]
    pub llm_provider: String,
    /// Cloud LLM model id used by the shared gateway.
    pub llm_model: String,
    /// Optional base URL override for the HTTP providers (e.g. an OpenAI-
    /// compatible gateway such as OpenRouter). Ignored by the CLI backend.
    #[serde(default)]
    pub llm_base_url: Option<String>,
}

/// The backend the app drives when nothing has been configured yet. Matches the
/// `llm` gateway's own default so an unconfigured install runs the local Claude
/// Code the owner already has, with no API key.
fn default_provider() -> String {
    "claude-cli".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            transfer_policy: TransferPolicy::default(),
            server_enabled: false,
            llm_provider: default_provider(),
            // The CLI backend's own default model. Kept in sync with
            // `ClaudeCliConfig::default` so the settings screen and the client
            // agree before the owner picks a model.
            llm_model: "claude-sonnet-5".to_string(),
            llm_base_url: None,
        }
    }
}

impl AppConfig {
    /// Push the LLM selection into the process environment so the gateway's
    /// `from_env` paths ([`crate::llm::build_client`], [`active_model_label`])
    /// resolve to *this* config rather than whatever `.env` shipped.
    ///
    /// The API key is not handled here — the desktop shell sets
    /// `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` from the encrypted store before
    /// calling this, and clearing them when absent is its job too. This only
    /// owns the non-secret selection (provider, model, base URL) and mirrors it
    /// to the variable each provider reads.
    ///
    /// [`active_model_label`]: crate::llm::active_model_label
    pub fn apply_to_env(&self) {
        let provider = self.llm_provider.trim();
        std::env::set_var("LLM_PROVIDER", provider);

        match provider.to_ascii_lowercase().as_str() {
            "openai" => {
                std::env::set_var("OPENAI_MODEL", &self.llm_model);
                Self::set_or_clear("OPENAI_BASE_URL", self.llm_base_url.as_deref());
            }
            "claude-cli" | "claude" => {
                std::env::set_var("CLAUDE_CLI_MODEL", &self.llm_model);
            }
            // Anthropic HTTP is the fallback provider in the gateway, so treat
            // any other value the same way rather than dropping the model.
            _ => {
                std::env::set_var("ANTHROPIC_MODEL", &self.llm_model);
                Self::set_or_clear("ANTHROPIC_BASE_URL", self.llm_base_url.as_deref());
            }
        }
    }

    /// Set `var` to `value`, or remove it entirely when `value` is `None`/blank,
    /// so switching back to a provider's default base URL doesn't leave a stale
    /// override behind in the process environment.
    fn set_or_clear(var: &str, value: Option<&str>) {
        match value.map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => std::env::set_var(var, v),
            None => std::env::remove_var(var),
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
