// U4 data-access port.
//
// Views depend on this interface and nothing else, so swapping the development
// mock for U1's real Tauri commands is a one-line change at the injection site
// (U4-NFR-M5). Types come from U1's shared contract mirror — U4 does not
// redefine them.

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

export interface KnowsMeApi {
  /** US-5.1 */
  getDashboard(): Promise<DashboardDto>;
  /** US-5.2 */
  getMiniHome(limit?: number): Promise<MiniHomeDto>;
  /** US-5.3 */
  getGraph(filter: GraphFilter): Promise<GraphDto>;
  /** US-5.3 — the full page record, for the wiki inspector's sharing control. */
  getFact(id: FactId): Promise<Fact>;
  /**
   * US-5.3 — set a page's sharing state: its visibility and (normalized)
   * category. `category` null clears it. This is the owner's "approve-share +
   * assign-category" gate — the single place a page becomes reachable by a
   * consumer token (`Shared` AND a granted category).
   */
  setFactSharing(id: FactId, visibility: Visibility, category: string | null): Promise<void>;
  /** US-6.1. `history` carries prior turns, oldest first. */
  personaChat(prompt: string, history?: ChatTurn[]): Promise<PersonaReply>;
  /** US-6.2 */
  personaDraft(req: DraftRequest): Promise<Draft>;
  /**
   * The saved conversation, oldest first. Lives in the encrypted vault, not
   * browser storage — it is the owner's context in the clear.
   */
  loadChatHistory(): Promise<StoredTurn[]>;
  saveChatHistory(turns: StoredTurn[]): Promise<void>;
}
