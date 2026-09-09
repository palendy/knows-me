import type { SourceKind, SourceStatus } from "../../shared/contracts";
import type { IngestSummary, SourcesApi } from "./api";

/** Field templates mirroring the Rust `credential_spec` so the browser-mode
 * sources screen renders the same connect form without Tauri. */
const CATALOG: SourceStatus[] = [
  { kind: "Session", fields: [], connected: false, ready: true },
  { kind: "File", fields: [], connected: false, ready: true },
  {
    kind: "Notion",
    fields: [
      {
        key: "token",
        label: "Integration 토큰",
        placeholder: "secret_xxx (Notion integration의 Internal Integration Token)",
        secret: true,
        required: true,
      },
    ],
    connected: false,
    ready: false,
  },
  {
    kind: "Gmail",
    fields: [
      {
        key: "address",
        label: "Gmail 주소",
        placeholder: "you@gmail.com",
        secret: false,
        required: true,
      },
      {
        key: "app_password",
        label: "앱 비밀번호",
        placeholder: "Google 계정 → 보안 → 앱 비밀번호에서 발급",
        secret: true,
        required: true,
      },
    ],
    connected: false,
    ready: false,
  },
];

/** Browser-mode stand-in so the sources screen is runnable without Tauri. Keeps
 * connection state in memory so connect/disconnect behave end-to-end. */
export class MockSourcesApi implements SourcesApi {
  /** Which sources have a stored credential this session. */
  private connected = new Set<SourceKind>();

  async listSources(): Promise<SourceStatus[]> {
    await new Promise((r) => setTimeout(r, 100));
    return CATALOG.map((s) => {
      const isConnected = this.connected.has(s.kind);
      return {
        ...s,
        connected: isConnected,
        // Credential-less sources stay ready; others become ready once connected.
        ready: s.fields.length === 0 ? true : isConnected,
      };
    });
  }

  async connectSource(
    source: SourceKind,
    values: Record<string, string>,
  ): Promise<string> {
    await new Promise((r) => setTimeout(r, 150));
    const spec = CATALOG.find((s) => s.kind === source);
    if (!spec || spec.fields.length === 0) {
      throw new Error(`${source}는 자격증명을 받지 않습니다`);
    }
    const missing = spec.fields
      .filter((f) => f.required && !values[f.key]?.trim())
      .map((f) => f.label);
    if (missing.length > 0) {
      throw new Error(`필수 항목을 입력하세요: ${missing.join(", ")}`);
    }
    this.connected.add(source);
    return "연결되었습니다.";
  }

  async disconnectSource(source: SourceKind): Promise<void> {
    await new Promise((r) => setTimeout(r, 100));
    this.connected.delete(source);
  }

  async triggerIngest(source?: SourceKind): Promise<IngestSummary> {
    await new Promise((r) => setTimeout(r, 300));
    if (
      (source === "Notion" || source === "Gmail") &&
      !this.connected.has(source)
    ) {
      throw new Error(`${source} 계정이 연결되지 않았습니다`);
    }
    return {
      collected: 12,
      skipped: 3,
      errors: 0,
      remaining: 41,
      error_messages: [],
      facts_created: 5,
      queue_items_created: 4,
      filtered: 3,
    };
  }
}
