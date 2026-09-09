// U2 collection port — the frontend's view of ingestion.

import type { SourceKind } from "../../shared/contracts";

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
  /** Sync one source, or every registered source when omitted. */
  triggerIngest(source?: SourceKind): Promise<IngestSummary>;
}
