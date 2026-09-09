// US-6.1 — chat with the persona built from confirmed context.
//
// The masking notice is permanent, not a one-time toast: AC2 and NFR-2 are
// about the owner *knowing* what leaves the device, and a notice they have
// already dismissed does not tell them anything.

import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import ReactMarkdown from "react-markdown";
import type { ChatTurn, FactRef, StoredTurn } from "../../shared/contracts";
import type { KnowsMeApi } from "../u4-shared/api";
import "../u4-shared/work-views.css";
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

/** The thread as the vault stores it — what was on screen, sources included. */
function toStored(messages: Message[]): StoredTurn[] {
  return messages.map((m) => ({
    role: m.role === "user" ? ("Owner" as const) : ("Persona" as const),
    text: m.text,
    sources: m.sources ?? [],
    error: m.error ?? null,
  }));
}

function fromStored(turns: StoredTurn[]): Message[] {
  return turns.map((t) => ({
    role: t.role === "Owner" ? ("user" as const) : ("persona" as const),
    text: t.text,
    sources: t.sources.length > 0 ? t.sources : undefined,
    error: t.error ?? undefined,
  }));
}

const MAX_PROMPT_CHARS = 4000;

/**
 * Exported so the test suite exercises the exact questions the view offers —
 * an example button that answers "no context" is a broken demo, and the two
 * lists silently drifting apart is how that ships unnoticed.
 */
export const EXAMPLE_QUESTIONS = [
  // A self-directed question: this one is answered from topic pages ranked by
  // friction, not by keyword search, so it exercises the path that makes the
  // persona worth asking. The dropped example ("선호하는 에디터") asked about a
  // preference nothing in the vault records — a suggestion that reliably
  // answers "모른다" teaches the owner the app does not know them.
  "내가 요즘 제일 걱정하는 게 뭘까?",
  "내 배포 절차 알려줘",
  "코드 리뷰는 어떻게 하지?",
];

export function PersonaChatView({ api }: Props) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  // The saved thread arrives asynchronously; until it does, a save would
  // overwrite the vault with an empty list.
  const [loaded, setLoaded] = useState(false);
  const listRef = useRef<HTMLUListElement>(null);

  // Resume where the owner left off. The thread lives in the encrypted vault,
  // so a locked or unreadable vault simply means starting fresh.
  useEffect(() => {
    let cancelled = false;
    api
      .loadChatHistory()
      .then((turns) => {
        if (!cancelled) setMessages(fromStored(turns));
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [api]);

  // Persist after every change once the initial load has settled. A failed
  // save is not surfaced: the conversation on screen is intact either way.
  useEffect(() => {
    if (!loaded) return;
    void api.saveChatHistory(toStored(messages)).catch(() => {});
  }, [api, messages, loaded]);

  // Keep the newest turn in view. `scrollIntoView` is a browser API that not
  // every host provides (jsdom, some embedded webviews), so it is a courtesy,
  // never a requirement.
  useEffect(() => {
    const last = listRef.current?.lastElementChild;
    if (last && typeof last.scrollIntoView === "function") {
      last.scrollIntoView({ block: "end" });
    }
  }, [messages, sending]);

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

  // Enter sends, Shift+Enter breaks the line — the convention every chat
  // client the owner already uses follows. IME composition (Korean input
  // assembles syllables before committing) must not be interrupted: an Enter
  // that lands mid-composition is the IME's, not ours.
  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
    e.preventDefault();
    if (canSend) void send(input.trim());
  }

  function startNew() {
    setMessages([]);
    setInput("");
  }

  const lastUserPrompt = [...messages].reverse().find((m) => m.role === "user");
  const canFollowUp = messages.some((m) => m.role === "persona");

  return (
    <section aria-label="나와 대화" className="chat-workspace">
      <header className="work-view-header">
        <div><h2>나와 대화</h2><p>쌓아 둔 맥락에서 나에게 필요한 답을 찾아보세요.</p></div>
        {messages.length > 0 && (
          <button type="button" className="chat-new" onClick={startNew} disabled={sending}>
            새 대화
          </button>
        )}
      </header>
      <div className="chat-content">

      {messages.length === 0 ? (
        <div className="chat-welcome">
          <span className="chat-monogram" aria-hidden="true">k.</span>
          <h3>나에 대해, 궁금한 것이 있나요?</h3>
          <p>내가 확인한 기록을 바탕으로<br />생각과 취향, 일하는 방식을 돌아보세요.</p>
          <ul className="chat-suggestions">
            {EXAMPLE_QUESTIONS.map((q) => (
              <li key={q}>
                <button type="button" onClick={() => void send(q)}>
                  {q}<span aria-hidden="true">↗</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <ul aria-label="대화" className="chat-messages" aria-live="polite" ref={listRef}>
          {messages.map((m, i) => (
            <li key={i} className={`chat-message chat-message--${m.role}`}>
              <div className="chat-speaker">{m.role === "user" ? "나" : "knows me"}</div>
              {m.role === "persona" ? (
                // Persona answers arrive as markdown (lists, emphasis, code);
                // shown raw, the asterisks and hashes read as noise. The
                // owner's own turns are plain text and stay that way — a
                // literal asterisk they typed should look like one.
                <div className="chat-bubble chat-markdown">
                  <ReactMarkdown>{m.text}</ReactMarkdown>
                </div>
              ) : (
                <div className="chat-bubble">{m.text}</div>
              )}
              {m.sources && m.sources.length > 0 && (
                <details className="chat-sources">
                  <summary>근거로 삼은 사실 {m.sources.length}개</summary>
                  <ul>
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
      {sending && <p className="chat-pending" role="status">내 맥락을 살펴보고 있어요…</p>}
      <form onSubmit={onSubmit} className="chat-composer">
        <label htmlFor="persona-input" className="chat-input-label">
          {canFollowUp ? "이어서 질문 (앞의 대화를 기억합니다)" : "질문"}
        </label>
        <textarea
          id="persona-input"
          value={input}
          rows={3}
          maxLength={MAX_PROMPT_CHARS}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="나에 대해 궁금한 것을 물어보세요 — Enter로 보내고, Shift+Enter로 줄을 바꿉니다"
        />
        <div className="chat-composer-footer"><span>{input.length.toLocaleString()} / 4,000</span><button type="submit" className="work-primary" disabled={!canSend}>
          {sending ? "보내는 중…" : "보내기"}
        </button></div>
      </form>
      <p role="note" className="chat-privacy">
        외부 LLM에 보낼 때 이메일·전화번호 등 식별자는 마스킹되어 전송되고, 답변은
        기기 안에서 원문으로 복원됩니다. 대화는 암호화된 저장소에 보관됩니다.
      </p>
      </div>
    </section>
  );
}
