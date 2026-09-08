// The Interview queue port (US-4.x, U3-owned surface).
//
// Kept separate from `KnowsMeApi` (the read-view/persona port) because the queue
// is the *write* side of the knowledge base: answering an item confirms or
// deepens a fact. Views depend only on the port, so the same QueueView drives a
// `MockInterviewApi` in tests and the Tauri command layer in the app.

import type {
  AnswerInput,
  AnswerResult,
  QueueItem,
  QueueItemId,
  QueueSort,
} from "../../shared/contracts";

export interface InterviewApi {
  /** Pending queue items (expired ones already filtered by U3). */
  list(sort: QueueSort): Promise<QueueItem[]>;
  /** Answer an item: confirm/reject (`Choice`), correct/reply (`Text`), or hold (`Skip`). */
  answer(id: QueueItemId, input: AnswerInput): Promise<AnswerResult>;
}
