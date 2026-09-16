import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HomeView } from "./HomeView";
import { MockSourcesApi } from "../sources/mock-sources-api";
import { ipc } from "../../shared/ipc";

vi.mock("../../shared/ipc", () => ({ ipc: { getConfig: vi.fn(), listTransfers: vi.fn(), setTransferPolicy: vi.fn(), setLlmConfig: vi.fn(), discoverClaudeInstalls: vi.fn(), setServerEnabled: vi.fn(), lock: vi.fn() } }));
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(ipc.getConfig).mockResolvedValue({
    transfer_policy: "MaskAndMinimize",
    server_enabled: false,
    sharing_enabled: false,
    llm_provider: "claude-cli",
    llm_model: "test-model",
    llm_base_url: null,
    llm_binary: null,
    llm_headers: null,
    llm_label: "test-model (로컬 Claude Code)",
    has_api_key: false,
  });
  vi.mocked(ipc.listTransfers).mockResolvedValue([]);
  vi.mocked(ipc.discoverClaudeInstalls).mockResolvedValue([]);
});
describe("settings", () => {
  it("preserves policy updates and vault locking", async () => {
    const user = userEvent.setup();
    const onLock = vi.fn();
    render(<HomeView onLock={onLock} sourcesApi={new MockSourcesApi()} />);
    await screen.findByText(/현재 사용 중/);
    await user.click(screen.getByRole("radio", { name: /기기 안에서만 사용/ }));
    expect(ipc.setTransferPolicy).toHaveBeenCalledWith("LocalOnlyNoLlm");
    await waitFor(() => expect(screen.getByRole("button", { name: "지금 잠그기" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "지금 잠그기" }));
    expect(ipc.lock).toHaveBeenCalledOnce();
    expect(onLock).toHaveBeenCalledOnce();
  });
  it("saves an LLM provider/model change with a write-only key", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.setLlmConfig).mockResolvedValue();
    render(<HomeView onLock={vi.fn()} sourcesApi={new MockSourcesApi()} />);
    await screen.findByText(/현재 사용 중/);
    // Switch to the Anthropic HTTP backend — the key/base-url fields appear.
    await user.click(screen.getByRole("radio", { name: /Anthropic API/ }));
    // The key field appears only for HTTP providers; find it by placeholder
    // (no stored key yet → the "sk-..." hint).
    const key = await screen.findByPlaceholderText("sk-...");
    await user.type(key, "sk-secret");
    await user.click(screen.getByRole("button", { name: "저장" }));
    expect(ipc.setLlmConfig).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "anthropic", model: "test-model", api_key: "sk-secret" }),
    );
  });
  it("keeps the stored key when the field is left blank", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.getConfig).mockResolvedValue({
      transfer_policy: "MaskAndMinimize", server_enabled: false, sharing_enabled: false,
      llm_provider: "anthropic", llm_model: "claude-opus-5", llm_base_url: null, llm_binary: null,
    llm_headers: null,
      llm_label: "claude-opus-5 (Anthropic)", has_api_key: true,
    });
    vi.mocked(ipc.setLlmConfig).mockResolvedValue();
    render(<HomeView onLock={vi.fn()} sourcesApi={new MockSourcesApi()} />);
    await screen.findByText(/claude-opus-5/);
    // The save button appears only on a change; edit the model, leave the key blank.
    await user.type(screen.getByRole("textbox", { name: "모델 이름" }), "-x");
    await user.click(screen.getByRole("button", { name: "저장" }));
    // Blank key field → null, so the backend keeps the stored secret.
    expect(ipc.setLlmConfig).toHaveBeenCalledWith(expect.objectContaining({ api_key: null }));
  });
  it("sends extra gateway headers with the URL backends and drops them for the CLI", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.setLlmConfig).mockResolvedValue();
    render(<HomeView onLock={vi.fn()} sourcesApi={new MockSourcesApi()} />);
    await screen.findByText(/현재 사용 중/);

    // The CLI backend makes no HTTP call, so it has no header field at all.
    expect(screen.queryByRole("textbox", { name: /추가 헤더/ })).toBeNull();

    await user.click(screen.getByRole("radio", { name: /Anthropic API/ }));
    const field = await screen.findByRole("textbox", { name: /추가 헤더/ });
    await user.type(field, "api-key: abc123");
    await user.click(screen.getByRole("button", { name: "저장" }));

    expect(ipc.setLlmConfig).toHaveBeenCalledWith(
      expect.objectContaining({ headers: "api-key: abc123" }),
    );
  });

  it("shows the backend's reason when a header block is rejected", async () => {
    // A malformed block is rejected with the offending line; a generic
    // "저장하지 못했습니다" would leave the owner rereading the whole field.
    const user = userEvent.setup();
    vi.mocked(ipc.setLlmConfig).mockRejectedValue(
      new Error("invalid input: 1번째 줄: `이름: 값` 형식이 아닙니다"),
    );
    render(<HomeView onLock={vi.fn()} sourcesApi={new MockSourcesApi()} />);
    await screen.findByText(/현재 사용 중/);

    await user.click(screen.getByRole("radio", { name: /Anthropic API/ }));
    await user.type(await screen.findByRole("textbox", { name: /추가 헤더/ }), "api-key abc");
    await user.click(screen.getByRole("button", { name: "저장" }));

    expect(await screen.findByText(/1번째 줄/)).toBeInTheDocument();
  });

  it("reports the live backend, not the chosen one, when they differ", async () => {
    // The label comes from the running client. A session that fell back to the
    // canned client must say so even though the config still names a model.
    vi.mocked(ipc.getConfig).mockResolvedValue({
      transfer_policy: "MaskAndMinimize", server_enabled: false, sharing_enabled: false,
      llm_provider: "openai", llm_model: "google/gemma-4-12b",
      llm_base_url: "http://localhost:1234/v1", llm_binary: null, llm_headers: null,
      llm_label: "오프라인 (LLM 미연결 — 고정 응답)", has_api_key: false,
    });
    render(<HomeView onLock={vi.fn()} sourcesApi={new MockSourcesApi()} />);
    expect(await screen.findByText(/오프라인 \(LLM 미연결/)).toBeInTheDocument();
  });

  it("re-reads the transfer log when its tab is opened", async () => {
    // The log grows while the app runs. Fetching once at mount left the tab
    // reporting "아직 전송된 기록이 없어요" right after a collection that had
    // just sent a dozen requests.
    const user = userEvent.setup();
    vi.mocked(ipc.listTransfers)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([
        { at: "2026-09-16T04:00:00Z", purpose: "summarize", model: "google/gemma-4-12b", masked_preview: "…", bytes_sent: 120 },
      ]);
    render(<HomeView onLock={vi.fn()} sourcesApi={new MockSourcesApi()} />);
    await screen.findByText(/현재 사용 중/);

    await user.click(screen.getByRole("tab", { name: /전송 기록/ }));
    expect(await screen.findByText("summarize")).toBeInTheDocument();
    expect(ipc.listTransfers).toHaveBeenCalledTimes(2);
  });

  it("toggles the local API server", async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.setServerEnabled).mockResolvedValue();
    render(<HomeView onLock={vi.fn()} sourcesApi={new MockSourcesApi()} />);
    await screen.findByText(/현재 사용 중/);
    await user.click(screen.getByRole("checkbox", { name: /비활성화됨/ }));
    expect(ipc.setServerEnabled).toHaveBeenCalledWith(true);
  });
  it("surfaces the source collection screen inside settings", async () => {
    render(<HomeView onLock={vi.fn()} initialTab="sources" sourcesApi={new MockSourcesApi()} />);
    expect(screen.getByRole("tab", { name: "연결 소스" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { name: "소스 수집" })).toBeInTheDocument();
    // The source list loads asynchronously from the backend catalog, so wait
    // for it to render rather than asserting synchronously.
    expect(await screen.findByText(/Notion/)).toBeInTheDocument();
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
