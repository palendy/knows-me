// US-5.2 example-based tests.

import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { MiniHomeView } from "./MiniHomeView";
import { FailingApi, MockApi } from "../u4-shared/mock-api";

describe("MiniHomeView", () => {
  it("renders confirmed context as highlight cards (AC1)", async () => {
    render(<MiniHomeView api={new MockApi()} />);

    expect(await screen.findByText("배포 절차")).toBeInTheDocument();
    expect(screen.getByText("코드 리뷰 규칙")).toBeInTheDocument();
  });

  it("leads with the most connected fact", async () => {
    render(<MiniHomeView api={new MockApi()} />);

    const items = await screen.findAllByRole("listitem");
    expect(items[0]).toHaveTextContent("배포 절차");
  });

  it("never shows an unconfirmed candidate", async () => {
    render(<MiniHomeView api={new MockApi()} />);

    await screen.findByText("배포 절차");
    expect(screen.queryByText("미확정 후보")).not.toBeInTheDocument();
  });

  it("points at the interview queue when there is nothing confirmed", async () => {
    render(<MiniHomeView api={new MockApi([], 0)} />);

    expect(
      await screen.findByText(/인터뷰 Queue에서 질문에 답하면/),
    ).toBeInTheDocument();
  });

  it("shows an error state rather than an empty grid", async () => {
    render(<MiniHomeView api={new FailingApi()} />);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });
});
