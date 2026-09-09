// US-1.x — source collection.
//
// A card catalog (modeled on alpha-agent-v3's skill grid) showing which sources
// are connected, letting the owner connect/disconnect the credential-backed
// ones, and triggering a sync. The catalog (which sources exist, what fields
// each needs) comes from the backend `list_sources` command; this view maps
// each kind to a label/icon and renders connection state.
//
// Display note: the backend's single `Session` source (Claude + Codex session
// logs) is shown as *two* cards — Claude and Codex — so each agent reads as its
// own connector. Both trigger the same `Session` sync since the backend does
// not split them.

import { useCallback, useEffect, useRef, useState } from "react";
import type { SourceKind, SourceStatus } from "../../shared/contracts";
import type {
  IngestProgress,
  IngestSummary,
  SessionProject,
  SourcesApi,
} from "./api";
import { messageOf } from "../u4-shared/view-state";
import { ConnectDialog } from "./ConnectDialog";
import "./sources.css";

interface Props {
  api: SourcesApi;
  /** Called after a successful sync so other views can pull fresh counts. */
  onIngested?: () => void;
}

/** A card as shown to the owner. `sourceKind` is what the backend collects for
 * (Claude/Codex both map to `Session`); the rest is display. `comingSoon` cards
 * are planned connectors with no backend yet — icon only, controls disabled. */
interface CardModel {
  id: string;
  /** Absent for coming-soon cards (no backend source). */
  sourceKind?: SourceKind;
  label: string;
  detail: string;
  icon: string;
  comingSoon?: boolean;
}

/** How each backend source expands into display cards. Session → Claude+Codex.
 * Coming-soon cards (Confluence/Jira/Knox Mail) are shown dimmed as a roadmap. */
const CARDS: CardModel[] = [
  {
    id: "claude",
    sourceKind: "Session",
    label: "Claude",
    detail: "~/.claude/projects 의 대화 기록 — 자격증명 불필요",
    icon: "/source-icons/claude.svg",
  },
  {
    id: "codex",
    sourceKind: "Session",
    label: "Codex",
    detail: "~/.codex/sessions 의 대화 기록 — 자격증명 불필요",
    icon: "/source-icons/codex.svg",
  },
  {
    id: "file",
    sourceKind: "File",
    label: "파일",
    detail: "텍스트·마크다운 파일 (PDF/DOCX는 향후)",
    icon: "/source-icons/file.svg",
  },
  {
    id: "notion",
    sourceKind: "Notion",
    label: "Notion",
    detail: "Internal Integration Token으로 연결",
    icon: "/source-icons/notion.svg",
  },
  {
    id: "gmail",
    sourceKind: "Gmail",
    label: "Gmail",
    detail: "Gmail 주소 + 앱 비밀번호로 연결",
    icon: "/source-icons/gmail.svg",
  },
  {
    id: "confluence",
    label: "Confluence",
    detail: "곧 지원 예정 — 스페이스·페이지 수집",
    icon: "/source-icons/confluence.svg",
    comingSoon: true,
  },
  {
    id: "jira",
    label: "Jira",
    detail: "곧 지원 예정 — 이슈·코멘트 수집",
    icon: "/source-icons/jira.svg",
    comingSoon: true,
  },
  {
    id: "knox-mail",
    label: "Knox Mail",
    detail: "곧 지원 예정 — 사내 메일 수집",
    icon: "/source-icons/knox-mail.svg",
    comingSoon: true,
  },
];

type RunState =
  | { status: "idle" }
  | { status: "running"; card: string | "all"; drain: boolean }
  | { status: "done"; card: string | "all"; summary: IngestSummary }
  | { status: "error"; card: string | "all"; message: string };

export function SourcesView({ api, onIngested }: Props) {
  // Backend status keyed by SourceKind, or null while loading.
  const [status, setStatus] = useState<Record<string, SourceStatus> | null>(
    null,
  );
  const [loadError, setLoadError] = useState<string | null>(null);
  const [run, setRun] = useState<RunState>({ status: "idle" });
  const [connecting, setConnecting] = useState<SourceStatus | null>(null);
  const [scoping, setScoping] = useState(false);
  // Live progress for the card currently syncing (done/total), or null.
  const [progress, setProgress] = useState<IngestProgress | null>(null);
  // The card that started the current run, so progress events (keyed by
  // SourceKind) attach to the right card — Claude/Codex share SourceKind.
  const runningCard = useRef<CardModel | "all" | null>(null);

  // Subscribe to sync progress once; the callback reads the ref so it always
  // targets the active run without re-subscribing per sync.
  useEffect(() => {
    const unsub = api.onProgress((p) => setProgress(p));
    return unsub;
  }, [api]);

  // Auto-dismiss the sync toast a few seconds after a run finishes.
  useEffect(() => {
    if (run.status !== "done" && run.status !== "error") return;
    const t = setTimeout(() => setRun({ status: "idle" }), 5000);
    return () => clearTimeout(t);
  }, [run]);

  const refresh = useCallback(async () => {
    try {
      const list = await api.listSources();
      const byKind: Record<string, SourceStatus> = {};
      for (const s of list) byKind[s.kind] = s;
      setStatus(byKind);
      setLoadError(null);
    } catch (e) {
      setLoadError(messageOf(e));
    }
  }, [api]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /** One capped batch (`drain: false`) or the whole backlog (`drain: true`).
   *
   * The distinction is the difference between "collect" doing something and
   * doing everything: one batch is bounded and quick, draining walks the entire
   * history and can run for a long time. Both are offered because both are
   * wanted — a quick top-up, and a first full collection you walk away from. */
  async function sync(card?: CardModel, drain = false) {
    const key = card?.id ?? "all";
    runningCard.current = card ?? "all";
    setProgress(null);
    setRun({ status: "running", card: key, drain });
    try {
      const summary = drain
        ? await api.triggerIngestAll(card?.sourceKind)
        : await api.triggerIngest(card?.sourceKind);
      setRun({ status: "done", card: key, summary });
      onIngested?.();
    } catch (e) {
      setRun({ status: "error", card: key, message: messageOf(e) });
    } finally {
      runningCard.current = null;
      setProgress(null);
    }
  }

  async function disconnect(kind: SourceKind) {
    try {
      await api.disconnectSource(kind);
      await refresh();
    } catch (e) {
      setLoadError(messageOf(e));
    }
  }

  const busy = run.status === "running";

  return (
    <section aria-label="소스">
      <header style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <h2 style={{ margin: 0 }}>소스 수집</h2>
        <button
          type="button"
          className="source-btn-sm"
          onClick={() => void sync(undefined, false)}
          disabled={busy}
          title="각 소스에서 한 묶음씩 가져옵니다. 금방 끝납니다."
        >
          {busy && run.status === "running" && run.card === "all" && !run.drain ? "수집 중…" : "새로 온 것만"}
        </button>
        <button
          type="button"
          className="source-btn-sm"
          onClick={() => void sync(undefined, true)}
          disabled={busy}
          title="남은 기록을 끝까지 가져옵니다. 처음 수집이라면 오래 걸립니다."
        >
          {busy && run.status === "running" && run.card === "all" && run.drain ? "끝까지 수집 중…" : "끝까지 수집"}
        </button>
        <button
          type="button"
          className="source-btn-sm"
          onClick={() => setScoping(true)}
          disabled={busy}
          title="어떤 프로젝트의 세션을 모을지 고릅니다"
        >
          범위
        </button>
        {run.status !== "idle" && (
          <SyncToast run={run} progress={progress} label={runLabel(run.card)} />
        )}
      </header>

      {loadError && (
        <p role="alert" style={{ color: "#c0392b" }}>
          {loadError}
        </p>
      )}

      {scoping && (
        <SessionScopeDialog api={api} onClose={() => setScoping(false)} />
      )}

      {status === null ? (
        <p style={{ color: "#6b7280", marginTop: 16 }}>불러오는 중…</p>
      ) : (
        <div className="sources-grid">
          {CARDS.map((card) => {
            // Coming-soon cards have no backend source: icon only, dimmed,
            // controls disabled.
            if (card.comingSoon) {
              return (
                <div key={card.id} className="source-card is-dimmed">
                  <div className="source-card__top">
                    <div className="source-card__icon">
                      <img src={card.icon} alt="" />
                    </div>
                    <div className="source-card__text">
                      <h3 className="source-card__title">{card.label}</h3>
                      <p className="source-card__detail">{card.detail}</p>
                    </div>
                    <button
                      type="button"
                      className="source-toggle"
                      role="switch"
                      aria-checked={false}
                      aria-label={`${card.label} 연결`}
                      disabled
                    >
                      <span className="source-toggle__thumb" />
                    </button>
                  </div>
                  <div className="source-card__bottom">
                    <span className="source-badge source-badge--needed">
                      준비 중
                    </span>
                  </div>
                </div>
              );
            }

            const s = card.sourceKind ? status[card.sourceKind] : undefined;
            // A card whose backend source didn't load is skipped defensively.
            if (!s) return null;
            const needsCredentials = s.fields.length > 0;
            const ready = s.ready;
            return (
              <div
                key={card.id}
                className={`source-card${ready ? "" : " is-dimmed"}`}
              >
                <div className="source-card__top">
                  <div className="source-card__icon">
                    <img src={card.icon} alt="" />
                  </div>
                  <div className="source-card__text">
                    <h3 className="source-card__title">{card.label}</h3>
                    <p className="source-card__detail">{card.detail}</p>
                  </div>
                  {needsCredentials && (
                    <button
                      type="button"
                      className={`source-toggle${s.connected ? " is-on" : ""}`}
                      role="switch"
                      aria-checked={s.connected}
                      aria-label={`${card.label} 연결`}
                      onClick={() =>
                        s.connected ? void disconnect(s.kind) : setConnecting(s)
                      }
                    >
                      <span className="source-toggle__thumb" />
                    </button>
                  )}
                </div>

                <div className="source-card__bottom">
                  <span
                    className={`source-badge ${
                      ready ? "source-badge--ready" : "source-badge--needed"
                    }`}
                  >
                    {ready
                      ? needsCredentials
                        ? "연결됨"
                        : "사용 가능"
                      : "연결 필요"}
                  </span>

                  <div className="source-card__actions">
                    {needsCredentials && s.connected && (
                      <button
                        type="button"
                        className="source-edit-btn"
                        aria-label={`${card.label} 설정 수정`}
                        title="설정 수정"
                        onClick={() => setConnecting(s)}
                      >
                        {/* pencil */}
                        <svg
                          width="15"
                          height="15"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="2"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        >
                          <path d="M12 20h9" />
                          <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
                        </svg>
                      </button>
                    )}
                    <button
                      type="button"
                      className="source-btn-sm"
                      onClick={() => void sync(card)}
                      disabled={busy || !ready}
                      title={ready ? undefined : "먼저 연결하세요"}
                    >
                      {busy && run.card === card.id ? "수집 중…" : "수집"}
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {connecting && (
        <ConnectDialog
          source={connecting}
          label={
            CARDS.find((c) => c.sourceKind === connecting.kind)?.label ??
            connecting.kind
          }
          onConnect={(source, values) => api.connectSource(source, values)}
          onClose={() => setConnecting(null)}
          onConnected={() => void refresh()}
        />
      )}

    </section>
  );
}

/** The label for the card a run belongs to ("전체" for a collect-all run). */
/**
 * Pick which project directories collection reads from.
 *
 * The list is every project on disk, not just the ones currently in scope —
 * a picker that hides what you excluded gives you no way to put it back. An
 * empty selection means "everything", which is both the natural reading of an
 * untouched picker and the only choice that cannot leave the owner with a
 * source that silently collects nothing.
 */
function SessionScopeDialog({
  api,
  onClose,
}: {
  api: SourcesApi;
  onClose: () => void;
}) {
  const [projects, setProjects] = useState<SessionProject[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.listSessionProjects(), api.getSessionScope()])
      .then(([list, scope]) => {
        if (cancelled) return;
        setProjects(list);
        setSelected(new Set(scope));
      })
      .catch((e) => !cancelled && setError(messageOf(e)));
    return () => {
      cancelled = true;
    };
  }, [api]);

  function toggle(path: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  }

  async function save() {
    setSaving(true);
    try {
      await api.setSessionScope([...selected]);
      onClose();
    } catch (e) {
      setError(messageOf(e));
      setSaving(false);
    }
  }

  const all = selected.size === 0;
  const totalPicked = (projects ?? [])
    .filter((p) => selected.has(p.path))
    .reduce((n, p) => n + p.sessions, 0);
  const totalAll = (projects ?? []).reduce((n, p) => n + p.sessions, 0);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="수집 범위"
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
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "#fff",
          borderRadius: 12,
          padding: 24,
          width: 460,
          maxWidth: "90vw",
          maxHeight: "80vh",
          overflowY: "auto",
        }}
      >
        <h3 style={{ marginTop: 0 }}>수집 범위</h3>
        <p style={{ color: "#5d6270", fontSize: 14, marginTop: 0 }}>
          어떤 프로젝트의 세션을 모을지 고르세요. 아무것도 고르지 않으면 전부
          모읍니다.
        </p>

        {error && <p role="alert" style={{ color: "#c0392b" }}>{error}</p>}
        {!projects && !error && <p>불러오는 중…</p>}

        {projects && (
          <ul
            style={{
              listStyle: "none",
              padding: 0,
              margin: "0 0 12px",
              maxHeight: "40vh",
              overflowY: "auto",
            }}
          >
            {projects.map((p) => (
              <li key={p.path} style={{ borderBottom: "1px solid #eceef1" }}>
                <label
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    padding: "8px 2px",
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={selected.has(p.path)}
                    onChange={() => toggle(p.path)}
                  />
                  <span style={{ flex: 1, fontSize: 14 }}>{p.label}</span>
                  <span
                    style={{
                      fontSize: 12.5,
                      color: "#6b6b70",
                      fontVariantNumeric: "tabular-nums",
                    }}
                  >
                    세션 {p.sessions}개
                  </span>
                </label>
              </li>
            ))}
          </ul>
        )}

        <p style={{ fontSize: 13.5, color: "#5d6270" }}>
          {all
            ? `전체 ${totalAll}개 세션을 모읍니다.`
            : `선택한 ${selected.size}개 프로젝트 · 세션 ${totalPicked}개`}
        </p>

        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            marginTop: 4,
          }}
        >
          <button type="button" onClick={onClose} disabled={saving}>
            취소
          </button>
          <button
            type="button"
            className="work-primary"
            onClick={() => void save()}
            disabled={saving || !projects}
          >
            {saving ? "저장 중…" : "저장"}
          </button>
        </div>
      </div>
    </div>
  );
}

function runLabel(cardId: string): string {
  if (cardId === "all") return "전체";
  return CARDS.find((c) => c.id === cardId)?.label ?? cardId;
}

/** Sync feedback shown as a toast beside the collect buttons. Kept out of the
 * cards so a running/finished sync never grows a card and ripples its grid
 * row's height. Distinguishes three done-outcomes so an all-zeros run isn't
 * ambiguous: error (with cause), items collected, or a clean "nothing new". */
function SyncToast({
  run,
  progress,
  label,
}: {
  run: RunState;
  progress: IngestProgress | null;
  label: string;
}) {
  if (run.status === "running") {
    const pct =
      progress && progress.total > 0
        ? Math.min(100, Math.round((progress.done / progress.total) * 100))
        : null;
    return (
      <div className="sync-toast" role="status">
        <span className="sync-toast__msg">
          {label} 수집 중…
          {progress && progress.total > 0
            ? ` ${progress.done}/${progress.total}`
            : progress && progress.done > 0
              ? ` ${progress.done}건`
              : ""}
        </span>
        <span className="sync-toast__track">
          <span
            className={`sync-toast__fill${pct === null ? " is-indeterminate" : ""}`}
            style={pct === null ? undefined : { width: `${pct}%` }}
          />
        </span>
      </div>
    );
  }
  if (run.status === "error") {
    return (
      <div className="sync-toast sync-toast--error" role="alert">
        {label} 수집 실패: {run.message}
      </div>
    );
  }
  if (run.status === "done") {
    const s = run.summary;
    if (s.errors > 0) {
      return (
        <div className="sync-toast sync-toast--error" role="alert">
          {label} · 오류 {s.errors}건
          {s.error_messages.length > 0 && ` — ${s.error_messages.join("; ")}`}
        </div>
      );
    }
    if (s.collected > 0) {
      return (
        <div className="sync-toast sync-toast--ok" role="status">
          {label} · {s.collected}건 수집 · 사실 {s.facts_created}개 · 질문{" "}
          {s.queue_items_created}개
          {s.remaining > 0 && ` · ${s.remaining}건 남음`}
        </div>
      );
    }
    return (
      <div className="sync-toast sync-toast--muted" role="status">
        {label} · 새로 가져올 항목이 없습니다
      </div>
    );
  }
  return null;
}
