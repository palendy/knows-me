import { useCallback, useEffect, useMemo, useState } from "react";
import type { AppStatus } from "./shared/contracts";
import { call, inTauri, ipc } from "./shared/ipc";
import { SetupView } from "./features/onboarding/SetupView";
import { UnlockView } from "./features/onboarding/UnlockView";
import { HomeView, type SettingsTab } from "./features/onboarding/HomeView";
import { DashboardView } from "./features/dashboard/DashboardView";

import { PersonaChatView } from "./features/persona-chat/PersonaChatView";
import { QueueView } from "./features/queue/QueueView";

import type { KnowsMeApi } from "./features/u4-shared/api";
import { MockApi } from "./features/u4-shared/mock-api";
import { TauriApi } from "./features/u4-shared/tauri-api";
import type { InterviewApi } from "./features/queue/api";
import { MockInterviewApi } from "./features/queue/mock-interview-api";
import { TauriInterviewApi } from "./features/queue/tauri-interview-api";
import type { SourcesApi } from "./features/sources/api";
import { MockSourcesApi } from "./features/sources/mock-sources-api";
import { TauriSourcesApi } from "./features/sources/tauri-sources-api";

// Views depend only on the ports (KnowsMeApi / InterviewApi). Under Tauri we
// inject the real command adapters; in a plain browser (`npm run dev` without
// Tauri) we inject mocks so the whole shell is runnable and screenshottable.
function makeApis(): {
  api: KnowsMeApi;
  interview: InterviewApi;
  sources: SourcesApi;
} {
  if (inTauri) {
    return {
      api: new TauriApi(call),
      interview: new TauriInterviewApi(call),
      sources: new TauriSourcesApi(call),
    };
  }
  return {
    api: new MockApi(),
    interview: new MockInterviewApi(),
    sources: new MockSourcesApi(),
  };
}

type Tab = "dashboard" | "queue" | "persona" | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "dashboard", label: "대시보드" },

  { id: "queue", label: "대기열" },

  { id: "persona", label: "나와 대화" },
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
    <main className={status?.initialized && status.unlocked ? "app" : "app auth-app"}>
      {!(status?.initialized && status.unlocked) && <header className="brand">
        <h1>Knows Me</h1>
        <p className="tagline">나의 맥락이 모이는 곳</p>
      </header>}

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

    </main>
  );
}

export function UnlockedShell({ onLock }: { onLock: () => void }) {
  const [tab, setTab] = useState<Tab>("dashboard");
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  // Bumping this key remounts the read views so the dashboard/mini-home pull
  // fresh numbers after the queue confirms or rejects a fact.
  const [dataVersion, setDataVersion] = useState(0);
  const bump = useCallback(() => setDataVersion((v) => v + 1), []);

  const { api, interview, sources } = useMemo(makeApis, []);

  return (
    <div className="workspace">
      <aside className="sidebar">
        <a className="wordmark" href="#" onClick={(event) => { event.preventDefault(); setTab("dashboard"); }} aria-label="Knows Me 대시보드"><span className="brand-symbol">K</span>Knows Me</a>
        <div className="workspace-label">내 공간</div>
      <nav role="tablist" aria-label="화면" className="tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={tab === t.id}
            onClick={() => { if (t.id === "settings") setSettingsTab("general"); setTab(t.id); }}
            id={`tab-${t.id}`}
            aria-controls="workspace-panel"
          >
            <NavIcon name={t.id} /><span>{t.label}</span>
          </button>
        ))}
      </nav>
      <div className="sidebar-foot"><span className="version">Knows Me · v0.1</span></div>
      </aside>
      <div className="workspace-body">
      <div className="workspace-topbar"><span>내 공간 <span className="breadcrumb-divider">/</span> <strong>{TABS.find(t => t.id === tab)?.label}</strong></span></div>
      <div role="tabpanel" id="workspace-panel" aria-labelledby={`tab-${tab}`} className="workspace-content">
        {tab === "dashboard" && <DashboardView key={dataVersion} api={api} onNavigate={(next) => { if (next === "settings") setSettingsTab("sources"); setTab(next); }} />}
        {tab === "queue" && <QueueView api={interview} onChanged={bump} />}

        {tab === "persona" && <PersonaChatView api={api} />}
        {tab === "settings" && <HomeView onLock={onLock} initialTab={settingsTab} sourcesApi={sources} onIngested={bump} />}
      </div>
      </div>
    </div>
  );
}

function NavIcon({ name }: { name: Tab | "lock" }) {
  const paths: Record<Tab | "lock", React.ReactNode> = {
    dashboard: <><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" /><rect x="14" y="14" width="7" height="7" rx="1.5" /></>,
    queue: <><path d="M4 4h16v16H4zM4 14h5l2 3h2l2-3h5M8 8h8" /></>,
    persona: <path d="M5 4h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H9l-6 3V6a2 2 0 0 1 2-2Zm3 5h8m-8 4h5" />,
    settings: <><path d="M4 6h16M4 12h16M4 18h16" /><circle cx="9" cy="6" r="2" /><circle cx="16" cy="12" r="2" /><circle cx="8" cy="18" r="2" /></>,
    lock: <><rect x="5" y="10" width="14" height="11" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3m-4 5v2" /></>,
  };
  return <svg aria-hidden="true" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">{paths[name]}</svg>;
}


