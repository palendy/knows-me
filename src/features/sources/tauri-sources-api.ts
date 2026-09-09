import type { SourceKind, SourceStatus } from "../../shared/contracts";
import type { IngestSummary, SourcesApi } from "./api";

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export class TauriSourcesApi implements SourcesApi {
  constructor(private readonly invoke: Invoke) {}

  listSources(): Promise<SourceStatus[]> {
    return this.invoke<SourceStatus[]>("list_sources");
  }

  connectSource(
    source: SourceKind,
    values: Record<string, string>,
  ): Promise<void> {
    return this.invoke<void>("connect_source", { source, values });
  }

  disconnectSource(source: SourceKind): Promise<void> {
    return this.invoke<void>("disconnect_source", { source });
  }

  triggerIngest(source?: SourceKind): Promise<IngestSummary> {
    return this.invoke<IngestSummary>("trigger_ingest", {
      source: source ?? null,
    });
  }
}
