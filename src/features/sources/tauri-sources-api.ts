import type { SourceKind, SourceStatus } from "../../shared/contracts";
import type { IngestProgress, IngestSummary, SourcesApi } from "./api";

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export class TauriSourcesApi implements SourcesApi {
  constructor(private readonly invoke: Invoke) {}

  onProgress(cb: (p: IngestProgress) => void): () => void {
    // `listen` resolves to an unlisten fn asynchronously; hold it so the
    // synchronously-returned disposer can stop the listener once ready.
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<IngestProgress>("ingest://progress", (e) => cb(e.payload)).then(
        (fn) => {
          if (cancelled) fn();
          else unlisten = fn;
        },
      ),
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }

  listSources(): Promise<SourceStatus[]> {
    return this.invoke<SourceStatus[]>("list_sources");
  }

  connectSource(
    source: SourceKind,
    values: Record<string, string>,
  ): Promise<string> {
    return this.invoke<string>("connect_source", { source, values });
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
