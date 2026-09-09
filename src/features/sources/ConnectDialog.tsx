// US-1.x — source connection form.
//
// Renders a credential form from a source's `FieldSpec[]` (the backend is the
// single source of truth for which fields a source needs). Secret fields use a
// password input; values are write-only — once stored the backend never echoes
// them back, so we always start from empty inputs, matching alpha-agent-v3's
// vault handling.

import { useState } from "react";
import type { SourceKind, SourceStatus } from "../../shared/contracts";
import { messageOf } from "../u4-shared/view-state";

interface Props {
  source: SourceStatus;
  label: string;
  onConnect: (
    source: SourceKind,
    values: Record<string, string>,
  ) => Promise<void>;
  onClose: () => void;
  /** Called after a successful connect so the list can refresh. */
  onConnected: () => void;
}

export function ConnectDialog({
  source,
  label,
  onConnect,
  onClose,
  onConnected,
}: Props) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await onConnect(source.kind, values);
      onConnected();
      onClose();
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={`${label} 연결`}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.35)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 100,
      }}
      onClick={onClose}
    >
      <form
        onSubmit={submit}
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "#fff",
          borderRadius: 12,
          padding: 24,
          width: 420,
          maxWidth: "90vw",
        }}
      >
        <h3 style={{ margin: "0 0 4px" }}>{label} 연결</h3>
        <p style={{ color: "#6b6b70", fontSize: 13, marginTop: 0 }}>
          입력한 자격증명은 이 기기에 암호화되어 저장됩니다.
        </p>

        {source.fields.map((f) => (
          <label
            key={f.key}
            style={{ display: "block", marginTop: 12, fontSize: 13 }}
          >
            <span style={{ fontWeight: 600 }}>
              {f.label}
              {f.required && <span style={{ color: "#c0392b" }}> *</span>}
            </span>
            <input
              type={f.secret ? "password" : "text"}
              placeholder={f.placeholder}
              value={values[f.key] ?? ""}
              autoComplete={f.secret ? "new-password" : "off"}
              onChange={(e) =>
                setValues((v) => ({ ...v, [f.key]: e.target.value }))
              }
              style={{
                display: "block",
                width: "100%",
                boxSizing: "border-box",
                marginTop: 4,
                padding: "8px 10px",
                border: "1px solid #d0d0d4",
                borderRadius: 8,
                fontSize: 14,
              }}
            />
          </label>
        ))}

        {error && (
          <p role="alert" style={{ color: "#c0392b", fontSize: 13 }}>
            {error}
          </p>
        )}

        <div
          style={{
            display: "flex",
            gap: 8,
            justifyContent: "flex-end",
            marginTop: 20,
          }}
        >
          <button type="button" onClick={onClose} disabled={saving}>
            취소
          </button>
          <button type="submit" disabled={saving}>
            {saving ? "연결 중…" : "연결"}
          </button>
        </div>
      </form>
    </div>
  );
}
