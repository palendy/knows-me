// US-1.x — source connection card UI tests.

import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SourcesView } from "./SourcesView";
import { MockSourcesApi } from "./mock-sources-api";

/** Find a card by its title text. */
function card(title: string): HTMLElement {
  return screen.getByText(title).closest(".source-card") as HTMLElement;
}

describe("SourcesView", () => {
  it("renders the card catalog with Claude/Codex split and coming-soon cards", async () => {
    render(<SourcesView api={new MockSourcesApi()} />);

    expect(await screen.findByText("Claude")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Notion")).toBeInTheDocument();
    expect(screen.getByText("Gmail")).toBeInTheDocument();
    expect(screen.getByText("Confluence")).toBeInTheDocument();
    expect(screen.getByText("Jira")).toBeInTheDocument();
    expect(screen.getByText("Knox Mail")).toBeInTheDocument();
  });

  it("dims not-ready and coming-soon cards", async () => {
    render(<SourcesView api={new MockSourcesApi()} />);
    await screen.findByText("Claude");

    expect(card("Claude").className).not.toContain("is-dimmed");
    expect(card("Notion").className).toContain("is-dimmed");
    expect(card("Jira").className).toContain("is-dimmed");
    // Coming-soon toggle is disabled.
    expect(within(card("Jira")).getByRole("switch")).toBeDisabled();
  });

  it("credential-less sources have no connect toggle", async () => {
    render(<SourcesView api={new MockSourcesApi()} />);
    await screen.findByText("Claude");
    expect(within(card("Claude")).queryByRole("switch")).toBeNull();
  });

  it("connects Notion via the toggle and flips it on", async () => {
    const api = new MockSourcesApi();
    const spy = vi.spyOn(api, "connectSource");
    render(<SourcesView api={api} />);

    await screen.findByText("Notion");
    const toggle = within(card("Notion")).getByRole("switch");
    expect(toggle).toHaveAttribute("aria-checked", "false");
    await userEvent.click(toggle);

    const dialog = await screen.findByRole("dialog", { name: "Notion 연결" });
    // The connect guide is shown.
    expect(within(dialog).getByText("연결 방법")).toBeInTheDocument();
    await userEvent.type(
      within(dialog).getByLabelText(/Integration 토큰/),
      "secret_abc",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "연결" }));

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith("Notion", { token: "secret_abc" }),
    );
    await waitFor(() =>
      expect(within(card("Notion")).getByRole("switch")).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    expect(within(card("Notion")).getByText("연결됨")).toBeInTheDocument();
  });

  it("shows the connect note (e.g. reachable scope) before closing", async () => {
    const api = new MockSourcesApi();
    vi.spyOn(api, "connectSource").mockResolvedValue(
      "토큰은 유효하지만 연결된 페이지가 없습니다. 최상위 페이지에 연결하세요.",
    );
    render(<SourcesView api={api} />);

    await screen.findByText("Notion");
    await userEvent.click(within(card("Notion")).getByRole("switch"));
    const dialog = await screen.findByRole("dialog", { name: "Notion 연결" });
    await userEvent.type(
      within(dialog).getByLabelText(/Integration 토큰/),
      "secret_abc",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "연결" }));

    // The note is shown; dialog stays open with a 완료 button.
    expect(
      await within(dialog).findByText(/연결된 페이지가 없습니다/),
    ).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "완료" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Notion 연결" })).toBeNull(),
    );
  });

  it("edits a connected source through the pencil button", async () => {
    const api = new MockSourcesApi();
    await api.connectSource("Notion", { token: "secret_abc" });
    render(<SourcesView api={api} />);

    await screen.findByText("Notion");
    await userEvent.click(
      within(card("Notion")).getByRole("button", { name: /설정 수정/ }),
    );
    expect(
      await screen.findByRole("dialog", { name: "Notion 연결" }),
    ).toBeInTheDocument();
  });

  it("disconnects via the toggle when already on", async () => {
    const api = new MockSourcesApi();
    await api.connectSource("Notion", { token: "secret_abc" });
    const spy = vi.spyOn(api, "disconnectSource");
    render(<SourcesView api={api} />);

    await screen.findByText("Notion");
    const toggle = within(card("Notion")).getByRole("switch");
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await userEvent.click(toggle);

    await waitFor(() => expect(spy).toHaveBeenCalledWith("Notion"));
    await waitFor(() =>
      expect(within(card("Notion")).getByRole("switch")).toHaveAttribute(
        "aria-checked",
        "false",
      ),
    );
  });

  it("blocks sync on an unconnected source", async () => {
    render(<SourcesView api={new MockSourcesApi()} />);
    await screen.findByText("Gmail");
    expect(within(card("Gmail")).getByRole("button", { name: "수집" })).toBeDisabled();
  });

  it("shows the sync result in a toast labeled with the source that ran", async () => {
    render(<SourcesView api={new MockSourcesApi()} />);
    await screen.findByText("Claude");
    await userEvent.click(within(card("Claude")).getByRole("button", { name: "수집" }));

    // The result rides in a toast (mock returns 12 collected), tagged "Claude".
    await waitFor(() => {
      const toast = screen.getByRole("status");
      expect(toast).toHaveTextContent(/12건 수집/);
      expect(toast).toHaveTextContent("Claude");
    });
    // It is not rendered inside any card, so it can't grow the grid row.
    expect(within(card("Claude")).queryByText(/12건 수집/)).toBeNull();
    expect(within(card("Codex")).queryByText(/12건 수집/)).toBeNull();
  });

  it("shows a determinate progress bar in the toast while syncing", async () => {
    render(<SourcesView api={new MockSourcesApi()} />);
    await screen.findByText("Claude");
    await userEvent.click(within(card("Claude")).getByRole("button", { name: "수집" }));

    // Mock emits done/total ticks; the toast shows "수집 중… N/12".
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(/수집 중… \d+\/12/),
    );
    // Eventually completes with the collected summary.
    await waitFor(
      () => expect(screen.getByRole("status")).toHaveTextContent(/12건 수집/),
      { timeout: 2000 },
    );
  });

  it("distinguishes an error cause from a clean empty run", async () => {
    const api = new MockSourcesApi();
    vi.spyOn(api, "triggerIngest").mockResolvedValue({
      collected: 0,
      skipped: 0,
      errors: 1,
      remaining: 0,
      error_messages: ["Notion: notion search 401: unauthorized"],
      facts_created: 0,
      queue_items_created: 0,
      filtered: 0,
    });
    render(<SourcesView api={api} />);
    await screen.findByText("Claude");
    await userEvent.click(within(card("Claude")).getByRole("button", { name: "수집" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/401: unauthorized/);
  });

  it("surfaces a connect error inside the dialog without closing it", async () => {
    const api = new MockSourcesApi();
    vi.spyOn(api, "connectSource").mockRejectedValue(
      new Error("locked: unlock required"),
    );
    render(<SourcesView api={api} />);

    await screen.findByText("Notion");
    await userEvent.click(within(card("Notion")).getByRole("switch"));

    const dialog = await screen.findByRole("dialog", { name: "Notion 연결" });
    await userEvent.type(
      within(dialog).getByLabelText(/Integration 토큰/),
      "secret_abc",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "연결" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "locked: unlock required",
    );
    expect(
      screen.getByRole("dialog", { name: "Notion 연결" }),
    ).toBeInTheDocument();
  });
});
