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

export interface FactSummary {
  id: FactId;
  title: string;
  scope: Scope;
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

export interface PersonaReply {
  text: string;
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
