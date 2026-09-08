import { useState, type FormEvent } from "react";
import { ipc } from "../../shared/ipc";

/** Returning user: unlock by re-deriving the key from the password (US-7.2). */
export function UnlockView({ onDone }: { onDone: () => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await ipc.unlock(password);
      onDone();
    } catch {
      // Deliberately generic — never reveal whether the password was close.
      setError("Incorrect password.");
      setBusy(false);
      setPassword("");
    }
  }

  return (
    <section className="card">
      <h2>Unlock</h2>
      <p className="muted">Enter your password to decrypt your vault for this session.</p>
      <form onSubmit={submit}>
        <label>
          Password
          <input
            type="password"
            value={password}
            autoFocus
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        {error && <p className="error">{error}</p>}
        <button type="submit" disabled={busy || password.length === 0}>
          {busy ? "Unlocking…" : "Unlock"}
        </button>
      </form>
    </section>
  );
}
