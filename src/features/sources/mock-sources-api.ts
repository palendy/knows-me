import type { SourceKind } from "../../shared/contracts";
import type { IngestSummary, SourcesApi } from "./api";

/** Browser-mode stand-in so the sources screen is runnable without Tauri. */
export class MockSourcesApi implements SourcesApi {
  async triggerIngest(source?: SourceKind): Promise<IngestSummary> {
    await new Promise((r) => setTimeout(r, 300));
    if (source === "Notion" || source === "Gmail") {
      throw new Error(`${source} 계정이 연결되지 않았습니다`);
    }
    return {
      collected: 12,
      skipped: 3,
      errors: 0,
      remaining: 41,
      facts_created: 5,
      queue_items_created: 4,
      filtered: 3,
    };
  }
}
