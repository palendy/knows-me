import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HomeView } from "./HomeView";
import { MockSourcesApi } from "../sources/mock-sources-api";
import { ipc } from "../../shared/ipc";

vi.mock("../../shared/ipc", () => ({ ipc: { getConfig: vi.fn(), listTransfers: vi.fn(), setTransferPolicy: vi.fn(), lock: vi.fn() } }));
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(ipc.getConfig).mockResolvedValue({ transfer_policy: "MaskAndMinimize", llm_model: "test-model", server_enabled: false });
  vi.mocked(ipc.listTransfers).mockResolvedValue([]);
});
describe("settings", () => {
  it("preserves policy updates and vault locking", async () => {
    const user = userEvent.setup();
    const onLock = vi.fn();
    render(<HomeView onLock={onLock} sourcesApi={new MockSourcesApi()} />);
    await screen.findByText("test-model");
    await user.click(screen.getByRole("radio", { name: /기기 안에서만 사용/ }));
    expect(ipc.setTransferPolicy).toHaveBeenCalledWith("LocalOnlyNoLlm");
    await waitFor(() => expect(screen.getByRole("button", { name: "지금 잠그기" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "지금 잠그기" }));
    expect(ipc.lock).toHaveBeenCalledOnce();
    expect(onLock).toHaveBeenCalledOnce();
  });
  it("surfaces the source collection screen inside settings", async () => {
    render(<HomeView onLock={vi.fn()} initialTab="sources" sourcesApi={new MockSourcesApi()} />);
    expect(screen.getByRole("tab", { name: "연결 소스" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { name: "소스 수집" })).toBeInTheDocument();
    expect(screen.getByText(/Notion/)).toBeInTheDocument();
    expect(screen.getAllByText("연결 필요").length).toBeGreaterThan(0);
    await waitFor(() => expect(ipc.getConfig).toHaveBeenCalledOnce());
  });
  it("shows the recorded date, model, size and masked preview", async () => {
    vi.mocked(ipc.listTransfers).mockResolvedValue([{ at: "2026-09-09T10:30:00Z", purpose: "persona", model: "recorded-model", bytes_sent: 1234, masked_preview: "[이름]의 기록" }]);
    render(<HomeView onLock={vi.fn()} initialTab="transfers" sourcesApi={new MockSourcesApi()} />);
    expect(await screen.findByText("persona")).toBeInTheDocument();
    expect(screen.getByText(/recorded-model/)).toHaveTextContent("1,234 bytes");
    expect(screen.getByText("[이름]의 기록")).toBeInTheDocument();
    expect(document.querySelector("time")).toHaveAttribute("dateTime", "2026-09-09T10:30:00Z");
  });
});
