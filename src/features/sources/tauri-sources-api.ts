import type { SourceKind } from "../../shared/contracts";
import type { IngestSummary, SourcesApi } from "./api";

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export class TauriSourcesApi implements SourcesApi {
  constructor(private readonly invoke: Invoke) {}

  triggerIngest(source?: SourceKind): Promise<IngestSummary> {
    return this.invoke<IngestSummary>("trigger_ingest", {
      source: source ?? null,
    });
  }
}
