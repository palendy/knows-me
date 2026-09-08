// US-4.x example-based tests for the interview queue UI.

import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueueView } from "./QueueView";
import { MockInterviewApi } from "./mock-interview-api";
import type { InterviewApi } from "./api";
import type { QueueItem } from "../../shared/contracts";

const confirmItem: QueueItem = {
  id: "q-confirm",
  kind: {
    Confirm: {
      candidate: {
        title: "선호하는 배포 방식",
        body: "GitHub Actions로 자동 배포",
        provenance: { source: "Session", collected_at: "2026-01-01T00:00:00Z" },
        suggested_scope: "Company",
      },
    },
  },
  priority: 5,
  created_at: "2026-01-01T00:00:00Z",
  expires_at: null,
};

const deepenItem: QueueItem = {
  id: "q-deepen",
  kind: { Deepen: { question: "주로 어떤 언어를 쓰나요?", hypothesis: null } },
  priority: 3,
  created_at: "2026-01-02T00:00:00Z",
  expires_at: null,
};

describe("QueueView", () => {
  it("lists pending items with their prompts", async () => {
    render(<QueueView api={new MockInterviewApi([confirmItem, deepenItem])} />);

    expect(await screen.findByText("선호하는 배포 방식")).toBeInTheDocument();
    expect(screen.getByText("주로 어떤 언어를 쓰나요?")).toBeInTheDocument();
  });

  it("confirms a Confirm item and refreshes the list", async () => {
    const api = new MockInterviewApi([confirmItem]);
    const onChanged = vi.fn();
    render(<QueueView api={api} onChanged={onChanged} />);

    await screen.findByText("선호하는 배포 방식");
    await userEvent.click(screen.getByRole("button", { name: "확정" }));

    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    // Item left the queue → empty guidance shows.
    expect(
      await screen.findByText(/확인할 질문이 없습니다/),
    ).toBeInTheDocument();
  });

  it("saves a Deepen answer as text", async () => {
    const api = new MockInterviewApi([deepenItem]);
    const spy = vi.spyOn(api, "answer");
    render(<QueueView api={api} />);

    await screen.findByText("주로 어떤 언어를 쓰나요?");
    await userEvent.type(
      screen.getByRole("textbox", { name: "답변" }),
      "Rust와 TypeScript",
    );
    await userEvent.click(screen.getByRole("button", { name: "답변 저장" }));

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith("q-deepen", { Text: "Rust와 TypeScript" }),
    );
  });

  it("shows empty guidance when the queue is clear", async () => {
    render(<QueueView api={new MockInterviewApi([])} />);
    expect(
      await screen.findByText(/확인할 질문이 없습니다/),
    ).toBeInTheDocument();
  });

  it("surfaces an answer error without dropping the row", async () => {
    const api: InterviewApi = {
      async list() {
        return [confirmItem];
      },
      async answer() {
        throw new Error("locked: unlock required");
      },
    };
    render(<QueueView api={api} />);

    await screen.findByText("선호하는 배포 방식");
    await userEvent.click(screen.getByRole("button", { name: "확정" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "locked: unlock required",
    );
    expect(screen.getByText("선호하는 배포 방식")).toBeInTheDocument();
  });
});
