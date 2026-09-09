// US-1.x — source connection UI tests.

import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SourcesView } from "./SourcesView";
import { MockSourcesApi } from "./mock-sources-api";

describe("SourcesView", () => {
  it("renders the catalog with connection state", async () => {
    render(<SourcesView api={new MockSourcesApi()} />);

    // Credential-less sources are usable; Notion/Gmail need connecting.
    expect(await screen.findByText("에이전트 세션")).toBeInTheDocument();
    expect(screen.getByText("Notion")).toBeInTheDocument();
    expect(screen.getByText("Gmail")).toBeInTheDocument();
    // At least two "사용 가능" (Session, File) and two "연결 필요".
    expect(screen.getAllByText("사용 가능").length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByText("연결 필요").length).toBeGreaterThanOrEqual(2);
  });

  it("connects Notion through the form and flips it to connected", async () => {
    const api = new MockSourcesApi();
    const spy = vi.spyOn(api, "connectSource");
    render(<SourcesView api={api} />);

    await screen.findByText("Notion");
    const notionRow = screen.getByText("Notion").closest("li")!;
    await userEvent.click(within(notionRow).getByRole("button", { name: "연결" }));

    // Fill the token field in the dialog and submit.
    const dialog = await screen.findByRole("dialog", { name: "Notion 연결" });
    await userEvent.type(
      within(dialog).getByLabelText(/Integration 토큰/),
      "secret_abc",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "연결" }));

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith("Notion", { token: "secret_abc" }),
    );
    // Row now shows "연결됨" and a disconnect button (after refresh lands).
    await waitFor(() => {
      const updatedRow = screen.getByText("Notion").closest("li")!;
      expect(within(updatedRow).getByText("연결됨")).toBeInTheDocument();
    });
    const updatedRow = screen.getByText("Notion").closest("li")!;
    expect(
      within(updatedRow).getByRole("button", { name: "연결 해제" }),
    ).toBeInTheDocument();
  });

  it("blocks sync on an unconnected source", async () => {
    render(<SourcesView api={new MockSourcesApi()} />);

    const gmailRow = (await screen.findByText("Gmail")).closest("li")!;
    // The "수집" button is disabled until connected.
    expect(within(gmailRow).getByRole("button", { name: "수집" })).toBeDisabled();
  });

  it("disconnects a connected source", async () => {
    const api = new MockSourcesApi();
    await api.connectSource("Notion", { token: "secret_abc" });
    const spy = vi.spyOn(api, "disconnectSource");
    render(<SourcesView api={api} />);

    const notionRow = (await screen.findByText("Notion")).closest("li")!;
    await userEvent.click(
      within(notionRow).getByRole("button", { name: "연결 해제" }),
    );

    await waitFor(() => expect(spy).toHaveBeenCalledWith("Notion"));
    await waitFor(() => {
      const updatedRow = screen.getByText("Notion").closest("li")!;
      expect(within(updatedRow).getByText("연결 필요")).toBeInTheDocument();
    });
  });

  it("surfaces a connect error inside the dialog without closing it", async () => {
    const api = new MockSourcesApi();
    vi.spyOn(api, "connectSource").mockRejectedValue(
      new Error("locked: unlock required"),
    );
    render(<SourcesView api={api} />);

    const notionRow = (await screen.findByText("Notion")).closest("li")!;
    await userEvent.click(within(notionRow).getByRole("button", { name: "연결" }));

    const dialog = await screen.findByRole("dialog", { name: "Notion 연결" });
    await userEvent.type(
      within(dialog).getByLabelText(/Integration 토큰/),
      "secret_abc",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "연결" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "locked: unlock required",
    );
    // Dialog is still open.
    expect(
      screen.getByRole("dialog", { name: "Notion 연결" }),
    ).toBeInTheDocument();
  });
});
