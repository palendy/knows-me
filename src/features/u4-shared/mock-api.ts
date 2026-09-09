// Deterministic development/test implementation of the U4 data port.
//
// Mirrors the semantics of U1's `mocks::InMemoryKnowledge` (only confirmed
// facts surface; persona answers are grounded in them) so that behaviour seen
// against this adapter matches what the real backend will do.

import type {
  ChatTurn,
  DashboardDto,
  StoredTurn,
  Draft,
  DraftRequest,
  Fact,
  FactId,
  GraphDto,
  GraphFilter,
  MiniHomeDto,
  PersonaReply,
  Visibility,
} from "../../shared/contracts";
import type { KnowsMeApi } from "./api";
import { selectHighlights } from "./selection";

/** Mirrors `NO_CONTEXT_REPLY` in `src-tauri/src/persona/mod.rs` (BR-P4). */
export const NO_CONTEXT_REPLY =
  "확정된 맥락이 없어 답변할 수 없습니다. 인터뷰 Queue에서 질문에 답하면 페르소나가 근거로 쓸 사실이 쌓입니다.";

/** Fixed ids keep fixtures — and therefore test expectations — stable. */
const id = (n: number): string =>
  `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;

function fact(
  n: number,
  title: string,
  body: string,
  links: string[],
  confirmed = true,
  topics: string[] = [],
): Fact {
  return {
    id: id(n),
    title,
    body,
    links,
    metadata: {
      provenance: { source: "Session", collected_at: "2026-09-01T09:00:00Z" },
      confirmed,
      scope: n % 2 === 0 ? "Company" : "Personal",
      confirmed_at: confirmed ? `2026-09-0${(n % 8) + 1}T09:00:00Z` : null,
      topics,
      visibility: "Private",
      category: null,
    },
  };
}

export const SAMPLE_FACTS: Fact[] = [
  fact(1, "배포 절차", "main 에 머지되면 make deploy 로 배포한다", [id(2), id(3), id(4)], true, ["deploy"]),
  fact(2, "코드 리뷰 규칙", "PR 은 최소 1인 승인 후 머지한다", [id(1)], true, ["review", "deploy"]),
  fact(3, "테스트 명령", "cargo test 와 npm test 를 둘 다 돌린다", [id(1)]),
  fact(4, "선호 에디터", "neovim 을 주로 쓴다", [id(1)]),
  fact(5, "커피 취향", "산미 있는 원두를 좋아한다", []),
  fact(6, "미확정 후보", "아직 확인되지 않은 내용", [], false),
];

/** In-memory adapter over a fixed fact set. */
export class MockApi implements KnowsMeApi {
  private readonly facts: Fact[];
  constructor(
    facts: Fact[] = SAMPLE_FACTS,
    private readonly pendingQueue = 3,
  ) {
    // Clone so setFactSharing mutations don't leak into the shared fixture (and
    // across test instances that share SAMPLE_FACTS).
    this.facts = facts.map((f) => ({ ...f, metadata: { ...f.metadata } }));
  }

  async getDashboard(): Promise<DashboardDto> {
    const confirmed = this.facts.filter((f) => f.metadata.confirmed);
    return {
      collected_count: this.facts.length,
      pending_queue: this.pendingQueue,
      recent_facts: selectHighlights(confirmed, 5),
    };
  }

  async getMiniHome(limit = 9): Promise<MiniHomeDto> {
    return { highlights: selectHighlights(this.facts, limit) };
  }

  async getGraph(filter: GraphFilter): Promise<GraphDto> {
    const visible = this.facts.filter(
      (f) =>
        f.metadata.confirmed &&
        (filter.scope == null || f.metadata.scope === filter.scope),
    );
    const present = new Set(visible.map((f) => f.id));
    return {
      nodes: visible.map((f) => ({ id: f.id, label: f.title, kind: "fact" as const })),
      edges: visible.flatMap((f) =>
        f.links
          .filter((l) => present.has(l))
          .map((l) => ({ from: f.id, to: l })),
      ),
    };
  }

  async getFact(id: FactId): Promise<Fact> {
    const f = this.facts.find((x) => x.id === id);
    if (!f) throw new Error(`fact not found: ${id}`);
    return { ...f, metadata: { ...f.metadata } };
  }

  async setFactSharing(id: FactId, visibility: Visibility, category: string | null): Promise<void> {
    const f = this.facts.find((x) => x.id === id);
    if (!f) throw new Error(`fact not found: ${id}`);
    f.metadata.visibility = visibility;
    f.metadata.category = category;
  }

  async personaChat(prompt: string, history: ChatTurn[] = []): Promise<PersonaReply> {
    // Mirror the service: a follow-up retrieves over the whole thread, so
    // "그거 더 자세히" resolves the same way it does against the real backend.
    const query = [prompt, ...history.filter((t) => t.role === "Owner").map((t) => t.text)]
      .join(" ")
      .toLowerCase();
    // Term-wise matching, mirroring the Rust service. Matching the whole query
    // as one substring would mean a natural question ("내 배포 절차 알려줘")
    // never hits a fact whose title is just "배포 절차" — the demo path would
    // answer "no context" for every question a person would actually type.
    const terms = query
      .split(/[^\p{L}\p{N}]+/u)
      .filter((t) => t.length >= 2);

    const confirmed = this.facts.filter((f) => f.metadata.confirmed);

    // No confirmed facts at all is the *only* condition that yields the
    // no-context reply (BR-P4). Otherwise the service widens to the confirmed
    // set and lets the model say it does not know — Korean particles mean
    // "에디터가" will not substring-match "에디터", so term scoring alone
    // would strand perfectly answerable questions.
    if (confirmed.length === 0) {
      return { text: NO_CONTEXT_REPLY, sources: [] };
    }

    const scored = confirmed
      .map((f) => {
        const title = f.title.toLowerCase();
        const body = f.body.toLowerCase();
        const score = terms.reduce(
          (acc, t) => acc + (title.includes(t) ? 3 : 0) + (body.includes(t) ? 1 : 0),
          0,
        );
        return { fact: f, score };
      })
      .sort((a, b) => b.score - a.score || a.fact.id.localeCompare(b.fact.id))
      .slice(0, 3);

    return {
      text: scored.map((x) => `${x.fact.title}: ${x.fact.body}`).join("\n"),
      sources: scored.map((x) => ({ id: x.fact.id, title: x.fact.title })),
    };
  }

  async personaDraft(req: DraftRequest): Promise<Draft> {
    return { text: `[${req.kind} 초안]\n${req.prompt}` };
  }

  private history: StoredTurn[] = [];
  async loadChatHistory(): Promise<StoredTurn[]> {
    return [...this.history];
  }
  async saveChatHistory(turns: StoredTurn[]): Promise<void> {
    this.history = [...turns];
  }
}

/** An adapter that always fails — for exercising error states in tests. */
export class FailingApi implements KnowsMeApi {
  constructor(private readonly message = "외부 서비스에 연결할 수 없습니다") {}
  private fail(): never {
    throw new Error(this.message);
  }
  async getDashboard(): Promise<DashboardDto> {
    this.fail();
  }
  async getMiniHome(): Promise<MiniHomeDto> {
    this.fail();
  }
  async getGraph(): Promise<GraphDto> {
    this.fail();
  }
  async getFact(): Promise<Fact> {
    this.fail();
  }
  async setFactSharing(): Promise<void> {
    this.fail();
  }
  async personaChat(): Promise<PersonaReply> {
    this.fail();
  }
  async personaDraft(): Promise<Draft> {
    this.fail();
  }
  async loadChatHistory(): Promise<StoredTurn[]> {
    // A vault that cannot be read has no history to offer; the view must still
    // open, so this is an empty thread rather than a thrown error.
    return [];
  }
  async saveChatHistory(): Promise<void> {
    this.fail();
  }
}

/**
 * Build an adapter from `base` with selected methods replaced.
 *
 * Spreading a class instance (`{ ...new MockApi(), getDashboard }`) silently
 * drops every prototype method, so tests delegate explicitly instead.
 *
 * Each delegate forwards `...args`. An earlier version listed parameters one by
 * one, and when `personaChat` gained a `history` argument the helper quietly
 * dropped it — the call still succeeded, so a test written to prove the thread
 * reaches the backend passed against a helper that had thrown it away.
 */
export function withOverrides(
  base: KnowsMeApi,
  overrides: Partial<KnowsMeApi>,
): KnowsMeApi {
  return {
    getDashboard: (...args) =>
      overrides.getDashboard?.(...args) ?? base.getDashboard(...args),
    getMiniHome: (...args) =>
      overrides.getMiniHome?.(...args) ?? base.getMiniHome(...args),
    getGraph: (...args) => overrides.getGraph?.(...args) ?? base.getGraph(...args),
    getFact: (...args) => overrides.getFact?.(...args) ?? base.getFact(...args),
    setFactSharing: (...args) =>
      overrides.setFactSharing?.(...args) ?? base.setFactSharing(...args),
    personaChat: (...args) =>
      overrides.personaChat?.(...args) ?? base.personaChat(...args),
    personaDraft: (...args) =>
      overrides.personaDraft?.(...args) ?? base.personaDraft(...args),
    loadChatHistory: (...args) =>
      overrides.loadChatHistory?.(...args) ?? base.loadChatHistory(...args),
    saveChatHistory: (...args) =>
      overrides.saveChatHistory?.(...args) ?? base.saveChatHistory(...args),
  };
}
