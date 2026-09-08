// Tauri adapter for the Interview queue port — talks to the `queue_*` commands
// registered in `desktop/src/main.rs`. Mirrors the U4 TauriApi pattern: `invoke`
// is injected rather than imported so the class stays testable.

import type {
  AnswerInput,
  AnswerResult,
  QueueItem,
  QueueItemId,
  QueueSort,
} from "../../shared/contracts";
import type { InterviewApi } from "./api";

export type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export const QUEUE_COMMANDS = {
  list: "queue_list",
  answer: "queue_answer",
} as const;

export class TauriInterviewApi implements InterviewApi {
  constructor(private readonly invoke: Invoke) {}

  list(sort: QueueSort): Promise<QueueItem[]> {
    return this.invoke<QueueItem[]>(QUEUE_COMMANDS.list, { sort });
  }
  answer(id: QueueItemId, input: AnswerInput): Promise<AnswerResult> {
    return this.invoke<AnswerResult>(QUEUE_COMMANDS.answer, { id, input });
  }
}
