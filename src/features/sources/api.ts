// U2 collection port — the frontend's view of ingestion.

import type { SourceKind, SourceStatus } from "../../shared/contracts";

/** What one triggered sync produced, collection plus processing. */
export interface IngestSummary {
  collected: number;
  skipped: number;
  errors: number;
  /** Items left for a later run. Zero means the source is fully collected. */
  remaining: number;
  facts_created: number;
  queue_items_created: number;
  filtered: number;
}

export interface SourcesApi {
  /** Connection status of every catalog source (form template + ready flags). */
  listSources(): Promise<SourceStatus[]>;
  /** Store the credential for a source. `values` maps each field key → value. */
  connectSource(
    source: SourceKind,
    values: Record<string, string>,
  ): Promise<void>;
  /** Remove a source's credential (disconnect). */
  disconnectSource(source: SourceKind): Promise<void>;
  /** Sync one source, or every registered source when omitted. */
  triggerIngest(source?: SourceKind): Promise<IngestSummary>;
}
