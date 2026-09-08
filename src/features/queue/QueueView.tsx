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
import { card, muted } from "../u4-shared/styles";
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
    <section aria-label="인터뷰 대기열">
      <header style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <h2 style={{ margin: 0 }}>인터뷰 대기열</h2>
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
          <ul style={{ listStyle: "none", padding: 0 }}>
            {items.map((item) => (
              <QueueRow
                key={item.id}
                item={item}
                busy={busyId === item.id}
                onAnswer={(input) => answer(item.id, input)}
              />
            ))}
          </ul>
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
      <li style={{ ...card, marginBottom: 8 }}>
        <div style={{ fontWeight: 600 }}>{c.title}</div>
        <p style={{ margin: "4px 0" }}>{c.body}</p>
        <label style={{ display: "block", ...muted, fontSize: 13 }}>
          정정 (선택)
          <input
            type="text"
            value={text}
            placeholder="내용을 고치려면 입력하세요"
            onChange={(e) => setText(e.target.value)}
            style={{ display: "block", width: "100%", marginTop: 4 }}
          />
        </label>
        <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
          <button
            type="button"
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
    <li style={{ ...card, marginBottom: 8 }}>
      <div style={{ fontWeight: 600 }}>{question}</div>
      {hypothesis && <p style={{ ...muted, margin: "4px 0" }}>{hypothesis}</p>}
      <label style={{ display: "block", fontSize: 13 }}>
        답변
        <input
          type="text"
          value={text}
          onChange={(e) => setText(e.target.value)}
          style={{ display: "block", width: "100%", marginTop: 4 }}
        />
      </label>
      <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
        <button
          type="button"
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
