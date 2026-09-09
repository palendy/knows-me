// US-6.1 example-based tests.

import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PersonaChatView } from "./PersonaChatView";
import { FailingApi, MockApi, withOverrides } from "../u4-shared/mock-api";
import { EXAMPLE_QUESTIONS } from "./PersonaChatView";
import type { ChatTurn } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";

describe("PersonaChatView", () => {
  it("answers from confirmed context (AC1)", async () => {
    render(<PersonaChatView api={new MockApi()} />);

    await userEvent.type(screen.getByLabelText("질문"), "배포 절차");
    await userEvent.click(screen.getByRole("button", { name: "보내기" }));

    expect(await screen.findByText(/make deploy/)).toBeInTheDocument();
  });

  it("always tells the owner that identifiers are masked (AC2)", () => {
    render(<PersonaChatView api={new MockApi()} />);

    expect(screen.getByRole("note")).toHaveTextContent(/마스킹되어 전송/);
  });

  it("says so plainly when there is no confirmed grounding", async () => {
    // BR-P4 triggers on an empty confirmed set, not on an off-topic question:
    // with facts on hand the service grounds the answer and lets the model say
    // it does not know.
    render(<PersonaChatView api={new MockApi([], 0)} />);

    await userEvent.type(screen.getByLabelText("질문"), "존재하지 않는 주제");
    await userEvent.click(screen.getByRole("button", { name: "보내기" }));

    expect(
      await screen.findByText(/확정된 맥락이 없어 답변할 수 없습니다/),
    ).toBeInTheDocument();
  });

  it("refuses to send a blank prompt", async () => {
    render(<PersonaChatView api={new MockApi()} />);

    expect(screen.getByRole("button", { name: "보내기" })).toBeDisabled();
    await userEvent.type(screen.getByLabelText("질문"), "   ");
    expect(screen.getByRole("button", { name: "보내기" })).toBeDisabled();
  });

  it("keeps the conversation and offers a retry when the call fails", async () => {
    render(<PersonaChatView api={new FailingApi()} />);

    await userEvent.type(screen.getByLabelText("질문"), "배포 절차");
    await userEvent.click(screen.getByRole("button", { name: "보내기" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "외부 서비스에 연결할 수 없습니다",
    );
    // The owner's own message is still on screen.
    expect(screen.getByText("배포 절차")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "다시 시도" })).toBeInTheDocument();
  });

  it("recovers when a retry succeeds", async () => {
    let failNext = true;
    const api: KnowsMeApi = withOverrides(new MockApi(), {
      async personaChat(prompt: string) {
        if (failNext) {
          failNext = false;
          throw new Error("일시적 오류");
        }
        return { text: `복구됨: ${prompt}`, sources: [] };
      },
    });

    render(<PersonaChatView api={api} />);
    await userEvent.type(screen.getByLabelText("질문"), "배포 절차");
    await userEvent.click(screen.getByRole("button", { name: "보내기" }));

    await screen.findByRole("alert");
    await userEvent.click(screen.getByRole("button", { name: "다시 시도" }));

    await waitFor(() =>
      expect(screen.getByText("복구됨: 배포 절차")).toBeInTheDocument(),
    );
  });

  it("sends the prior turns with a follow-up", async () => {
    // Without this a follow-up like "그거 더 자세히" has no referent.
    const seen: ChatTurn[][] = [];
    const api = withOverrides(new MockApi(), {
      async personaChat(prompt: string, history: ChatTurn[] = []) {
        seen.push(history);
        return { text: `응답: ${prompt}`, sources: [] };
      },
    });

    render(<PersonaChatView api={api} />);
    const box = screen.getByLabelText("질문");

    await userEvent.type(box, "배포 절차");
    await userEvent.click(screen.getByRole("button", { name: "보내기" }));
    await screen.findByText("응답: 배포 절차");

    await userEvent.type(screen.getByLabelText(/이어서 질문/), "그거 더 자세히");
    await userEvent.click(screen.getByRole("button", { name: "보내기" }));
    await screen.findByText("응답: 그거 더 자세히");

    expect(seen[0]).toEqual([]);
    expect(seen[1]).toEqual([
      { role: "Owner", text: "배포 절차" },
      { role: "Persona", text: "응답: 배포 절차" },
    ]);
  });

  it("shows which facts the answer was grounded in", async () => {
    render(<PersonaChatView api={new MockApi()} />);

    await userEvent.type(screen.getByLabelText("질문"), "배포");
    await userEvent.click(screen.getByRole("button", { name: "보내기" }));

    const disclosure = await screen.findByText(/근거로 삼은 사실 \d+개/);
    expect(disclosure).toBeInTheDocument();
    await userEvent.click(disclosure);
    expect(screen.getByText("배포 절차")).toBeInTheDocument();
  });

  it("offers example questions before the first turn", async () => {
    render(<PersonaChatView api={new MockApi()} />);

    const example = screen.getByRole("button", { name: "내 배포 절차 알려줘" });
    expect(example).toBeInTheDocument();
  });

  it("answers every example question from the sample context", async () => {
    // Regression guard: the mock used to match the whole query as one
    // substring, so clicking any example answered "no context" and the demo
    // path looked broken while CI stayed green.
    for (const question of EXAMPLE_QUESTIONS) {
      const { unmount } = render(<PersonaChatView api={new MockApi()} />);

      await userEvent.click(screen.getByRole("button", { name: question }));

      // A persona turn was appended...
      expect(await screen.findByText("knows me")).toBeInTheDocument();
      // ...and it is a grounded answer, not the no-context fallback.
      expect(screen.queryByText(/확정된 맥락이 없어/)).not.toBeInTheDocument();

      unmount();
    }
  });
});
