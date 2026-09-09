import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SharingSettings } from "./SharingSettings";
import { ipc } from "../../shared/ipc";

vi.mock("../../shared/ipc", () => ({
  ipc: {
    shareStatus: vi.fn(),
    setSharingEnabled: vi.fn(),
    listShareCategories: vi.fn(),
    issueShareToken: vi.fn(),
    revokeShareToken: vi.fn(),
    listShareTokens: vi.fn(),
    startShareTunnel: vi.fn(),
    stopShareTunnel: vi.fn(),
  },
}));

const enabledStatus = {
  enabled: true,
  owner_port: 8766,
  shared_port: 8767,
  tunnel_url: null,
  cloudflared_installed: true,
};

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(ipc.shareStatus).mockResolvedValue(enabledStatus);
  vi.mocked(ipc.listShareCategories).mockResolvedValue(["deploy", "payment"]);
  vi.mocked(ipc.listShareTokens).mockResolvedValue([]);
  vi.mocked(ipc.setSharingEnabled).mockResolvedValue();
  vi.mocked(ipc.revokeShareToken).mockResolvedValue(true);
});

describe("SharingSettings", () => {
  it("issues a token for a picked category and surfaces the one-time secret", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.issueShareToken).mockResolvedValue({ id: "지빈", secret: "s3cr3t-once" });
    render(<SharingSettings />);

    // The issuance form appears once the (enabled) status loads.
    await screen.findByText("공유할 범주");
    await user.type(screen.getByPlaceholderText("예: 지빈"), "지빈");
    await user.click(screen.getByRole("checkbox", { name: "deploy" }));
    await user.click(screen.getByRole("button", { name: "발급" }));

    expect(ipc.issueShareToken).toHaveBeenCalledWith("지빈", ["deploy"]);
    // The secret is shown verbatim, exactly once, for the owner to copy now.
    expect(await screen.findByText("s3cr3t-once")).toBeInTheDocument();
  });

  it("revokes a listed token by its label", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.listShareTokens).mockResolvedValue([
      { id: "지빈", granted: ["deploy"], issued_at: "2026-09-09T10:00:00Z" },
    ]);
    render(<SharingSettings />);

    await user.click(await screen.findByRole("button", { name: "폐기" }));
    expect(ipc.revokeShareToken).toHaveBeenCalledWith("지빈");
  });

  it("starts a tunnel and shows the MCP endpoint", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.shareStatus)
      .mockResolvedValueOnce(enabledStatus)
      .mockResolvedValue({ ...enabledStatus, tunnel_url: "https://calm-band.trycloudflare.com" });
    vi.mocked(ipc.startShareTunnel).mockResolvedValue("https://calm-band.trycloudflare.com");
    render(<SharingSettings />);

    await user.click(await screen.findByRole("button", { name: "터널 시작" }));
    expect(ipc.startShareTunnel).toHaveBeenCalledOnce();
    expect(
      await screen.findByText("https://calm-band.trycloudflare.com/mcp"),
    ).toBeInTheDocument();
  });

  it("offers no tunnel start when cloudflared is not installed", async () => {
    vi.mocked(ipc.shareStatus).mockResolvedValue({
      ...enabledStatus,
      cloudflared_installed: false,
    });
    render(<SharingSettings />);

    // The button renders but is disabled, and the install hint is shown.
    const start = await screen.findByRole("button", { name: "터널 시작" });
    expect(start).toBeDisabled();
    expect(screen.getByText(/cloudflared가 설치/)).toBeInTheDocument();
  });
});
