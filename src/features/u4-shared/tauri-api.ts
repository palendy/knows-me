// Tauri adapter — the integration seam with U1's command layer.
//
// U1 owns `CommandRouter` and registers the five commands named below. Until
// that lands this adapter is unused (views are constructed with `MockApi`);
// switching over is a one-line change at the injection site, which is the whole
// point of the port (U4-NFR-M5).

import type {
  ChatTurn,
  DashboardDto,
  Draft,
  DraftRequest,
  GraphDto,
  GraphFilter,
  MiniHomeDto,
  PersonaReply,
} from "../../shared/contracts";
import type { KnowsMeApi } from "./api";

/** Signature of Tauri's `invoke`, injected rather than imported. */
export type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/** Command names U4 expects U1 to register. */
export const COMMANDS = {
  dashboard: "get_dashboard",
  minihome: "get_minihome",
  graph: "get_graph",
  chat: "persona_chat",
  draft: "persona_draft",
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
  personaChat(prompt: string, history: ChatTurn[] = []): Promise<PersonaReply> {
    return this.invoke<PersonaReply>(COMMANDS.chat, { prompt, history });
  }
  personaDraft(req: DraftRequest): Promise<Draft> {
    return this.invoke<Draft>(COMMANDS.draft, { req });
  }
}
