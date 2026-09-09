// U2 collection port — the frontend's view of ingestion.

import type { SourceKind, SourceStatus } from "../../shared/contracts";

/** What one triggered sync produced, collection plus processing. */
export interface IngestSummary {
  collected: number;
  skipped: number;
  errors: number;
  /** Items left for a later run. Zero means the source is fully collected. */
  remaining: number;
  /** Why each error happened — lets the UI tell a bad token from "nothing new". */
  error_messages: string[];
  facts_created: number;
  queue_items_created: number;
  filtered: number;
}

/** One progress tick during a sync (mirrors the backend `IngestProgress`). */
export interface IngestProgress {
  source: SourceKind;
  done: number;
  /** Best-known total; 0 = unknown yet. */
  total: number;
}

export interface SourcesApi {
  /** Subscribe to per-source sync progress. Returns an unsubscribe function.
   * Adapters without a live channel (mock) may return a no-op. */
  onProgress(cb: (p: IngestProgress) => void): () => void;
  /** Connection status of every catalog source (form template + ready flags). */
  listSources(): Promise<SourceStatus[]>;
  /** Store the credential for a source after verifying it. `values` maps each
   * field key → value. Resolves with a human-readable note about what the
   * connection can see (e.g. Notion's shared-page count, or a warning that a
   * valid token has no pages shared yet). */
  connectSource(
    source: SourceKind,
    values: Record<string, string>,
  ): Promise<string>;
  /** Remove a source's credential (disconnect). */
  disconnectSource(source: SourceKind): Promise<void>;
  /** Sync one source, or every registered source when omitted.
   *
   * Takes ONE capped batch per source, so a long history needs repeated calls.
   * Use `triggerIngestAll` to drain it in one go. */
  triggerIngest(source?: SourceKind): Promise<IngestSummary>;
  /** Keep syncing until the source has nothing left.
   *
   * Can run for a long time on a first collection — hundreds of transcripts at
   * roughly ten seconds of model time each — so callers should show progress
   * and let the owner walk away. */
  triggerIngestAll(source?: SourceKind): Promise<IngestSummary>;
  /** Project directories available to collect from, with session counts. */
  listSessionProjects(): Promise<SessionProject[]>;
  /** Root paths collection is currently limited to. Empty = everything. */
  getSessionScope(): Promise<string[]>;
  /** Limit collection to these roots; an empty list restores "everything". */
  setSessionScope(roots: string[]): Promise<void>;
}

/** One project directory the owner can include in or exclude from collection. */
export interface SessionProject {
  /** Absolute path — what `setSessionScope` expects back. */
  path: string;
  /** Readable name derived from the working directory the sessions belong to. */
  label: string;
  sessions: number;
}
