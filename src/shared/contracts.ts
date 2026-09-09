// knows-me — shared frontend contract types (U1 Milestone 0)
//
// Mirror of the Rust contract types in `src-tauri/src/core/types.rs`. The
// frontend (U4) codes against these while the Tauri commands are implemented.
// Keep this file in sync with the Rust types (U1 owner gate-keeps changes).
//
// NOTE: serde serializes Rust enums like `SourceKind::Session` as the string
// "Session", and data-carrying enums (QueueItemKind, AnswerInput) as
// externally-tagged objects, e.g. { "Confirm": { candidate: {...} } }.

export type Uuid = string;
export type FactId = Uuid;
export type QueueItemId = Uuid;
/** ISO-8601 timestamp (UTC). */
export type IsoDateTime = string;

export type SourceKind = "Session" | "Notion" | "Gmail" | "File";
export type Scope = "Company" | "Personal" | "Unknown";
export type TransferPolicy = "MaskAndMinimize" | "AllowAll" | "LocalOnlyNoLlm";
export type DraftKind = "Email" | "Message" | "Post";
export type QueueSort = "PriorityDesc" | "NewestFirst";

export interface Provenance {
  source: SourceKind;
  collected_at: IsoDateTime;
}

export interface FactMetadata {
  provenance: Provenance;
  confirmed: boolean;
  scope: Scope;
  confirmed_at: IsoDateTime | null;
  /** Normalized topic tags; absent on facts stored before topics existed. */
  topics?: string[];
  kind?: FactKind;
  visibility?: Visibility;
}

export interface Fact {
  id: FactId;
  title: string;
  body: string;
  links: FactId[];
  metadata: FactMetadata;
}

export interface FactChange {
  changed_at: IsoDateTime;
  before: string | null;
  after: string;
  note: string | null;
}

export interface FactCandidate {
  title: string;
  body: string;
  provenance: Provenance;
  suggested_scope: Scope;
}

/** What kind of thing a stored item is. */
export type FactKind = "Note" | "Practice" | "Preference" | "Project" | "Concern" | "Concept";

/** Whether a page may leave the owner. Separate axis from `Scope`. */
export type Visibility = "Private" | "Shared";

export interface FactSummary {
  id: FactId;
  title: string;
  scope: Scope;
  kind: FactKind;
  topics: string[];
  visibility: Visibility;
  confirmed: boolean;
}

export interface FactFilter {
  scope?: Scope | null;
}

export interface GraphFilter {
  scope?: Scope | null;
}

// Data-carrying enums are externally tagged by serde.
export type QueueItemKind =
  | { Confirm: { candidate: FactCandidate } }
  | { Deepen: { question: string; hypothesis: string | null } };

export interface QueueItem {
  id: QueueItemId;
  kind: QueueItemKind;
  priority: number;
  created_at: IsoDateTime;
  expires_at: IsoDateTime | null;
}

export type AnswerInput =
  | { Choice: string }
  | { Text: string }
  | "Skip";

export interface AnswerResult {
  confirmed_fact: Fact | null;
  follow_ups: QueueItem[];
}

export interface DashboardDto {
  collected_count: number;
  pending_queue: number;
  recent_facts: FactSummary[];
}

/** A fact node is a record; a topic node is a synthetic hub the facts sharing
 *  a subject connect to, so the graph reads as constellations, not a chain. */
export type GraphNodeKind = "fact" | "topic";

export interface GraphNode {
  id: FactId;
  label: string;
  kind: GraphNodeKind;
}

export interface GraphEdge {
  from: FactId;
  to: FactId;
}

export interface GraphDto {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface MiniHomeDto {
  highlights: FactSummary[];
}

/** Who said a turn in a persona conversation. */
export type ChatRole = "Owner" | "Persona";

/** One prior turn, replayed so follow-up questions resolve. */
export interface ChatTurn {
  role: ChatRole;
  text: string;
}

/** A fact the persona used as grounding. */
export interface FactRef {
  id: FactId;
  title: string;
}

/** One persisted conversation turn, as the desktop crate stores it. */
export interface StoredTurn {
  role: ChatRole;
  text: string;
  sources: FactRef[];
  error: string | null;
}

export interface PersonaReply {
  text: string;
  /** Facts the answer was grounded in; empty when it declined for lack of context. */
  sources: FactRef[];
}

export interface DraftRequest {
  kind: DraftKind;
  prompt: string;
}

export interface Draft {
  text: string;
}

export interface IngestReport {
  collected: number;
  skipped: number;
  errors: number;
}

// --- U2: source connection ------------------------------------------------

/** One credential field a source requires. Mirrors Rust `FieldSpec`. */
export interface FieldSpec {
  /** JSON key the value is stored under in the credential. */
  key: string;
  label: string;
  placeholder: string;
  /** Secret (token/password): password input; never echoed back once stored. */
  secret: boolean;
  required: boolean;
}

/** A source's connection status. Mirrors Rust `SourceStatus`. Secret values
 * are never included — only whether the source is connected/ready. */
export interface SourceStatus {
  kind: SourceKind;
  fields: FieldSpec[];
  connected: boolean;
  ready: boolean;
}

// --- U1: onboarding / security / config / transparency --------------------

export interface Credential {
  // serde: Credential(pub serde_json::Value) → the inner JSON value directly.
  [key: string]: unknown;
}

/** Snapshot the onboarding UI uses to pick the Setup vs. Unlock screen. */
export interface AppStatus {
  initialized: boolean;
  unlocked: boolean;
}

/** Which LLM backend the shared gateway drives. */
export type LlmProvider = "claude-cli" | "anthropic" | "openai";

export interface AppConfig {
  transfer_policy: TransferPolicy;
  server_enabled: boolean;
  /** Whether the sharing (MCP) servers are enabled (owner-loopback + shared). */
  sharing_enabled: boolean;
  /** Backend selector: local Claude Code CLI, Anthropic HTTP, or OpenAI-compatible. */
  llm_provider: LlmProvider;
  /** Model id sent to the selected provider. */
  llm_model: string;
  /** Base URL override for the HTTP providers (null = provider default). */
  llm_base_url: string | null;
  /** Which local Claude Code install the CLI backend drives, as a command line
   * ("claude", a path, or "wsl -d <distro> claude"). null = default `claude`. */
  llm_binary: string | null;
}

/** A detected local Claude Code CLI install offered in the settings picker. */
export interface ClaudeInstall {
  /** Stable id, e.g. "native" or "wsl:Ubuntu". */
  id: string;
  /** Human label, e.g. "Windows" or "WSL · Ubuntu". */
  label: string;
  /** Command line the backend invokes for this install. */
  binary: string;
  /** Model this install currently has configured (settings.json), if known. */
  model: string | null;
}

/**
 * What `get_config` returns: the persisted {@link AppConfig} fields (flattened)
 * plus two read-only signals for the settings screen.
 */
export interface ConfigDto extends AppConfig {
  /** The model actually in effect right now, e.g. "claude-sonnet-5 (로컬 Claude Code)". */
  llm_label: string;
  /** Whether an API key is stored for the selected HTTP provider (never the key itself). */
  has_api_key: boolean;
}

/** The editable LLM settings the frontend sends to `set_llm_config`. */
export interface LlmConfigInput {
  provider: LlmProvider;
  model: string;
  base_url: string | null;
  /** Write-only: omit (null) to keep the stored key, "" to clear it. */
  api_key: string | null;
  /** claude-cli only: the selected install's command line (null = default). */
  binary: string | null;
}

/** One recorded outbound cloud-LLM call (masked content only). */
export interface TransferRecord {
  at: IsoDateTime;
  purpose: string;
  model: string;
  masked_preview: string;
  bytes_sent: number;
}

// --- Sharing (MCP) ---------------------------------------------------------

/** Runtime sharing status from `share_status`. */
export interface ShareStatus {
  /** Config flag: whether the MCP servers should run this session. */
  enabled: boolean;
  /** Owner-loopback listener port (step ⓐ), or null when not running. */
  owner_port: number | null;
  /** Bearer-only shared listener port (step ⓑ), or null when not running. */
  shared_port: number | null;
  /** Public tunnel URL fronting the shared listener, or null when no tunnel is up. */
  tunnel_url: string | null;
  /** Whether a cloudflared binary is runnable — distinguishes "tunnel off" from
   * "can't tunnel". */
  cloudflared_installed: boolean;
}

/** A freshly issued consumer token. `secret` is shown **once** — it is never
 * recoverable, so the UI must surface it immediately. */
export interface IssuedShareToken {
  id: string;
  secret: string;
}

/** A live token's public metadata (never the secret). */
export interface ShareTokenInfo {
  id: string;
  granted: string[];
  issued_at: IsoDateTime;
}
