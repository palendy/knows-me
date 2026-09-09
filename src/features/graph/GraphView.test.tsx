// US-5.3 example-based tests.

import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GraphView } from "./GraphView";
import { FailingApi, MockApi } from "../u4-shared/mock-api";

describe("GraphView", () => {
  it("draws facts as nodes and links as edges (AC1)", async () => {
    const { container } = render(<GraphView api={new MockApi()} />);

    await screen.findByRole("img", { name: /사실 \d+개, 연결 \d+개/ });
    expect(container.querySelectorAll("circle").length).toBeGreaterThan(0);
    expect(container.querySelectorAll("line").length).toBeGreaterThan(0);
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
});
