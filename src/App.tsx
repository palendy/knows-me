import { useCallback, useEffect, useState } from "react";
import type { AppStatus } from "./shared/contracts";
import { ipc } from "./shared/ipc";
import { SetupView } from "./features/onboarding/SetupView";
import { UnlockView } from "./features/onboarding/UnlockView";
import { HomeView } from "./features/onboarding/HomeView";

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
        <HomeView onLock={refresh} />
      )}

      <footer className="foot muted">
        U1 · Core Platform &amp; Security — encryption never leaves this device.
      </footer>
    </main>
  );
}
