// Tauri adapter — the integration seam with U1's command layer.
//
// U1 owns `CommandRouter` and registers the five commands named below. Until
// that lands this adapter is unused (views are constructed with `MockApi`);
// switching over is a one-line change at the injection site, which is the whole
// point of the port (U4-NFR-M5).

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

/** Signature of Tauri's `invoke`, injected rather than imported. */
export type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/** Command names U4 expects U1 to register. */
export const COMMANDS = {
  dashboard: "get_dashboard",
  minihome: "get_minihome",
  graph: "get_graph",
  fact: "get_fact",
  setFactSharing: "set_fact_sharing",
  chat: "persona_chat",
  draft: "persona_draft",
  historyLoad: "persona_history_load",
  historySave: "persona_history_save",
} as const;

export class TauriApi implements KnowsMeApi {
  constructor(private readonly invoke: Invoke) {}

  getDashboard(): Promise<DashboardDto> {
    return this.invoke<DashboardDto>(COMMANDS.dashboard);
  }
  getMiniHome(limit?: number): Promise<MiniHomeDto> {
    return this.invoke<MiniHomeDto>(COMMANDS.minihome, { limit: limit ?? null });
  }
  getGraph(filter: GraphFilter): Promise<GraphDto> {
    return this.invoke<GraphDto>(COMMANDS.graph, { filter });
  }
  getFact(id: FactId): Promise<Fact> {
    return this.invoke<Fact>(COMMANDS.fact, { id });
  }
  setFactSharing(id: FactId, visibility: Visibility, category: string | null): Promise<void> {
    return this.invoke<void>(COMMANDS.setFactSharing, { id, visibility, category });
  }
  personaChat(prompt: string, history: ChatTurn[] = []): Promise<PersonaReply> {
    return this.invoke<PersonaReply>(COMMANDS.chat, { prompt, history });
  }
  personaDraft(req: DraftRequest): Promise<Draft> {
    return this.invoke<Draft>(COMMANDS.draft, { req });
  }
  loadChatHistory(): Promise<StoredTurn[]> {
    return this.invoke<StoredTurn[]>(COMMANDS.historyLoad);
  }
  saveChatHistory(turns: StoredTurn[]): Promise<void> {
    return this.invoke<void>(COMMANDS.historySave, { turns });
  }
}
