// Integration wiring test: once unlocked, the shell exposes every U3/U4 view as
// a tab and each renders through its mock port. This is the regression guard for
// the app-wiring work (the views existed but weren't mounted before).

import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UnlockedShell } from "./App";

describe("UnlockedShell (app wiring)", () => {
  it("shows a tab for every view", async () => {
    render(<UnlockedShell onLock={() => {}} />);
    const tablist = screen.getByRole("tablist", { name: "화면" });
    for (const label of [
      "대시보드",
      "대기열",
      "나와 대화",
      "설정",
    ]) {
      expect(within(tablist).getByRole("tab", { name: label })).toBeInTheDocument();
    }
    // Let the default dashboard tab settle so its async load doesn't warn.
    await screen.findByRole("heading", { name: "대시보드" });
    expect(within(tablist).queryByRole("tab", { name: "미니홈피" })).not.toBeInTheDocument();
    expect(within(tablist).queryByRole("tab", { name: "지식 그래프" })).not.toBeInTheDocument();
    expect(within(tablist).queryByRole("tab", { name: "소스" })).not.toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "나를 이루는 맥락" })).toBeInTheDocument();
    expect(await screen.findByRole("region", { name: "지식 그래프" })).toBeInTheDocument();
  });

  it("opens on the dashboard", async () => {
    render(<UnlockedShell onLock={() => {}} />);
    expect(await screen.findByRole("heading", { name: "대시보드" })).toBeInTheDocument();
  });

  it("switches to the persona chat tab", async () => {
    render(<UnlockedShell onLock={() => {}} />);
    // Let the dashboard finish loading before leaving it (avoids act warnings).
    await screen.findByRole("heading", { name: "대시보드" });
    await userEvent.click(screen.getByRole("tab", { name: "나와 대화" }));
    // PersonaChatView renders its chat surface; the dashboard heading is gone.
    expect(screen.queryByRole("heading", { name: "대시보드" })).not.toBeInTheDocument();
  });

  it("switches to the queue tab", async () => {
    render(<UnlockedShell onLock={() => {}} />);
    await screen.findByRole("heading", { name: "대시보드" });
    await userEvent.click(screen.getByRole("tab", { name: "대기열" }));
    expect(await screen.findByRole("heading", { name: "인터뷰 대기열" })).toBeInTheDocument();
  });

  it("keeps the U1 settings surface (lock button) on the settings tab", async () => {
    render(<UnlockedShell onLock={() => {}} />);
    await screen.findByRole("heading", { name: "대시보드" });
    await userEvent.click(screen.getByRole("tab", { name: "설정" }));
    expect(await screen.findByRole("button", { name: /잠그기|lock/i })).toBeInTheDocument();
  });

  it("keeps source ingestion available inside settings", async () => {
    render(<UnlockedShell onLock={() => {}} />);
    await screen.findByRole("heading", { name: "대시보드" });
    await userEvent.click(screen.getByRole("tab", { name: "설정" }));
    await userEvent.click(screen.getByRole("tab", { name: "연결 소스" }));
    expect(await screen.findByRole("button", { name: "전체 수집" })).toBeEnabled();
  });
});

