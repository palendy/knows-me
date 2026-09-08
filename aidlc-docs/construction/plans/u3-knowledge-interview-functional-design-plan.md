# U3 (Knowledge & Interview) — Functional Design Plan + Questions

> **Stage**: CONSTRUCTION → Functional Design (per-unit, U3) — **Part 1: Plan + Questions**
> **Unit**: U3 Knowledge & Interview 〔Owner: Dev C〕
> **Depends on**: U1 (shared domain types + `EncryptedStore`). Contracts + mocks already delivered (Milestone 0).
> **Provides (U2·U4 consume)**: `KnowledgeApi`, `InterviewApi`.
> **Enforced extensions**: Property-Based Testing = **Partial** (blocking: PBT-02, PBT-03, PBT-07, PBT-08, PBT-09). Security/Resiliency baselines = disabled.

---

## 1. Unit Context

### Stories in scope
| Story | Title |
|---|---|
| US-3.1 | 사실 문서 저장(메타데이터 포함) |
| US-3.2 | 변경 이력 보존 |
| US-3.3 | 검색·조회 |
| US-4.1 | Queue 아이템 자동 생성(확인형·심화형) |
| US-4.2 | 전환 없는 답변 UX(선택지+직접입력) |
| US-4.3 | 우선순위·만료 |

### Components (from Application Design)
- **Knowledge**: `FactStore` (파일 위키 md/json + 링크), `HistoryTracker` (이력), `SearchIndex` (검색), **KnowledgeService** (오케스트레이션)
- **Interview**: `QueueManager` (확인형/심화형·우선순위·만료), `AnswerIntake` (전환 없는 답변→확정 사실), **InterviewService**
- **Frontend**: `InterviewQueueView` (`src/features/queue/`) — 전환 없는 답변 UX

### Contracts already fixed (Milestone 0 — must be honored)
- `KnowledgeApi`: `upsert / get / links / history / search / graph / dashboard`
- `InterviewApi`: `enqueue / list / answer / expire`
- Shared types: `Fact`, `FactMetadata`, `Provenance`, `FactChange`, `FactCandidate`, `FactSummary`, `FactFilter`, `GraphFilter`, `QueueItem`, `QueueItemKind` (`Confirm`/`Deepen`), `AnswerInput` (`Choice`/`Text`/`Skip`), `AnswerResult`, `QueueSort`, `GraphDto`, `DashboardDto`.
- Persistence gate: `EncryptedStore` (namespaced encrypted KV: `put/get/list/delete`).

### Environment status (verified on this machine — 2026-09-08)
- Rust 1.96.1 + cargo + Apple clang + node 26.7 + npm 11.19 present. `knows_me_core` builds, **5/5 tests pass, clippy clean**.
- **Backend env: ready — no install required.** Open items handled by Q1/Q2 below (frontend scaffold ownership; Rust PBT framework).

---

## 2. Functional Design — Execution Plan (Part 2, after answers approved)

- [x] Resolve all `[Answer]:` below; raise clarification file if any answer is ambiguous — all 11 = A (recommended), consistent, no clarification needed
- [x] `aidlc-docs/construction/u3-knowledge-interview/functional-design/domain-entities.md` — persisted records (fact document format, history record, search index entry, queue record), relationships, mapping to shared types
- [x] `.../functional-design/business-logic-model.md` — KnowledgeService & InterviewService orchestration + algorithms (upsert+history, search/filter, link/graph, queue enqueue, answer intake → confirmed fact + follow-ups, priority scoring, expiry sweep)
- [x] `.../functional-design/business-rules.md` — validation, constraints, invariants (confirmed-only persistence US-3.1/AC3, history-never-overwrite US-3.2, priority ordering + expiry US-4.3, no-force US-4.2/AC2) **+ Testable Properties (PBT-01)** section
- [x] `.../functional-design/frontend-components.md` — `InterviewQueueView` hierarchy, props/state, no-context-switch answer flow, `data-testid` naming, which Tauri commands it calls
- [x] Present Functional Design completion message → **GATE** (Request Changes / Continue to NFR Requirements)

## 3. Preliminary Testable Properties (PBT-01 — refined in artifacts)
- **Round-trip (PBT-02)**: `Fact` (and history/queue records) serialize → deserialize = identity. *(US-3.1 explicitly requires this.)*
- **Invariant (PBT-03)**:
  - After `upsert` of a changed existing fact: history grows by exactly 1, previous body retained, current value identifiable (US-3.2).
  - `search(query, filter)` results all satisfy `filter` and (query non-empty ⇒ match) (US-3.3/AC2).
  - `list(PriorityDesc)` is non-increasing in `priority`; `list(NewestFirst)` non-increasing in `created_at` (US-4.3/AC1).
  - After `expire()`: no remaining item has `expires_at <= now` (US-4.3/AC2).
- **Generators (PBT-07)** / **Shrinking+seed (PBT-08)** / **Framework (PBT-09)**: addressed in NFR Requirements (framework) + Code Generation (generators/CI). Q2 below pre-confirms the framework if desired.

---

## 4. Clarification Questions

Please answer each question by putting a letter after its `[Answer]:` tag. If none fit, pick the "Other" option and describe. First option is my recommendation.

### Environment & Coordination

## Question 1
U3의 프론트엔드 Queue UI(`InterviewQueueView`)를 지금 어떻게 준비할까요? (현재 루트에 프론트 빌드 스캐폴드가 없고, Tauri 앱 셸/`package.json`/Vite/`tauri.conf.json`은 U1 오너십입니다.)

A) **Rust 백엔드 우선** — U3의 `knowledge/`·`interview/` Rust 모듈부터 구현·테스트하고, Queue UI(React)는 U1 앱 셸이 확정된 뒤 통합. 지금 프론트 스캐폴딩은 하지 않음(Functional Design에서 컴포넌트 설계는 문서로 남김). *(권장 — U1 오너십 충돌 회피, 즉시 착수 가능)*

B) 최소 프론트 스캐폴드 병행 — U3가 지금 루트에 Vite+React+TS 최소 스캐폴드를 만들어 Queue UI를 독립(mock invoke)으로 개발하고, 추후 U1 셸과 병합.

C) 풀 Tauri 셸까지 구성 — Tauri CLI 설치 + `tauri.conf.json` 포함 앱 셸을 지금 구성(주의: U1 오너십과 충돌 가능 → Dev A와 사전 합의 필요).

D) Other (please describe after [Answer]: tag below)

[Answer]:  A

## Question 2
Rust PBT 프레임워크를 지금 환경에 선반영할까요? (U3는 PBT Partial 대상: 왕복·불변식. 정식 확정은 NFR Requirements/PBT-09이지만 지금 미리 설치 가능.)

A) **proptest** — 지금 `dev-dependency`로 추가. rule PBT-09 권장, 매크로 기반 + shrinking. *(권장)*

B) quickcheck 를 지금 추가.

C) 지금 설치하지 않고 NFR Requirements 단계에서 확정.

D) Other (please describe after [Answer]: tag below)

[Answer]:  A

### Knowledge (US-3.x)

## Question 3
사실(Fact) 영속화 모델 — US-3.1은 "사람이 읽고 백업·편집할 수 있는 md/json 위키", US-7.1은 저장 암호화를 요구합니다. 둘을 어떻게 조화시킬까요?

A) **암호화 저장이 정본** — `EncryptedStore`(ns="facts", key=`fact_id`)에 직렬화 bytes로 암호화 저장. 사람이 읽는 위키는 앱 내 뷰어 + 명시적 "내보내기(export)" 시에만 평문 md 생성. *(권장 — 보안 기본 충족, 백업은 암호문 파일 복사)*

B) 평문 md/json 파일을 디스크 위키로 저장(가독·백업·편집 우선), 암호화는 선택/후순위.

C) 이중화 — 암호화 저장이 정본 + 잠금 해제 상태에서 평문 위키 폴더로 동기화(외부 편집분은 재암호화 반영).

D) Other (please describe after [Answer]: tag below)

[Answer]:  A

## Question 4
사실 문서 직렬화 형식 (PBT-02 왕복 대상)?

A) **JSON** (serde_json) — 왕복 안정적, 기계 처리 용이. *(권장)*

B) Markdown + YAML frontmatter — 본문=md, 메타=frontmatter(위키 친화, 왕복 시 정규화 규칙 필요).

C) 둘 다 — JSON을 정본으로 저장하고 Markdown은 렌더/내보내기용 파생.

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 5
사실 링크(위키 그래프 엣지) 생성 주체/방식? (`FactStore::links` / `graph`)

A) **명시적 저장 링크만** — `Fact.links`에 담긴 것만 엣지로. 링크를 채우는 로직은 답변 intake/가공에서 점진 추가하고, 이 단계에서는 저장·조회 정확성만 보장. *(권장 — 범위 명확, MVP 단순)*

B) U3가 LLM(U1 `LlmClient`)으로 사실 간 관련성을 추론해 링크 자동 제안.

C) 제목/키워드 규칙 기반 로컬 자동 링크(LLM 없이).

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 6
변경 이력 저장 단위 (US-3.2)?

A) **전체 body 스냅샷** — 변경 시 `before`/`after` 전체 저장(현재 mock과 동일). 단순·복원 용이. *(권장)*

B) 필드 단위 diff(title/scope/body 각각 추적).

C) 라인 단위 텍스트 diff.

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 7
검색 접근 (US-3.3 — NFR-4: 수천~수만 사실에서 반응성)?

A) **인메모리 역색인** — 잠금 해제 시 사실을 로드하며 인덱스 구축, upsert마다 증분 갱신. 외부 검색 크레이트 없이 시작(수만 규모까지 충분 가정). *(권장 — 의존성 최소, 로컬 앱 적합)*

B) 임베디드 검색 엔진(tantivy 등) — 확장성↑, 의존성·복잡도↑.

C) 선형 스캔(현재 mock) 유지 — 가장 단순하나 대규모에서 성능 저하.

D) Other (please describe after [Answer]: tag below)

[Answer]: A

### Interview (US-4.x)

## Question 8
Queue 아이템 생성 주체? 확인형(Confirm)은 가공(U2)이 후보→`enqueue` 합니다. 심화형(Deepen)과 후속 질문(US-4.1/AC3)은 누가 만드나요?

A) **U3가 심화형·후속 질문 생성 담당** — `AnswerIntake`가 U1 `LlmClient`(+`Masker`)로 답변 키워드→후속 질문 파생. ⇒ U3가 `LlmClient`/`Masker`에 의존(계약 이미 존재). *(권장 — AC3 충족)*

B) 모든 큐 아이템(확인·심화·후속)은 외부(U2)가 만들어 `enqueue`만; U3는 큐 관리·답변 수용만 담당.

C) 규칙/템플릿 기반 후속 질문(LLM 없이).

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 9
답변 처리 결과 규칙 (US-4.2/AC3, US-4.1/AC3)?

A) **Confirm+긍정 → 후보를 확정 Fact로 upsert. Confirm+부정/거부 → 후보 폐기(사실 저장 안 함). Deepen+텍스트 → 확정 Fact 생성 + 후속 질문 파생.** *(권장)*

B) 모든 답변을 사실로 기록(부정도 "아니다"라는 사실로 저장).

C) Deepen 답변은 기존 사실 갱신만 하고 신규 사실은 만들지 않음.

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 10
우선순위·만료 정책 (US-4.3)?

A) **가중 우선순위 + TTL 만료** — priority(0–255)는 소스 신뢰도·최근성·확인필요도 가중, 만료는 생성 후 TTL(예: 확인형 14일 / 심화형 30일) + 동일 주제 반복 억제. 구체 수치를 이 단계에서 확정. *(권장)*

B) 단순 — 고정 우선순위 + 만료 없음(수동 정리).

C) 사용자 설정 가능한 정책(설정 화면에서 조정).

D) Other (please describe after [Answer]: tag below)

[Answer]: A

### Frontend (US-4.2 — 설계 문서는 스캐폴딩 여부와 무관하게 작성)

## Question 11
전환 없는 답변 UX 상호작용 모델 (US-4.2/AC1)?

A) **리스트 인라인 확장** — 각 아이템 카드에서 바로 선택지 버튼 + 자유 입력 + Skip을 같은 화면에 노출하고, 답하면 카드가 갱신/제거. *(권장 — "전환 없음" 요건에 가장 부합)*

B) 좌측 리스트 + 우측 답변 패널(같은 화면 분할).

C) 상단 단일 포커스 카드(한 번에 한 질문, 아래 대기열 미리보기).

D) Other (please describe after [Answer]: tag below)

[Answer]: A
