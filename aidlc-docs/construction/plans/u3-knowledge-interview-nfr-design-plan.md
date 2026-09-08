# U3 (Knowledge & Interview) — NFR Design Plan + Questions

> **Stage**: CONSTRUCTION → NFR Design (per-unit, U3) — **Part 1: Plan + Questions**
> **Prereq**: U3 NFR Requirements APPROVED. Reads `aidlc-docs/construction/u3-knowledge-interview/nfr-requirements/`.
> **Purpose**: Turn U3's NFR requirements into concrete design patterns + logical components (no new infra — local desktop).

---

## 1. Category applicability (per nfr-design.md — evaluate ALL)
| Category | Applicable to U3? | Note |
|---|---|---|
| **Resilience patterns** | ✅ | LLM failure/offline for follow-up gen; store failure → self-healing index. Q2 refines. |
| **Scalability patterns** | ⚠️ N/A (single-user) | No horizontal scaling; "scale" = in-corpus size, handled by index (NFR-4). |
| **Performance patterns** | ✅ | In-memory inverted index; index startup strategy (Q1); history lazy-load (Q3). |
| **Security patterns** | ➖ mostly U1 | U3 patterns = mask-before-LLM + no-plaintext-outside-EncryptedStore (already fixed as KR-7/KR-8). No new question. |
| **Logical components** | ✅ | Enumerate U3 logical components; confirm no queues/caches/circuit-breakers beyond the in-memory index. |

## 2. NFR Design — Execution Plan (Part 2, after answers approved)
- [x] Resolve all `[Answer]:` below — all 3 = A (recommended), consistent, no clarification needed
- [x] `aidlc-docs/construction/u3-knowledge-interview/nfr-design/nfr-design-patterns.md` — patterns applied per NFR (performance: index strategy; resilience: LLM fallback + self-healing index; privacy: mask gate; portability: export) with how/where
- [x] `.../nfr-design/logical-components.md` — U3 logical components (KnowledgeService, InterviewService, FactStore, HistoryTracker, SearchIndex, QueueManager, AnswerIntake) + their integration and the deliberate absence of extra infra
- [x] Present NFR Design completion message → **GATE** (Request Changes / Continue to Code Generation)

---

## 3. Clarification Questions

Answer each with a letter after `[Answer]:`. First option is my recommendation.

## Question 1
검색 인덱스 시작(startup) 전략 (NFR-4: 잠금 해제 후 검색 반응성)?

A) **잠금 해제 시 매번 재구축** — `ns=facts`를 읽어 인메모리 색인 구축(<2s @50k), 항상 정합. 디스크에 색인 캐시 없음. *(권장 — 단순·정합성 보장, 재구축 비용 허용)*

B) 색인을 **암호화해 디스크 캐시** → 다음 실행 시 빠르게 로드. 시작 빠르나 캐시-정본 불일치 관리 필요.

C) 하이브리드 — 캐시 로드 후 백그라운드로 정합성 검증/보정.

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 2
LLM 호출 실패·타임아웃 시 후속 질문 생성 복원력 패턴 (NFR-3, Deepen 답변 경로)?

A) **즉시 폴백 + 로그, 재시도 없음** — `LlmClient` 오류/타임아웃 시 후속 질문을 건너뛰고(사실 저장은 유지) 사유를 로깅. MVP. *(권장 — NFR-3 부합, 단순)*

B) 짧은 지수 백오프 1~2회 재시도 후 폴백.

C) 보류 큐에 적재 후 온라인 복귀 시 자동 재시도(상태·복잡도↑).

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 3
이력·사실 메모리 로딩 패턴 (NFR-4 메모리 효율)?

A) **이력 지연 로드** — 인메모리에는 사실 색인만 유지, `fact_history[id]`는 `history(id)` 호출 시 store에서 로드. 메모리 절약, 이력 조회는 드묾. *(권장)*

B) 사실+이력 모두 메모리 상주(조회 최속, 메모리↑).

C) LRU 캐시로 최근 접근 사실/이력만 상주.

D) Other (please describe after [Answer]: tag below)

[Answer]: A
