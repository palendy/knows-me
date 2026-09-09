// Typed bridge to the Rust core.
//
// Inside the Tauri webview these call real `#[tauri::command]` handlers (in the
// `desktop/` crate). In a plain browser (`npm run dev` without Tauri) they fall
// back to an in-browser mock so the onboarding UI is runnable and screenshottable
// on its own. The mock is NOT secure — it exists only for standalone UI dev.

import type {
  AppConfig,
  AppStatus,
  ClaudeInstall,
  ConfigDto,
  IssuedShareToken,
  LlmConfigInput,
  ShareStatus,
  ShareTokenInfo,
  TransferPolicy,
  TransferRecord,
} from "./contracts";

export const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Invoke a Rust command. Inside Tauri this hits the real handler; in a plain
 * browser it uses the onboarding mock. Exported so the U3/U4 adapters
 * (`TauriApi`, `TauriInterviewApi`) can share one bridge — they only run under
 * Tauri, where every branch reaches the real command layer.
 */
export async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  return mock<T>(cmd, args);
}

export const ipc = {
  getStatus: () => call<AppStatus>("get_status"),
  setupPassword: (password: string) => call<void>("setup_password", { password }),
  unlock: (password: string) => call<void>("unlock", { password }),
  lock: () => call<void>("lock"),
  getConfig: () => call<ConfigDto>("get_config"),
  setTransferPolicy: (policy: TransferPolicy) =>
    call<void>("set_transfer_policy", { policy }),
  setLlmConfig: (input: LlmConfigInput) =>
    call<void>("set_llm_config", { ...input }),
  discoverClaudeInstalls: () =>
    call<ClaudeInstall[]>("discover_claude_installs"),
  setServerEnabled: (on: boolean) => call<void>("set_server_enabled", { on }),
  listTransfers: () => call<TransferRecord[]>("list_transfers"),
  // Sharing (MCP): servers, tunnel, consumer tokens.
  shareStatus: () => call<ShareStatus>("share_status"),
  setSharingEnabled: (on: boolean) => call<void>("set_sharing_enabled", { on }),
  listShareCategories: () => call<string[]>("list_share_categories"),
  issueShareToken: (id: string, categories: string[]) =>
    call<IssuedShareToken>("issue_share_token", { id, categories }),
  revokeShareToken: (id: string) => call<boolean>("revoke_share_token", { id }),
  listShareTokens: () => call<ShareTokenInfo[]>("list_share_tokens"),
  startShareTunnel: () => call<string>("start_share_tunnel"),
  stopShareTunnel: () => call<void>("stop_share_tunnel"),
};

// --- Browser mock ----------------------------------------------------------

const LS = {
  initialized: "knowsme.mock.initialized",
  password: "knowsme.mock.password",
  config: "knowsme.mock.config",
  // Not secure — the mock only needs to remember whether a key was "saved" so
  // the standalone UI can show the same states as the real, encrypted store.
  apiKeys: "knowsme.mock.apiKeys",
  // Sharing mock state for standalone UI dev.
  shareTokens: "knowsme.mock.shareTokens",
  tunnelUrl: "knowsme.mock.tunnelUrl",
};

let mockUnlocked = false;

function defaultConfig(): AppConfig {
  return {
    transfer_policy: "MaskAndMinimize",
    server_enabled: false,
    sharing_enabled: false,
    llm_provider: "claude-cli",
    llm_model: "claude-sonnet-5",
    llm_base_url: null,
    llm_binary: null,
  };
}

function readConfig(): AppConfig {
  const raw = localStorage.getItem(LS.config);
  // Merge over the default so a config saved before these fields existed
  // (older mock storage) still returns a complete object.
  return raw ? { ...defaultConfig(), ...(JSON.parse(raw) as AppConfig) } : defaultConfig();
}

/** The provider labels the real backend produces, mirrored for the mock. */
function mockLabel(cfg: AppConfig): string {
  switch (cfg.llm_provider) {
    case "openai":
      return `${cfg.llm_model} (${cfg.llm_base_url?.includes("openrouter") ? "OpenRouter" : "OpenAI"})`;
    case "anthropic":
      return `${cfg.llm_model} (Anthropic)`;
    default:
      return `${cfg.llm_model} (로컬 Claude Code)`;
  }
}

async function mock<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const initialized = localStorage.getItem(LS.initialized) === "true";
  switch (cmd) {
    case "get_status":
      return { initialized, unlocked: mockUnlocked } as T;
    case "setup_password":
      if (initialized) throw new Error("already initialized: use unlock");
      localStorage.setItem(LS.initialized, "true");
      localStorage.setItem(LS.password, String(args?.password ?? ""));
      localStorage.setItem(LS.config, JSON.stringify(defaultConfig()));
      mockUnlocked = true;
      return undefined as T;
    case "unlock":
      if (String(args?.password ?? "") !== localStorage.getItem(LS.password)) {
        throw new Error("incorrect password");
      }
      mockUnlocked = true;
      return undefined as T;
    case "lock":
      mockUnlocked = false;
      return undefined as T;
    case "get_config": {
      const cfg = readConfig();
      const keys = JSON.parse(localStorage.getItem(LS.apiKeys) ?? "{}");
      return {
        ...cfg,
        llm_label: mockLabel(cfg),
        has_api_key: Boolean(keys[cfg.llm_provider]),
      } as T;
    }
    case "set_transfer_policy": {
      if (!mockUnlocked) throw new Error("locked: unlock required");
      const cfg = readConfig();
      cfg.transfer_policy = args?.policy as TransferPolicy;
      localStorage.setItem(LS.config, JSON.stringify(cfg));
      return undefined as T;
    }
    case "set_llm_config": {
      if (!mockUnlocked) throw new Error("locked: unlock required");
      const cfg = readConfig();
      cfg.llm_provider = args?.provider as AppConfig["llm_provider"];
      cfg.llm_model = String(args?.model ?? "");
      const base = args?.base_url as string | null | undefined;
      cfg.llm_base_url = base && base.trim() ? base.trim() : null;
      const bin = args?.binary as string | null | undefined;
      cfg.llm_binary = bin && bin.trim() ? bin.trim() : null;
      localStorage.setItem(LS.config, JSON.stringify(cfg));
      // Write-only key: only touch storage when a value was supplied.
      const key = args?.api_key as string | null | undefined;
      if (key !== null && key !== undefined) {
        const keys = JSON.parse(localStorage.getItem(LS.apiKeys) ?? "{}");
        if (key.trim()) keys[cfg.llm_provider] = true;
        else delete keys[cfg.llm_provider];
        localStorage.setItem(LS.apiKeys, JSON.stringify(keys));
      }
      return undefined as T;
    }
    case "set_server_enabled": {
      if (!mockUnlocked) throw new Error("locked: unlock required");
      const cfg = readConfig();
      cfg.server_enabled = Boolean(args?.on);
      localStorage.setItem(LS.config, JSON.stringify(cfg));
      return undefined as T;
    }
    case "discover_claude_installs":
      // Standalone UI dev: pretend a native + one WSL install were found.
      return [
        { id: "native", label: "로컬", binary: "claude", model: "claude-sonnet-5" },
        { id: "wsl:Ubuntu", label: "WSL · Ubuntu", binary: "wsl -d Ubuntu claude", model: "opus" },
      ] as T;
    case "share_status": {
      const cfg = readConfig();
      const tunnel = localStorage.getItem(LS.tunnelUrl);
      return {
        enabled: cfg.sharing_enabled,
        owner_port: cfg.sharing_enabled ? 8766 : null,
        shared_port: cfg.sharing_enabled ? 8767 : null,
        tunnel_url: cfg.sharing_enabled ? tunnel : null,
        cloudflared_installed: true,
      } as T;
    }
    case "set_sharing_enabled": {
      if (!mockUnlocked) throw new Error("locked: unlock required");
      const cfg = readConfig();
      cfg.sharing_enabled = Boolean(args?.on);
      localStorage.setItem(LS.config, JSON.stringify(cfg));
      if (!cfg.sharing_enabled) localStorage.removeItem(LS.tunnelUrl);
      return undefined as T;
    }
    case "list_share_categories":
      return ["deploy", "payment", "workstyle"] as T;
    case "issue_share_token": {
      if (!mockUnlocked) throw new Error("locked: unlock required");
      const id = String(args?.id ?? "");
      const categories = (args?.categories as string[]) ?? [];
      const secret = `mock-${Math.random().toString(36).slice(2)}${Math.random()
        .toString(36)
        .slice(2)}`;
      const tokens = JSON.parse(
        localStorage.getItem(LS.shareTokens) ?? "[]",
      ) as ShareTokenInfo[];
      tokens.push({ id, granted: categories, issued_at: new Date().toISOString() });
      localStorage.setItem(LS.shareTokens, JSON.stringify(tokens));
      return { id, secret } as T;
    }
    case "revoke_share_token": {
      if (!mockUnlocked) throw new Error("locked: unlock required");
      const id = String(args?.id ?? "");
      const tokens = JSON.parse(
        localStorage.getItem(LS.shareTokens) ?? "[]",
      ) as ShareTokenInfo[];
      const kept = tokens.filter((t) => t.id !== id);
      localStorage.setItem(LS.shareTokens, JSON.stringify(kept));
      return (kept.length !== tokens.length) as T;
    }
    case "list_share_tokens":
      return JSON.parse(
        localStorage.getItem(LS.shareTokens) ?? "[]",
      ) as T;
    case "start_share_tunnel": {
      if (!mockUnlocked) throw new Error("locked: unlock required");
      const url = "https://mock-tunnel.trycloudflare.com";
      localStorage.setItem(LS.tunnelUrl, url);
      return url as T;
    }
    case "stop_share_tunnel":
      localStorage.removeItem(LS.tunnelUrl);
      return undefined as T;
    case "list_transfers":
      return [] as T;
    default:
      throw new Error(`unknown command: ${cmd}`);
  }
}
