// US-6.1 — chat with the persona built from confirmed context.
//
// The masking notice is permanent, not a one-time toast: AC2 and NFR-2 are
// about the owner *knowing* what leaves the device, and a notice they have
// already dismissed does not tell them anything.

import { useState, type FormEvent } from "react";
import type { ChatTurn, FactRef } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";
import { card, muted } from "../u4-shared/styles";
import { messageOf } from "../u4-shared/view-state";

interface Props {
  api: KnowsMeApi;
}

interface Message {
  role: "user" | "persona";
  text: string;
  /** Facts this answer was grounded in (persona turns only). */
  sources?: FactRef[];
  error?: string;
}

/** The thread as the backend wants it: prior turns, oldest first. */
function toHistory(messages: Message[]): ChatTurn[] {
  return messages
    .filter((m) => !m.error)
    .map((m) => ({
      role: m.role === "user" ? ("Owner" as const) : ("Persona" as const),
      text: m.text,
    }));
}

const MAX_PROMPT_CHARS = 4000;

/**
 * Exported so the test suite exercises the exact questions the view offers —
 * an example button that answers "no context" is a broken demo, and the two
 * lists silently drifting apart is how that ships unnoticed.
 */
export const EXAMPLE_QUESTIONS = [
  "내 배포 절차 알려줘",
  "코드 리뷰는 어떻게 하지?",
  "내가 선호하는 에디터가 뭐였지?",
];

export function PersonaChatView({ api }: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);

  const canSend = input.trim().length > 0 && input.length <= MAX_PROMPT_CHARS && !sending;

  async function send(prompt: string) {
    setSending(true);
    // Snapshot the thread *before* appending, so the history sent is what came
    // earlier rather than including the question being asked.
    const history = toHistory(messages);
    setMessages((m) => [...m, { role: "user", text: prompt }]);
    setInput("");
    try {
      const reply = await api.personaChat(prompt, history);
      setMessages((m) => [
        ...m,
        { role: "persona", text: reply.text, sources: reply.sources },
      ]);
    } catch (err) {
      // Keep the conversation; mark just this turn as failed so an outage
      // never costs the owner what they already typed (BR-E2).
      setMessages((m) => {
        const next = [...m];
        const last = next[next.length - 1];
        if (last) next[next.length - 1] = { ...last, error: messageOf(err) };
        return next;
      });
    } finally {
      setSending(false);
    }
  }

  function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (!canSend) return;
    void send(input.trim());
  }

  const lastUserPrompt = [...messages].reverse().find((m) => m.role === "user");
  const canFollowUp = messages.some((m) => m.role === "persona");

  return (
    <section aria-label="페르소나 챗">
      <h2>페르소나 챗</h2>

      <p role="note" style={{ ...muted, ...card }}>
        외부 LLM에 보낼 때 이메일·전화번호 등 식별자는 마스킹되어 전송되고, 답변은
        기기 안에서 원문으로 복원됩니다.
      </p>

      {messages.length === 0 ? (
        <div style={muted}>
          <p>확정된 내 맥락을 근거로 답합니다. 예를 들어:</p>
          <ul>
            {EXAMPLE_QUESTIONS.map((q) => (
              <li key={q}>
                <button type="button" onClick={() => void send(q)}>
                  {q}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <ul aria-label="대화" style={{ listStyle: "none", padding: 0 }}>
          {messages.map((m, i) => (
            <li key={i} style={{ ...card, marginBottom: 8 }}>
              <div style={muted}>{m.role === "user" ? "나" : "페르소나"}</div>
              <div style={{ whiteSpace: "pre-wrap" }}>{m.text}</div>
              {m.sources && m.sources.length > 0 && (
                <details style={{ ...muted, marginTop: 6 }}>
                  <summary>근거로 삼은 사실 {m.sources.length}개</summary>
                  <ul style={{ margin: "6px 0 0", paddingLeft: 18 }}>
                    {m.sources.map((s) => (
                      <li key={s.id}>{s.title}</li>
                    ))}
                  </ul>
                </details>
              )}
              {m.error && (
                <div role="alert">
                  <span>{m.error}</span>{" "}
                  <button
                    type="button"
                    onClick={() => lastUserPrompt && void send(lastUserPrompt.text)}
                  >
                    다시 시도
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      <form onSubmit={onSubmit}>
        <label htmlFor="persona-input">
          {canFollowUp ? "이어서 질문 (앞의 대화를 기억합니다)" : "질문"}
        </label>
        <textarea
          id="persona-input"
          value={input}
          rows={3}
          maxLength={MAX_PROMPT_CHARS}
          onChange={(e) => setInput(e.target.value)}
          style={{ width: "100%" }}
        />
        <button type="submit" disabled={!canSend}>
          {sending ? "보내는 중…" : "보내기"}
        </button>
      </form>
    </section>
  );
}
