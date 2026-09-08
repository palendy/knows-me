// Typed bridge to the Rust core.
//
// Inside the Tauri webview these call real `#[tauri::command]` handlers (in the
// `desktop/` crate). In a plain browser (`npm run dev` without Tauri) they fall
// back to an in-browser mock so the onboarding UI is runnable and screenshottable
// on its own. The mock is NOT secure — it exists only for standalone UI dev.

import type {
  AppConfig,
  AppStatus,
  TransferPolicy,
  TransferRecord,
} from "./contracts";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
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
  getConfig: () => call<AppConfig>("get_config"),
  setTransferPolicy: (policy: TransferPolicy) =>
    call<void>("set_transfer_policy", { policy }),
  listTransfers: () => call<TransferRecord[]>("list_transfers"),
};

// --- Browser mock ----------------------------------------------------------

const LS = {
  initialized: "knowsme.mock.initialized",
  password: "knowsme.mock.password",
  config: "knowsme.mock.config",
};

let mockUnlocked = false;

function defaultConfig(): AppConfig {
  return { transfer_policy: "MaskAndMinimize", server_enabled: false, llm_model: "claude-opus-5" };
}

function readConfig(): AppConfig {
  const raw = localStorage.getItem(LS.config);
  return raw ? (JSON.parse(raw) as AppConfig) : defaultConfig();
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
    case "get_config":
      return readConfig() as T;
    case "set_transfer_policy": {
      if (!mockUnlocked) throw new Error("locked: unlock required");
      const cfg = readConfig();
      cfg.transfer_policy = args?.policy as TransferPolicy;
      localStorage.setItem(LS.config, JSON.stringify(cfg));
      return undefined as T;
    }
    case "list_transfers":
      return [] as T;
    default:
      throw new Error(`unknown command: ${cmd}`);
  }
}
