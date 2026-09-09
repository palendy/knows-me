import { useEffect, useState } from "react";
import type { IssuedShareToken, ShareStatus, ShareTokenInfo } from "../../shared/contracts";
import { ipc } from "../../shared/ipc";
import "./sharing.css";

/// The "공유" settings tab: turn the MCP servers on/off, publish a tunnel, and
/// mint/revoke per-teammate consumer tokens. Renders only while the vault is
/// unlocked (it lives inside the unlocked settings shell), so every call here has
/// a live session behind it.
export function SharingSettings() {
  const [status, setStatus] = useState<ShareStatus | null>(null);
  const [tokens, setTokens] = useState<ShareTokenInfo[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // Issuance form. `issued` holds the just-minted secret so it can be shown
  // once — it is never recoverable after this render.
  const [label, setLabel] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [issued, setIssued] = useState<IssuedShareToken | null>(null);

  async function refresh() {
    const [s, t] = await Promise.all([ipc.shareStatus(), ipc.listShareTokens()]);
    setStatus(s);
    setTokens(t);
  }

  useEffect(() => {
    let active = true;
    void Promise.all([ipc.shareStatus(), ipc.listShareTokens(), ipc.listShareCategories()])
      .then(([s, t, c]) => {
        if (!active) return;
        setStatus(s);
        setTokens(t);
        setCategories(c);
      })
      .catch(() => {
        if (active) setError("공유 설정을 불러오지 못했습니다. 다시 열어 주세요.");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  async function toggleSharing(on: boolean) {
    setBusy(true);
    setError("");
    try {
      await ipc.setSharingEnabled(on);
      await refresh();
    } catch {
      setError("공유 서버 설정을 저장하지 못했습니다. 다시 시도해 주세요.");
    } finally {
      setBusy(false);
    }
  }

  function togglePick(cat: string) {
    setPicked((prev) => (prev.includes(cat) ? prev.filter((c) => c !== cat) : [...prev, cat]));
    setIssued(null);
  }

  async function issue() {
    setBusy(true);
    setError("");
    setIssued(null);
    try {
      const token = await ipc.issueShareToken(label.trim(), picked);
      setIssued(token);
      setLabel("");
      setPicked([]);
      await refresh();
    } catch {
      setError("토큰을 발급하지 못했습니다. 다시 시도해 주세요.");
    } finally {
      setBusy(false);
    }
  }

  async function revoke(id: string) {
    setBusy(true);
    setError("");
    try {
      await ipc.revokeShareToken(id);
      await refresh();
    } catch {
      setError("토큰을 폐기하지 못했습니다. 다시 시도해 주세요.");
    } finally {
      setBusy(false);
    }
  }

  async function toggleTunnel() {
    const running = Boolean(status?.tunnel_url);
    setBusy(true);
    setError("");
    try {
      if (running) await ipc.stopShareTunnel();
      else await ipc.startShareTunnel();
      await refresh();
    } catch {
      setError(
        running
          ? "터널을 끄지 못했습니다. 다시 시도해 주세요."
          : "터널을 시작하지 못했습니다. cloudflared 설치를 확인해 주세요.",
      );
    } finally {
      setBusy(false);
    }
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* clipboard unavailable — the value is on screen to copy manually */
    }
  }

  const enabled = status?.enabled ?? false;
  const endpoint = status?.tunnel_url ? `${status.tunnel_url}/mcp` : null;

  return (
    <section className="settings-view" aria-label="지식 공유">
      <header className="settings-heading">
        <h2>지식 공유</h2>
        <p>내 지식을 팀원 에이전트가 안전하게 질의하도록 MCP로 공유합니다.</p>
      </header>
      {error && (
        <p role="alert" className="settings-error">
          {error}
        </p>
      )}

      <section className="settings-block">
        <div className="settings-block-title">
          <h3>MCP 서버</h3>
          <p>켜면 내 에이전트가 쓰는 로컬 서버와, 팀 공유용 서버가 함께 뜹니다.</p>
        </div>
        <label className="settings-toggle">
          <input
            type="checkbox"
            checked={enabled}
            disabled={loading || busy}
            onChange={(e) => void toggleSharing(e.target.checked)}
          />
          <span>{enabled ? "활성화됨" : "비활성화됨"}</span>
        </label>
        {enabled && status && (
          <dl className="settings-details">
            <div>
              <dt>내 에이전트용 (로컬)</dt>
              <dd>{status.owner_port ? `127.0.0.1:${status.owner_port}/mcp` : "—"}</dd>
            </div>
            <div>
              <dt>팀 공유용 (로컬)</dt>
              <dd>{status.shared_port ? `127.0.0.1:${status.shared_port}/mcp` : "—"}</dd>
            </div>
          </dl>
        )}
      </section>

      {enabled && (
        <section className="settings-block">
          <div className="settings-block-title">
            <h3>팀 공유 주소</h3>
            <p>공유용 서버를 인터넷에 노출하는 터널입니다. 토큰이 있어야 접근할 수 있습니다.</p>
          </div>
          {!status?.cloudflared_installed && !endpoint && (
            <p className="sharing-hint">
              cloudflared가 설치되어 있지 않습니다. <code>brew install cloudflared</code> 후 다시
              시도하세요.
            </p>
          )}
          {endpoint ? (
            <div className="sharing-url">
              <code>{endpoint}</code>
              <button className="secondary" type="button" onClick={() => void copy(endpoint)}>
                복사
              </button>
              <button
                className="secondary"
                type="button"
                onClick={() => void toggleTunnel()}
                disabled={busy}
              >
                터널 끄기
              </button>
            </div>
          ) : (
            <button
              className="primary"
              type="button"
              onClick={() => void toggleTunnel()}
              disabled={busy || !status?.cloudflared_installed}
            >
              터널 시작
            </button>
          )}
        </section>
      )}

      {enabled && (
        <section className="settings-block">
          <div className="settings-block-title">
            <h3>토큰 발급</h3>
            <p>팀원마다 하나씩. 부여한 범주의 공유 페이지만 보입니다.</p>
          </div>
          <label className="settings-field">
            <span>팀원 이름 / 라벨</span>
            <input
              type="text"
              value={label}
              onChange={(e) => {
                setLabel(e.target.value);
                setIssued(null);
              }}
              placeholder="예: 지빈"
              spellCheck={false}
            />
          </label>
          <fieldset className="sharing-categories" disabled={busy}>
            <legend>공유할 범주</legend>
            {categories.length === 0 ? (
              <p className="sharing-hint">
                아직 공유할 범주가 없습니다. 위키에서 페이지에 범주를 지정하고 공유로 승인하세요.
              </p>
            ) : (
              categories.map((c) => (
                <label key={c} className={`sharing-cat${picked.includes(c) ? " is-selected" : ""}`}>
                  <input type="checkbox" checked={picked.includes(c)} onChange={() => togglePick(c)} />
                  <span>{c}</span>
                </label>
              ))
            )}
          </fieldset>
          <button
            className="primary"
            type="button"
            onClick={() => void issue()}
            disabled={busy || !label.trim() || picked.length === 0}
          >
            발급
          </button>
          {issued && (
            <div className="sharing-secret" role="status">
              <p>
                <strong>{issued.id}</strong>의 토큰이 발급되었습니다.{" "}
                <em>지금 복사하세요 — 다시 볼 수 없습니다.</em>
              </p>
              <div className="sharing-url">
                <code>{issued.secret}</code>
                <button
                  className="secondary"
                  type="button"
                  onClick={() => void copy(issued.secret)}
                >
                  복사
                </button>
              </div>
            </div>
          )}
        </section>
      )}

      {enabled && (
        <section className="settings-block">
          <div className="settings-block-title">
            <h3>발급된 토큰</h3>
            <p>폐기하면 다음 요청부터 즉시 막힙니다.</p>
          </div>
          {tokens.length === 0 ? (
            <p className="settings-empty">아직 발급된 토큰이 없습니다.</p>
          ) : (
            <ul className="sharing-tokens">
              {tokens.map((t) => (
                <li key={`${t.id}-${t.issued_at}`}>
                  <div>
                    <strong>{t.id}</strong>
                    <span className="sharing-token-cats">{t.granted.join(", ") || "범주 없음"}</span>
                  </div>
                  <button
                    className="secondary"
                    type="button"
                    onClick={() => void revoke(t.id)}
                    disabled={busy}
                  >
                    폐기
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </section>
  );
}
