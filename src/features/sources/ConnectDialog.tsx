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
  ) => Promise<string>;
  onClose: () => void;
  /** Called after a successful connect so the list can refresh. */
  onConnected: () => void;
}

/** How to obtain each source's credentials — shown above the form so the owner
 * doesn't have to leave to figure out where a token comes from. */
interface Guide {
  steps: string[];
  link?: { href: string; text: string };
}

const GUIDES: Partial<Record<SourceKind, Guide>> = {
  Notion: {
    steps: [
      "notion.so/my-integrations 에서 New integration → Internal 로 생성",
      "생성 후 나오는 'Internal Integration Secret'(secret_… 또는 ntn_…) 복사",
      "수집할 페이지에서 ⋯ → Connections → 방금 만든 integration 연결 (하위 페이지 상속)",
      "아래에 토큰을 붙여넣고 연결",
    ],
    link: {
      href: "https://www.notion.so/my-integrations",
      text: "Notion Integrations 열기",
    },
  },
  Gmail: {
    steps: [
      "Google 계정에서 2단계 인증을 켜 둡니다",
      "myaccount.google.com/apppasswords 에서 앱 비밀번호 발급 (16자리)",
      "Gmail 주소와 발급받은 앱 비밀번호를 아래에 입력",
    ],
    link: {
      href: "https://myaccount.google.com/apppasswords",
      text: "Google 앱 비밀번호 발급",
    },
  },
};

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
  // The connect note (e.g. reachable-page count / "nothing shared" warning).
  // Shown after a successful verify so the owner learns what the connection can
  // actually see before dismissing the dialog.
  const [note, setNote] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      const msg = await onConnect(source.kind, values);
      // Refresh the list now (card flips to connected), but keep the dialog open
      // showing the note so the owner sees whether it can actually collect.
      onConnected();
      setNote(msg);
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

        {note !== null ? (
          <div>
            <div
              style={{
                background: "#ecfdf5",
                border: "1px solid #a7f3d0",
                borderRadius: 8,
                padding: "12px 14px",
                margin: "12px 0",
                fontSize: 13,
                color: "#065f46",
                lineHeight: 1.6,
              }}
              role="status"
            >
              ✓ {note}
            </div>
            <div
              style={{
                display: "flex",
                justifyContent: "flex-end",
                marginTop: 16,
              }}
            >
              <button type="button" onClick={onClose}>
                완료
              </button>
            </div>
          </div>
        ) : (
          <>
        <p style={{ color: "#6b6b70", fontSize: 13, marginTop: 0 }}>
          입력한 자격증명은 이 기기에 암호화되어 저장됩니다.
        </p>

        {GUIDES[source.kind] && (
          <div
            style={{
              background: "#f7f8fa",
              border: "1px solid #e4e7ec",
              borderRadius: 8,
              padding: "10px 12px",
              margin: "12px 0",
              fontSize: 12.5,
              color: "#374151",
            }}
          >
            <div style={{ fontWeight: 600, marginBottom: 6 }}>연결 방법</div>
            <ol style={{ margin: 0, paddingLeft: 18, lineHeight: 1.6 }}>
              {GUIDES[source.kind]!.steps.map((step, i) => (
                <li key={i}>{step}</li>
              ))}
            </ol>
            {GUIDES[source.kind]!.link && (
              <a
                href={GUIDES[source.kind]!.link!.href}
                target="_blank"
                rel="noreferrer noopener"
                style={{
                  display: "inline-block",
                  marginTop: 8,
                  color: "#1463ff",
                  fontWeight: 600,
                }}
              >
                {GUIDES[source.kind]!.link!.text} ↗
              </a>
            )}
          </div>
        )}

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
          </>
        )}
      </form>
    </div>
  );
}
