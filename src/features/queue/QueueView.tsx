// US-4.x — the interview queue. This is the only UI path that *confirms* facts,
// so without it the mini-home and persona stay empty even when the backend works.
//
// Confirm items → 확정 / 거부 buttons, with an optional correction that confirms
// the corrected body (Text). Deepen items → a free-text answer. Skip holds the
// item. Answering refreshes the list and signals the shell so the dashboard's
// pending-queue count and the confirmed views update too.

import { useCallback, useEffect, useState } from "react";
import type { AnswerInput, QueueItem } from "../../shared/contracts";
import type { InterviewApi } from "./api";
import { StateShell } from "../u4-shared/StateShell";
import "../u4-shared/work-views.css";
import { load, loading, messageOf, type ViewState } from "../u4-shared/view-state";

interface Props {
  api: InterviewApi;
  /** Called after any answer so sibling views (dashboard/mini-home) can refresh. */
  onChanged?: () => void;
}

export function QueueView({ api, onChanged }: Props) {
  const [state, setState] = useState<ViewState<QueueItem[]>>(loading);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setState(loading());
    void load(
      () => api.list("PriorityDesc"),
      (items) => items.length === 0,
    ).then(setState);
  }, [api]);

  useEffect(refresh, [refresh]);

  const answer = useCallback(
    async (id: string, input: AnswerInput) => {
      setBusyId(id);
      setActionError(null);
      try {
        await api.answer(id, input);
        onChanged?.();
        refresh();
      } catch (e) {
        setActionError(messageOf(e));
      } finally {
        setBusyId(null);
      }
    },
    [api, onChanged, refresh],
  );

  return (
    <section aria-label="인터뷰 대기열" className="queue-workspace">
      <header className="work-view-header">
        <div><h2>인터뷰 대기열</h2><p>짧은 확인으로, 나를 더 정확하게 이해하도록.</p></div>
        <button type="button" onClick={refresh}>
          새로고침
        </button>
      </header>

      {actionError && (
        <p role="alert" style={{ color: "#b00020" }}>
          {actionError}
        </p>
      )}

      <StateShell
        state={state}
        emptyMessage="확인할 질문이 없습니다. 수집·가공이 진행되면 여기에 질문이 쌓입니다."
        onRetry={refresh}
      >
        {(items) => (
          <><div className="queue-summary"><span>확인을 기다리는 질문 <strong>{items.length}</strong></span><span>우선순위 높은 순</span></div><ul className="queue-list">
            {items.map((item) => (
              <QueueRow
                key={item.id}
                item={item}
                busy={busyId === item.id}
                onAnswer={(input) => answer(item.id, input)}
              />
            ))}
          </ul></>
        )}
      </StateShell>
    </section>
  );
}

function QueueRow({
  item,
  busy,
  onAnswer,
}: {
  item: QueueItem;
  busy: boolean;
  onAnswer: (input: AnswerInput) => void;
}) {
  const [text, setText] = useState("");

  if ("Confirm" in item.kind) {
    const c = item.kind.Confirm.candidate;
    return (
      <li className="queue-card">
        <QueueMetadata item={item} />
        <h3>{c.title}</h3>
        <p className="queue-body">{c.body}</p>
        <div className="queue-provenance"><span>{({Session: "대화 기록", Notion: "Notion", Gmail: "Gmail", File: "파일"})[c.provenance.source]}</span><span>{({Company: "업무", Personal: "개인", Unknown: "미분류"})[c.suggested_scope]}</span><span>{formatDate(c.provenance.collected_at)} 수집</span></div>
        <label className="queue-answer-label">
          정정 (선택)
          <input
            type="text"
            value={text}
            placeholder="내용을 고치려면 입력하세요"
            onChange={(e) => setText(e.target.value)}
          />
        </label>
        <div className="queue-actions">
          <button
            type="button"
            className="work-primary"
            disabled={busy}
            onClick={() =>
              onAnswer(text.trim() ? { Text: text.trim() } : { Choice: "yes" })
            }
          >
            {text.trim() ? "정정 후 확정" : "확정"}
          </button>
          <button type="button" disabled={busy} onClick={() => onAnswer({ Choice: "no" })}>
            거부
          </button>
          <button type="button" disabled={busy} onClick={() => onAnswer("Skip")}>
            나중에
          </button>
        </div>
      </li>
    );
  }

  const { question, hypothesis } = item.kind.Deepen;
  return (
    <li className="queue-card">
      <QueueMetadata item={item} />
      <h3>{question}</h3>
      {hypothesis && <p className="queue-body">{hypothesis}</p>}
      <label className="queue-answer-label">
        답변
        <input
          type="text"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="내 생각을 간단히 남겨주세요"
        />
      </label>
      <div className="queue-actions">
        <button
          type="button"
          className="work-primary"
          disabled={busy || text.trim() === ""}
          onClick={() => onAnswer({ Text: text.trim() })}
        >
          답변 저장
        </button>
        <button type="button" disabled={busy} onClick={() => onAnswer("Skip")}>
          나중에
        </button>
      </div>
    </li>
  );
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString("ko-KR", { month: "short", day: "numeric" });
}

function QueueMetadata({ item }: { item: QueueItem }) {
  return <div className="queue-metadata"><span className="work-tag">{"Confirm" in item.kind ? "사실 확인" : "더 알아가기"}</span><span>우선순위 {item.priority}</span><time dateTime={item.created_at}>{formatDate(item.created_at)}</time>{item.expires_at && <span>{formatDate(item.expires_at)}까지</span>}</div>;
}
