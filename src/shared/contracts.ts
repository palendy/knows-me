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
export type FactKind = "Note" | "Practice" | "Preference" | "Project" | "Concern";

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

export interface GraphNode {
  id: FactId;
  label: string;
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

export interface AppConfig {
  transfer_policy: TransferPolicy;
  server_enabled: boolean;
  llm_model: string;
}

/** One recorded outbound cloud-LLM call (masked content only). */
export interface TransferRecord {
  at: IsoDateTime;
  purpose: string;
  model: string;
  masked_preview: string;
  bytes_sent: number;
}
