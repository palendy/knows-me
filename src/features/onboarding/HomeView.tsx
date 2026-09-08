import { useEffect, useState } from "react";
import type { AppConfig, TransferPolicy, TransferRecord } from "../../shared/contracts";
import { ipc } from "../../shared/ipc";

const POLICIES: { value: TransferPolicy; label: string; desc: string }[] = [
  { value: "MaskAndMinimize", label: "Mask & minimize", desc: "Strip identifiers before any cloud call (recommended)." },
  { value: "AllowAll", label: "Send as-is", desc: "No masking. Not recommended." },
  { value: "LocalOnlyNoLlm", label: "Fully local", desc: "Never contact the cloud LLM." },
];

/**
 * Unlocked home: the vault is open. Shows the cloud-transfer policy and the
 * transparency log (what has left the device), and lets the owner re-lock.
 * The rich dashboard/graph/persona views are U4; this is U1's security surface.
 */
export function HomeView({ onLock }: { onLock: () => void }) {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [transfers, setTransfers] = useState<TransferRecord[]>([]);

  useEffect(() => {
    void ipc.getConfig().then(setConfig);
    void ipc.listTransfers().then(setTransfers);
  }, []);

  async function choose(policy: TransferPolicy) {
    await ipc.setTransferPolicy(policy);
    setConfig(await ipc.getConfig());
  }

  async function lock() {
    await ipc.lock();
    onLock();
  }

  return (
    <section className="card">
      <div className="row spread">
        <h2>Vault unlocked</h2>
        <button className="secondary" onClick={lock}>
          Lock
        </button>
      </div>

      <h3>Cloud transfer policy</h3>
      <div className="policies">
        {POLICIES.map((p) => (
          <label key={p.value} className="policy">
            <input
              type="radio"
              name="policy"
              checked={config?.transfer_policy === p.value}
              onChange={() => choose(p.value)}
            />
            <span>
              <strong>{p.label}</strong>
              <span className="muted"> — {p.desc}</span>
            </span>
          </label>
        ))}
      </div>

      <h3>Transfer transparency</h3>
      {transfers.length === 0 ? (
        <p className="muted">Nothing has been sent to the cloud yet.</p>
      ) : (
        <ul className="transfers">
          {transfers.map((t, i) => (
            <li key={i}>
              <code>{t.purpose}</code> · {t.model} · {t.bytes_sent} bytes
              <div className="muted preview">{t.masked_preview}</div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
