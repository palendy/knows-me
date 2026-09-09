// US-5.3 example-based tests.

import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Fact } from "../../shared/contracts";
import { GraphView } from "./GraphView";
import { FailingApi, MockApi, withOverrides } from "../u4-shared/mock-api";

describe("GraphView", () => {
  it("draws facts as nodes and links as edges (AC1)", async () => {
    render(<GraphView api={new MockApi()} />);

    // The force canvas has no accessibility tree, so nodes surface as buttons
    // in the visually-hidden list and the graph reports its size via role=img.
    const graph = await screen.findByRole("img", { name: /사실 \d+개, 연결 \d+개/ });
    expect(graph.getAttribute("aria-label")).toMatch(/사실 [1-9]\d*개, 연결 [1-9]\d*개/);
    expect(screen.getByRole("button", { name: "배포 절차" })).toBeInTheDocument();
  });

  it("highlights the selection and its neighbours on click", async () => {
    render(<GraphView api={new MockApi()} />);

    const node = await screen.findByRole("button", { name: "배포 절차" });
    await userEvent.click(node);

    expect(node).toHaveAttribute("aria-pressed", "true");
    expect(await screen.findByText(/직접 연결된 항목/)).toBeInTheDocument();
  });

  it("can be operated from the keyboard", async () => {
    render(<GraphView api={new MockApi()} />);

    const node = await screen.findByRole("button", { name: "코드 리뷰 규칙" });
    node.focus();
    await userEvent.keyboard("{Enter}");

    expect(node).toHaveAttribute("aria-pressed", "true");
  });

  it("clears the selection when the same node is clicked again", async () => {
    render(<GraphView api={new MockApi()} />);

    const node = await screen.findByRole("button", { name: "배포 절차" });
    await userEvent.click(node);
    await userEvent.click(node);

    expect(node).toHaveAttribute("aria-pressed", "false");
  });

  it("filters by scope", async () => {
    render(<GraphView api={new MockApi()} />);

    await screen.findByRole("button", { name: "배포 절차" });
    await userEvent.selectOptions(screen.getByLabelText(/범위/), "Company");

    // 배포 절차 is fact #1 -> Personal, so it must disappear under Company.
    expect(
      await screen.findByRole("button", { name: "코드 리뷰 규칙" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "배포 절차" }),
    ).not.toBeInTheDocument();
  });

  it("explains an empty graph instead of showing a blank canvas", async () => {
    render(<GraphView api={new MockApi([], 0)} />);
    expect(await screen.findByText(/표시할 사실이 없습니다/)).toBeInTheDocument();
  });

  it("shows an error state", async () => {
    render(<GraphView api={new FailingApi()} />);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });

  it("publishes a selected page with a category from the inspector", async () => {
    const calls: Array<[string, string, string | null]> = [];
    const api = withOverrides(new MockApi(), {
      setFactSharing: async (id, visibility, category) => {
        calls.push([id, visibility, category]);
      },
    });
    render(<GraphView api={api} />);

    // 배포 절차 (fact #1) carries the topic "deploy", which the control offers
    // as a category choice.
    await userEvent.click(await screen.findByRole("button", { name: "배포 절차" }));

    await userEvent.click(await screen.findByRole("radio", { name: "공개" }));
    await userEvent.selectOptions(screen.getByLabelText("공유 범주"), "deploy");
    await userEvent.click(screen.getByRole("button", { name: "저장" }));

    expect(await screen.findByText("저장되었습니다.")).toBeInTheDocument();
    expect(calls).toContainEqual([
      "00000000-0000-4000-8000-000000000001",
      "Shared",
      "deploy",
    ]);
  });

  it("blocks publishing a page that has no topic to categorize", async () => {
    render(<GraphView api={new MockApi()} />);

    // 커피 취향 (fact #5) has no topics, so it cannot be given a share category.
    await userEvent.click(await screen.findByRole("button", { name: "커피 취향" }));
    await userEvent.click(await screen.findByRole("radio", { name: "공개" }));

    expect(
      screen.getByText(/주제 태그가 없어 공유 범주를 지정할 수 없습니다/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "저장" })).toBeDisabled();
  });

  it("keeps a stored category visible even when it left the page's topics", async () => {
    // A page shared under "legacy", whose topics later no longer include it.
    const custom: Fact = {
      id: "00000000-0000-4000-8000-0000000000aa",
      title: "레거시 페이지",
      body: "b",
      links: [],
      metadata: {
        provenance: { source: "Session", collected_at: "2026-09-01T09:00:00Z" },
        confirmed: true,
        scope: "Company",
        confirmed_at: "2026-09-01T09:00:00Z",
        topics: ["deploy"],
        visibility: "Shared",
        category: "legacy",
      },
    };
    const calls: Array<[string, string, string | null]> = [];
    const api = withOverrides(new MockApi([custom]), {
      setFactSharing: async (id, visibility, category) => {
        calls.push([id, visibility, category]);
      },
    });
    render(<GraphView api={api} />);

    await userEvent.click(await screen.findByRole("button", { name: "레거시 페이지" }));

    const select = (await screen.findByLabelText("공유 범주")) as HTMLSelectElement;
    // "legacy" is offered and selected, even though topics only has "deploy".
    expect(select.value).toBe("legacy");
    expect(within(select).getByRole("option", { name: "legacy" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "저장" }));
    expect(calls).toContainEqual(["00000000-0000-4000-8000-0000000000aa", "Shared", "legacy"]);
  });
});
