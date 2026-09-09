// US-5.1 example-based tests (PBT-10: properties cover the general rules,
// these pin the concrete behaviour the story asks for).

import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DashboardView } from "./DashboardView";
import { FailingApi, MockApi, withOverrides } from "../u4-shared/mock-api";
import type { KnowsMeApi } from "../u4-shared/api";
import type { DashboardDto } from "../../shared/contracts";

describe("DashboardView", () => {
  it("includes representative context and the interactive knowledge graph", async () => {
    render(<DashboardView api={new MockApi()} />);
    const context = await screen.findByRole("region", { name: "나를 이루는 맥락" });
    expect(await within(context).findByText("배포 절차")).toBeInTheDocument();
    const graph = screen.getByRole("region", { name: "지식 그래프" });
    const node = await within(graph).findByRole("button", { name: "배포 절차" });
    await userEvent.click(node);
    expect(node).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText(/최근 확정 사실 \d+개 기준/)).toBeInTheDocument();
  });

  it("shows collection status, pending queue and recent facts (AC1)", async () => {
    render(<DashboardView api={new MockApi()} />);

    expect(await screen.findByText("수집 현황")).toBeInTheDocument();
    expect(screen.getByText("대기 중인 질문")).toBeInTheDocument();
    expect(screen.getByText("최근 확정 사실")).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "대기 중인 질문" })).getByText("3")).toBeInTheDocument();
  });

  it("reflects the latest numbers when refreshed (AC2)", async () => {
    let collected = 1;
    const api: KnowsMeApi = withOverrides(new MockApi(), {
      async getDashboard(): Promise<DashboardDto> {
        return {
          collected_count: collected++,
          pending_queue: 0,
          recent_facts: [],
        };
      },
    });

    render(<DashboardView api={api} />);
    expect(await screen.findByRole("group", { name: "수집 현황" })).toHaveTextContent("1개");

    await userEvent.click(screen.getByRole("button", { name: "새로고침" }));
    await waitFor(() => expect(screen.getByRole("group", { name: "수집 현황" })).toHaveTextContent("2개"));
  });

  it("guides the owner when nothing has been collected yet", async () => {
    const api = new MockApi([], 0);
    render(<DashboardView api={api} />);

    expect(
      await screen.findByText(/아직 수집된 내용이 없습니다/),
    ).toBeInTheDocument();
  });

  it("surfaces an error with a retry instead of a blank panel", async () => {
    render(<DashboardView api={new FailingApi()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "외부 서비스에 연결할 수 없습니다",
    );
    expect(screen.getByRole("button", { name: "다시 시도" })).toBeInTheDocument();
  });
});
