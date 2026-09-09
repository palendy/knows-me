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
import type { IngestProgress, IngestSummary, SourcesApi } from "./api";
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
  | { status: "running"; card: string | "all" }
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

  async function sync(card?: CardModel) {
    const key = card?.id ?? "all";
    runningCard.current = card ?? "all";
    setProgress(null);
    setRun({ status: "running", card: key });
    try {
      const summary = await api.triggerIngest(card?.sourceKind);
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
          onClick={() => void sync()}
          disabled={busy}
        >
          {busy && run.card === "all" ? "수집 중…" : "전체 수집"}
        </button>
      </header>

      {loadError && (
        <p role="alert" style={{ color: "#c0392b" }}>
          {loadError}
        </p>
      )}

      {/* Aggregate result for "전체 수집". Per-card runs render inside the card. */}
      {run.status !== "idle" && run.card === "all" && (
        <div style={{ marginTop: 12 }}>
          <SyncResult run={run} cardId="all" progress={progress} />
        </div>
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

                <SyncResult run={run} cardId={card.id} progress={progress} />
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

/** Inline sync feedback for one card (or the "전체 수집" summary when
 * `cardId === "all"`). Distinguishes three outcomes so a run of all-zeros isn't
 * ambiguous: an error (with its cause), items collected, or a clean "nothing
 * new" — which confirms the connection works but had no fresh data. */
function SyncResult({
  run,
  cardId,
  progress,
}: {
  run: RunState;
  cardId: string;
  progress: IngestProgress | null;
}) {
  if (run.status === "running" && run.card === cardId) {
    // Show a determinate bar once the connector reports done/total; until then
    // (or for sources that don't report), a simple "수집 중…" label.
    const pct =
      progress && progress.total > 0
        ? Math.min(100, Math.round((progress.done / progress.total) * 100))
        : null;
    return (
      <div className="source-result">
        <div className="source-progress-label">
          {progress && progress.total > 0
            ? `수집 중… ${progress.done}/${progress.total}`
            : progress && progress.done > 0
              ? `수집 중… ${progress.done}건`
              : "수집 중…"}
        </div>
        <div className="source-progress-track">
          <div
            className={`source-progress-fill${pct === null ? " is-indeterminate" : ""}`}
            style={pct === null ? undefined : { width: `${pct}%` }}
          />
        </div>
      </div>
    );
  }
  if (run.status === "error" && run.card === cardId) {
    return (
      <div className="source-result source-result--error" role="alert">
        수집 실패: {run.message}
      </div>
    );
  }
  if (run.status === "done" && run.card === cardId) {
    const s = run.summary;
    if (s.errors > 0) {
      return (
        <div className="source-result source-result--error" role="alert">
          오류 {s.errors}건
          {s.error_messages.length > 0 && (
            <>
              {" — "}
              {s.error_messages.join("; ")}
            </>
          )}
        </div>
      );
    }
    if (s.collected > 0) {
      return (
        <div className="source-result source-result--ok" role="status">
          {s.collected}건 수집 · 사실 {s.facts_created}개 · 질문{" "}
          {s.queue_items_created}개
          {s.remaining > 0 && (
            <div style={{ marginTop: 4, color: "#475569" }}>
              아직 {s.remaining}건 남음 — 다시 눌러 이어서 수집
            </div>
          )}
        </div>
      );
    }
    return (
      <div className="source-result source-result--muted" role="status">
        연결됨 · 새로 가져올 항목이 없습니다
      </div>
    );
  }
  return null;
}
