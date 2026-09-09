// US-1.x — source collection.
//
// One screen to see which sources are connected, connect/disconnect the ones
// that need credentials, and trigger a sync. The catalog (which sources exist,
// what credential fields each needs) comes from the backend `list_sources`
// command — this view only maps each kind to a human label/description and
// renders the connection state. The sync counts are deliberately specific
// ("12 collected, 5 facts, 4 questions") because "done" tells the owner nothing
// about whether their data actually made it in.

import { useCallback, useEffect, useState } from "react";
import type { SourceKind, SourceStatus } from "../../shared/contracts";
import type { IngestSummary, SourcesApi } from "./api";
import { messageOf } from "../u4-shared/view-state";
import { ConnectDialog } from "./ConnectDialog";

interface Props {
  api: SourcesApi;
  /** Called after a successful sync so other views can pull fresh counts. */
  onIngested?: () => void;
}

/** Human-facing label + description per source kind (the backend spec carries
 * only the credential fields, not display copy). */
const META: Record<SourceKind, { label: string; detail: string }> = {
  Session: {
    label: "에이전트 세션",
    detail: "~/.claude/projects, ~/.codex/sessions 의 대화 기록 — 자격증명 불필요",
  },
  File: {
    label: "파일",
    detail: "텍스트·마크다운 파일 (PDF/DOCX는 향후)",
  },
  Notion: {
    label: "Notion",
    detail: "Internal Integration Token으로 연결",
  },
  Gmail: {
    label: "Gmail",
    detail: "Gmail 주소 + 앱 비밀번호로 연결",
  },
};

type RunState =
  | { status: "idle" }
  | { status: "running"; source: SourceKind | "all" }
  | { status: "done"; source: SourceKind | "all"; summary: IngestSummary }
  | { status: "error"; source: SourceKind | "all"; message: string };

export function SourcesView({ api, onIngested }: Props) {
  const [sources, setSources] = useState<SourceStatus[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [run, setRun] = useState<RunState>({ status: "idle" });
  const [connecting, setConnecting] = useState<SourceStatus | null>(null);

  const refresh = useCallback(async () => {
    try {
      setSources(await api.listSources());
      setLoadError(null);
    } catch (e) {
      setLoadError(messageOf(e));
    }
  }, [api]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function sync(source?: SourceKind) {
    const key = source ?? "all";
    setRun({ status: "running", source: key });
    try {
      const summary = await api.triggerIngest(source);
      setRun({ status: "done", source: key, summary });
      onIngested?.();
    } catch (e) {
      setRun({ status: "error", source: key, message: messageOf(e) });
    }
  }

  async function disconnect(source: SourceKind) {
    try {
      await api.disconnectSource(source);
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
        <button type="button" onClick={() => void sync()} disabled={busy}>
          {busy && run.source === "all" ? "수집 중…" : "전체 수집"}
        </button>
      </header>

      {loadError && (
        <p role="alert" style={{ color: "#c0392b" }}>
          {loadError}
        </p>
      )}

      {sources === null ? (
        <p style={{ color: "#6b6b70" }}>불러오는 중…</p>
      ) : (
        <ul style={{ listStyle: "none", padding: 0, marginTop: 12 }}>
          {sources.map((s) => {
            const meta = META[s.kind];
            const needsCredentials = s.fields.length > 0;
            return (
              <li
                key={s.kind}
                style={{
                  border: "1px solid #e3e3e6",
                  borderRadius: 10,
                  padding: "12px 14px",
                  marginBottom: 8,
                  display: "flex",
                  alignItems: "center",
                  gap: 12,
                }}
              >
                <div style={{ flex: 1 }}>
                  <div style={{ fontWeight: 600 }}>
                    {meta.label}{" "}
                    <span
                      style={{
                        fontSize: 11,
                        padding: "2px 8px",
                        borderRadius: 999,
                        background: s.ready ? "#eaf7ee" : "#f1f1f3",
                        color: s.ready ? "#1c7a3d" : "#5c5c62",
                      }}
                    >
                      {s.ready
                        ? needsCredentials
                          ? "연결됨"
                          : "사용 가능"
                        : "연결 필요"}
                    </span>
                  </div>
                  <div style={{ color: "#6b6b70", fontSize: 13 }}>
                    {meta.detail}
                  </div>
                </div>

                {needsCredentials &&
                  (s.connected ? (
                    <>
                      <button type="button" onClick={() => setConnecting(s)}>
                        재설정
                      </button>
                      <button
                        type="button"
                        onClick={() => void disconnect(s.kind)}
                      >
                        연결 해제
                      </button>
                    </>
                  ) : (
                    <button type="button" onClick={() => setConnecting(s)}>
                      연결
                    </button>
                  ))}

                <button
                  type="button"
                  onClick={() => void sync(s.kind)}
                  disabled={busy || !s.ready}
                  title={s.ready ? undefined : "먼저 연결하세요"}
                >
                  {busy && run.source === s.kind ? "수집 중…" : "수집"}
                </button>
              </li>
            );
          })}
        </ul>
      )}

      {connecting && (
        <ConnectDialog
          source={connecting}
          label={META[connecting.kind].label}
          onConnect={(source, values) => api.connectSource(source, values)}
          onClose={() => setConnecting(null)}
          onConnected={() => void refresh()}
        />
      )}

      {run.status === "done" && (
        <div role="status" style={{ marginTop: 12 }}>
          <strong>수집 완료</strong>
          <ul>
            <li>
              수집 {run.summary.collected}건 · 건너뜀 {run.summary.skipped}건 ·
              오류 {run.summary.errors}건
            </li>
            <li>
              확정 사실 {run.summary.facts_created}개 · 질문{" "}
              {run.summary.queue_items_created}개 · 걸러냄{" "}
              {run.summary.filtered}건
            </li>
          </ul>
          {run.summary.remaining > 0 ? (
            <p style={{ color: "#6b6b70", fontSize: 13 }}>
              아직 <strong>{run.summary.remaining}건</strong>이 남아 있습니다. 한 번에
              일부만 가져오므로, 같은 항목을 다시 가져오는 것이 아니라 과거
              기록을 이어서 처리합니다. 계속하려면 다시 눌러 주세요.
            </p>
          ) : (
            <p style={{ color: "#1c7a3d", fontSize: 13 }}>
              이 소스는 모두 가져왔습니다.
            </p>
          )}
          {run.summary.collected === 0 && run.summary.remaining === 0 && (
            <p style={{ color: "#6b6b70", fontSize: 13 }}>
              새로 수집된 항목이 없습니다. 이미 가져온 항목은 다시 저장하지 않습니다.
            </p>
          )}
        </div>
      )}

      {run.status === "error" && (
        <div role="alert" style={{ marginTop: 12 }}>
          <p>{run.message}</p>
          <button type="button" onClick={() => void sync()}>
            다시 시도
          </button>
        </div>
      )}
    </section>
  );
}
