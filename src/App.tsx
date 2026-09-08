import { useCallback, useEffect, useMemo, useState } from "react";
import type { AppStatus } from "./shared/contracts";
import { call, inTauri, ipc } from "./shared/ipc";
import { SetupView } from "./features/onboarding/SetupView";
import { UnlockView } from "./features/onboarding/UnlockView";
import { HomeView } from "./features/onboarding/HomeView";
import { DashboardView } from "./features/dashboard/DashboardView";
import { MiniHomeView } from "./features/minihome/MiniHomeView";
import { GraphView } from "./features/graph/GraphView";
import { PersonaChatView } from "./features/persona-chat/PersonaChatView";
import { QueueView } from "./features/queue/QueueView";
import type { KnowsMeApi } from "./features/u4-shared/api";
import { MockApi } from "./features/u4-shared/mock-api";
import { TauriApi } from "./features/u4-shared/tauri-api";
import type { InterviewApi } from "./features/queue/api";
import { MockInterviewApi } from "./features/queue/mock-interview-api";
import { TauriInterviewApi } from "./features/queue/tauri-interview-api";

// Views depend only on the ports (KnowsMeApi / InterviewApi). Under Tauri we
// inject the real command adapters; in a plain browser (`npm run dev` without
// Tauri) we inject mocks so the whole shell is runnable and screenshottable.
function makeApis(): { api: KnowsMeApi; interview: InterviewApi } {
  if (inTauri) {
    return { api: new TauriApi(call), interview: new TauriInterviewApi(call) };
  }
  return { api: new MockApi(), interview: new MockInterviewApi() };
}

type Tab = "dashboard" | "queue" | "minihome" | "graph" | "persona" | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "dashboard", label: "대시보드" },
  { id: "queue", label: "대기열" },
  { id: "minihome", label: "미니홈피" },
  { id: "graph", label: "지식 그래프" },
  { id: "persona", label: "페르소나" },
  { id: "settings", label: "설정" },
];

export function App() {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await ipc.getStatus());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <main className="app">
      <header className="brand">
        <h1>knows&#8209;me</h1>
        <p className="tagline">Your private, local context vault</p>
      </header>

      {error && <p className="error banner">{error}</p>}

      {status === null ? (
        <p className="muted">Loading…</p>
      ) : !status.initialized ? (
        <SetupView onDone={refresh} />
      ) : !status.unlocked ? (
        <UnlockView onDone={refresh} />
      ) : (
        <UnlockedShell onLock={refresh} />
      )}

      <footer className="foot muted">
        knows&#8209;me — encryption never leaves this device.
      </footer>
    </main>
  );
}

export function UnlockedShell({ onLock }: { onLock: () => void }) {
  const [tab, setTab] = useState<Tab>("dashboard");
  // Bumping this key remounts the read views so the dashboard/mini-home pull
  // fresh numbers after the queue confirms or rejects a fact.
  const [dataVersion, setDataVersion] = useState(0);
  const bump = useCallback(() => setDataVersion((v) => v + 1), []);

  const { api, interview } = useMemo(makeApis, []);

  return (
    <div>
      <nav role="tablist" aria-label="화면" className="tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={tab === t.id}
            onClick={() => setTab(t.id)}
            style={{ fontWeight: tab === t.id ? 700 : 400 }}
          >
            {t.label}
          </button>
        ))}
      </nav>

      <div role="tabpanel">
        {tab === "dashboard" && <DashboardView key={dataVersion} api={api} />}
        {tab === "queue" && <QueueView api={interview} onChanged={bump} />}
        {tab === "minihome" && <MiniHomeView key={dataVersion} api={api} />}
        {tab === "graph" && <GraphView key={dataVersion} api={api} />}
        {tab === "persona" && <PersonaChatView api={api} />}
        {tab === "settings" && <HomeView onLock={onLock} />}
      </div>
    </div>
  );
}
