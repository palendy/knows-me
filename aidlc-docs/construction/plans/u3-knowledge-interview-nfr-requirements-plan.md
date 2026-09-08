# U3 (Knowledge & Interview) — NFR Requirements Plan + Questions

> **Stage**: CONSTRUCTION → NFR Requirements (per-unit, U3) — **Part 1: Plan + Questions**
> **Prereq**: U3 Functional Design APPROVED. Reads `aidlc-docs/construction/u3-knowledge-interview/functional-design/`.
> **Relevant NFRs** (requirements.md §4): NFR-2 프라이버시, NFR-3 로컬/오프라인, **NFR-4 규모(검색 반응성)**, NFR-5 멱등, NFR-6 이식성, NFR-8 PBT(Partial).
> **Infrastructure Design**: SKIP (local desktop app, no cloud infra — per execution plan).

---

## 1. What NFR Requirements decides for U3
U3 is data-centric and local. The material NFR decisions are: **search performance at scale (NFR-4)**, **offline/graceful degradation of the one LLM-dependent path (NFR-3, follow-up generation)**, **portability/backup vs encrypted-canonical storage (NFR-6 ↔ FD Q3)**, **tokenization for KO/EN content**, **concurrency/consistency**, and **tech-stack confirmation (PBT-09, serialization, tokenizer)**.

Already fixed upstream (not re-litigated here): local-only + encryption (U1/SecurityService), masking before LLM (FD KR-7), in-memory inverted index (FD Q7), JSON serialization (FD Q4).

## 2. NFR Assessment — Execution Plan (Part 2, after answers approved)
- [x] Resolve all `[Answer]:` below; raise clarification file if any answer is ambiguous — all 6 = A (recommended), consistent, no clarification needed
- [x] `aidlc-docs/construction/u3-knowledge-interview/nfr-requirements/nfr-requirements.md` — per-NFR requirements for U3 (perf budget, scale, offline behavior, reliability, portability, privacy, testability) with measurable targets + traceability
- [x] `.../nfr-requirements/tech-stack-decisions.md` — confirmed libraries/approaches (serde_json, proptest, tokenizer approach, async_trait/runtime, storage via EncryptedStore) with rationale; PBT-09 framework record
- [x] Present NFR Requirements completion message → **GATE** (Request Changes / Continue to NFR Design)

---

## 3. Clarification Questions

Answer each with a letter after `[Answer]:`. First option is my recommendation.

## Question 1
검색 규모·지연 목표 (NFR-4: "수천~수만 사실에서 반응성")?

A) **최대 ~50,000 사실, 검색 p95 < 100ms** 목표 — 인메모리 역색인으로 충족(잠금 해제 시 구축, 증분 갱신). 인덱스 메모리 오버헤드 허용. *(권장 — 1인 규모에 충분, 의존성 최소)*

B) ~10,000 사실, < 50ms — 더 보수적 목표(메모리·구축시간 최소).

C) 100,000+ 확장 대비 — 임베디드 검색 엔진(tantivy) 재검토(복잡도↑).

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 2
검색 토큰화 / 한국어 처리 (사실 본문에 한국어+영어 혼재. 한글은 공백 분할만으로 부분검색이 약함)?

A) **유니코드 단어 분할 + 소문자화, CJK 텍스트는 bigram(2-gram) 색인으로 부분검색 지원** — 형태소 분석기 없이 한/영 실용 검색. *(권장 — 의존성 최소, MVP 적정)*

B) 단순 공백/구두점 분할 + 소문자화만 — 영문 위주, 한국어 부분검색은 약함(가장 단순).

C) 한국어 형태소 분석기 도입 — 정확도↑, 무거운 의존성/복잡도↑.

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 3
LLM(클라우드) 미가용 시 답변 처리 (NFR-3 우아한 저하) — Deepen 답변은 사실 저장 + 후속 질문 LLM 생성을 동반합니다.

A) **사실 저장은 정상 수행, 후속 질문 생성만 건너뜀**(로그 남기고 다음 기회에 자연 재시도). 수집·조회·답변 저장은 오프라인에서도 항상 동작. *(권장 — NFR-3 부합, 데이터 유실 없음)*

B) 후속 질문 생성 실패 시 답변 전체 실패(사실도 저장 안 함).

C) 후속 생성 요청을 보류 큐에 쌓아 온라인 복귀 시 자동 재시도(추가 상태·복잡도).

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 4
이식성·백업 (NFR-6은 "사람이 읽는 md/json 이식·백업·수동 편집"; FD Q3=A는 암호화 저장이 정본) — 둘의 조화?

A) **명시적 "내보내기(export)" 커맨드** — 잠금 해제 상태에서 평문 md/json 번들 생성(백업·수동 편집용). 가져오기(import)는 후순위. 평상시 at-rest는 암호화 유지. *(권장 — 보안·이식성 모두 충족)*

B) 평문 위키 폴더 + 암호문을 상시 동기 유지(외부 편집 즉시 반영, 노출면↑).

C) 암호문 백업만 지원(이식성 포기, 사람이 못 읽음).

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 5
동시성·일관성 모델 (1인 로컬 앱; store + in-memory index 동시 갱신)?

A) **쓰기 직렬화** — 서비스가 store/index를 락으로 보호하고 커맨드 단위로 순차 처리. upsert는 "store 기록 성공 후 index 갱신"(index는 재구축 가능하므로 안전). *(권장 — 단순·경합 거의 없음)*

B) 세밀한 동시성(RwLock 다중 리더/단일 라이터) — 조회 병렬성↑, 복잡도↑.

C) 저널링/트랜잭션 도입 — 강한 원자성, 오버엔지니어링 위험.

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 6
PBT 프레임워크 확정 (PBT-09, NFR Requirements 필수 기록) — U3는 Rust 백엔드 중심.

A) **proptest 확정** — 이미 `src-tauri/Cargo.toml` dev-dependency로 추가됨(1.11.0). 매크로 strategy·shrinking·seed 재현 지원. 프론트 UI 테스트 도구(fast-check 등)는 U1 앱 셸/프론트 통합 시점에 결정. *(권장)*

B) quickcheck로 교체.

C) proptest + 지금 프론트 fast-check까지 함께 확정.

D) Other (please describe after [Answer]: tag below)

[Answer]: A
