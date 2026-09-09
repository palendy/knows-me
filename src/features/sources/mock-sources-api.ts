import type { SourceKind, SourceStatus } from "../../shared/contracts";
import type {
  IngestProgress,
  IngestSummary,
  SessionProject,
  SourcesApi,
} from "./api";

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
  /** Roots collection is limited to; empty means everything. */
  private scope: string[] = [];
  private progressCbs = new Set<(p: IngestProgress) => void>();

  onProgress(cb: (p: IngestProgress) => void): () => void {
    this.progressCbs.add(cb);
    return () => this.progressCbs.delete(cb);
  }

  private emitProgress(p: IngestProgress) {
    for (const cb of this.progressCbs) cb(p);
  }

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
    if (
      (source === "Notion" || source === "Gmail") &&
      !this.connected.has(source)
    ) {
      await new Promise((r) => setTimeout(r, 300));
      throw new Error(`${source} 계정이 연결되지 않았습니다`);
    }
    // Simulate page-by-page progress so the browser-mode bar is exercisable.
    const total = 12;
    for (let done = 1; done <= total; done++) {
      await new Promise((r) => setTimeout(r, 30));
      this.emitProgress({ source: source ?? "Session", done, total });
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

  /** Drains the backlog: the distinguishing feature is `remaining: 0`. */
  async triggerIngestAll(source?: SourceKind): Promise<IngestSummary> {
    const first = await this.triggerIngest(source);
    const total = first.collected + first.remaining;
    return {
      ...first,
      collected: total,
      remaining: 0,
      facts_created: Math.round(first.facts_created * (total / first.collected)),
      queue_items_created: first.queue_items_created,
      filtered: first.filtered,
    };
  }

  async listSessionProjects(): Promise<SessionProject[]> {
    await new Promise((r) => setTimeout(r, 60));
    return [
      { path: "/p/knows-me", label: "Work/18_avatar/knows-me", sessions: 103, newest: "2026-09-09T04:00:00Z" },
      { path: "/p/trade", label: "Work/06_trade_follow", sessions: 19, newest: "2026-09-02T10:00:00Z" },
      { path: "/p/hack", label: "Work/18_avatar/dsdn_hackerton", sessions: 12, newest: "2026-08-30T09:00:00Z" },
    ];
  }

  async getSessionScope(): Promise<string[]> {
    return [...this.scope];
  }

  async setSessionScope(roots: string[]): Promise<void> {
    await new Promise((r) => setTimeout(r, 40));
    this.scope = [...roots];
  }
}
