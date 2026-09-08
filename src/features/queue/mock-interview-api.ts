// In-memory Interview queue for standalone UI dev and tests. Confirm items
// become facts on affirmative Choice or a Text correction; Deepen items become
// facts from the Text answer. Reject/Skip mirror the Rust rules (IR-1/IR-3).

import type {
  AnswerInput,
  AnswerResult,
  QueueItem,
  QueueItemId,
  QueueSort,
} from "../../shared/contracts";
import type { InterviewApi } from "./api";

const NEGATIVE = new Set([
  "no",
  "n",
  "reject",
  "false",
  "x",
  "아니오",
  "아니요",
  "아니",
  "아님",
]);

function isAffirmative(choice: string): boolean {
  const c = choice.trim().toLowerCase();
  return c.length > 0 && !NEGATIVE.has(c);
}

export class MockInterviewApi implements InterviewApi {
  private items: QueueItem[];

  constructor(seed: QueueItem[] = []) {
    this.items = [...seed];
  }

  async list(sort: QueueSort): Promise<QueueItem[]> {
    const out = [...this.items];
    if (sort === "PriorityDesc") {
      out.sort((a, b) => b.priority - a.priority);
    } else {
      out.sort((a, b) => b.created_at.localeCompare(a.created_at));
    }
    return out;
  }

  async answer(id: QueueItemId, input: AnswerInput): Promise<AnswerResult> {
    const item = this.items.find((i) => i.id === id);
    if (!item) throw new Error(`queue item ${id} not found`);

    if (input === "Skip") {
      return { confirmed_fact: null, follow_ups: [] };
    }
    if ("Choice" in input && input.Choice.trim() === "") {
      throw new Error("answer choice must not be empty");
    }
    if ("Text" in input && input.Text.trim() === "") {
      throw new Error("answer text must not be empty");
    }

    // Answered items leave the queue (reject discards without a fact).
    this.items = this.items.filter((i) => i.id !== id);

    let confirmed: AnswerResult["confirmed_fact"] = null;
    if ("Confirm" in item.kind) {
      const c = item.kind.Confirm.candidate;
      const affirm = "Choice" in input ? isAffirmative(input.Choice) : true;
      if (affirm) {
        confirmed = {
          id: `mock-${id}`,
          title: c.title,
          body: "Text" in input ? input.Text : c.body,
          links: [],
          metadata: {
            provenance: c.provenance,
            confirmed: true,
            scope: c.suggested_scope,
            confirmed_at: item.created_at,
          },
        };
      }
    } else {
      const text =
        "Text" in input ? input.Text : "Choice" in input ? input.Choice : "";
      confirmed = {
        id: `mock-${id}`,
        title: item.kind.Deepen.question,
        body: text,
        links: [],
        metadata: {
          provenance: { source: "Session", collected_at: item.created_at },
          confirmed: true,
          scope: "Unknown",
          confirmed_at: item.created_at,
        },
      };
    }
    return { confirmed_fact: confirmed, follow_ups: [] };
  }
}
