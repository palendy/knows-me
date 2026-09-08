import { useState, type FormEvent } from "react";
import { ipc } from "../../shared/ipc";

/** First-run: set a password, which initializes encryption (US-7.1). */
export function SetupView({ onDone }: { onDone: () => void }) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const canSubmit = password.length >= 8 && password === confirm && !busy;

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await ipc.setupPassword(password);
      onDone();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  }

  return (
    <section className="card">
      <h2>Set your password</h2>
      <p className="muted">
        Your data is encrypted at rest with a key derived from this password
        (Argon2id → AES&#8209;256&#8209;GCM). We never store the password or the key —
        if you lose it, the data cannot be recovered.
      </p>
      <form onSubmit={submit}>
        <label>
          Password
          <input
            type="password"
            value={password}
            autoFocus
            onChange={(e) => setPassword(e.target.value)}
            placeholder="at least 8 characters"
          />
        </label>
        <label>
          Confirm password
          <input
            type="password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
          />
        </label>
        {password.length > 0 && password.length < 8 && (
          <p className="hint">Use at least 8 characters.</p>
        )}
        {confirm.length > 0 && password !== confirm && (
          <p className="hint">Passwords don’t match.</p>
        )}
        {error && <p className="error">{error}</p>}
        <button type="submit" disabled={!canSubmit}>
          {busy ? "Initializing…" : "Create vault"}
        </button>
      </form>
    </section>
  );
}
