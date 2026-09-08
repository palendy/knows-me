# U4 NFR Design Patterns (Interface & Persona)

## 1. 마스킹 강제 — "데이터를 한쪽으로 몰고, 타입으로 막고, 속성으로 검증"
| 레이어 | 수단 |
|---|---|
| 구조 | 소유자 데이터를 전부 `PersonaPrompt.user_document`에 모으고 `system`은 정적 템플릿으로 둔다. 마스킹 대상이 한 덩어리라 **`mask()` 호출이 요청당 정확히 1회** — 두 `UnmaskMap` 사이의 placeholder 충돌이 발생할 여지가 없다 |
| 컴파일 타임 | `LlmClient::chat(system: &str, input: &MaskedText)` — `MaskedText`는 `Masker::mask()`로만 생성 가능한 값이므로, 마스킹을 건너뛴 원문은 애초에 전달할 수 없다 |
| 런타임 | PBT-03: 임의 생성한 사실·질문에 대해 LLM mock이 받은 인자에 원문 식별자(이메일/전화/URL)가 없는지 검증 |
| 사용자 가시성 | `<MaskingNotice/>` 상시 표시 (NFR-2 투명성, U4-NFR-SEC7) |

> 충족: U4-NFR-SEC3, BR-P2, US-6.1 AC2

## 2. 오프라인 저하 — "의존성 분리로 구조적 보장"
```text
QueryService   : KnowledgeApi 만 주입           <- LLM 의존 불가(컴파일 타임)
PersonaService : KnowledgeApi + Masker + LlmClient
```
조회 3종은 `LlmClient`를 아예 알지 못하므로, 네트워크가 끊겨도 동작이 보장된다. "잊고 호출하는" 실수가 구조적으로 불가능하다.

> 충족: U4-NFR-A1, BR-V8, NFR-3

## 3. 결정성 — "순수 함수 + 전순서"
| 함수 | 결정성 확보 방법 |
|---|---|
| `select_context` | 정렬 키가 전순서: `relevance desc → confirmed_at desc → FactId asc`. 동점이 남지 않음 |
| `rank_highlights` | `degree desc → title asc → FactId asc` (요약에는 타임스탬프가 없다 — BR-V2 개정 근거 참조) |
| `layout_graph` | 난수·시각 미사용. 연결도 정렬 후 동심 링에 각도 균등 배치 |
- 안정 정렬만으로는 입력 순서 의존이 남으므로, **마지막 타이브레이커에 항상 고유 ID**를 둔다.

> 충족: U4-NFR-R2, BR-P7, BR-V4 · 검증: PBT-03(순열 불변)

## 4. 성능 — "상한을 설계에 박는다"
| 지점 | 패턴 | 상한 |
|---|---|---|
| 페르소나 맥락 | Bounded selection | 12개 사실 |
| 맥락 후보 fetch | 제목 연관도로 pre-rank 후 절단 (id 순 절단이면 연관 사실 유실) | 64개 `get()` |
| 미니홈피 | `search("")` + `graph()` 2회 고정, 사실당 `get()` 없음 | 백엔드 왕복 2회 |
| 그래프 | Truncate-then-normalize (연결도 상위 N 절단 → dangling 제거) | 500 노드 |
| 챗 입력 | Input cap | 4000자 |
| LLM 호출 | Timeout | 30초 |
- 절단이 일어나면 `truncated: true`로 UI에 알려 "조용한 데이터 손실"을 만들지 않는다.

> 충족: U4-NFR-P3~P6, S2

## 5. 오류 국소화 — "뷰 단위 격리"
- 각 뷰가 독립적으로 `ViewState`를 가진다. 챗 실패가 그래프 상태를 건드리지 않는다.
- Rust 측은 모든 실패를 `AppError`로 정규화. 로컬 API 핸들러는 panic-safe(내부 panic이 프로세스를 죽이지 않도록 결과를 `Result`로 회수).

**오류 매핑 (단일 출처)**
| `AppError` | 로컬 API | UI 표시 |
|---|---|---|
| `Locked` | 423 | "잠금 해제가 필요합니다" |
| `InvalidInput` | 400 | 입력 오류 메시지 |
| `NotFound` | 404 | "대상을 찾을 수 없습니다" |
| `External` | 502 | "외부 서비스에 연결할 수 없습니다 (오프라인일 수 있습니다)" |
| `Crypto`/`Io`/`Serde` | 500 | "일시적인 오류가 발생했습니다" |

> 충족: U4-NFR-A2, R1, SEC6, BR-A6, BR-E1~E4

## 6. 로컬 전용 노출 — "이중 방어"
1. **바인딩**: `TcpListener`를 `127.0.0.1:<port>`에만 연다 (외부 인터페이스에 소켓 자체가 없음)
2. **Host 검증**: `Host` 헤더가 loopback이 아니면 403 (DNS rebinding 방어)

> 충족: U4-NFR-SEC1/SEC2, BR-A1/A2, US-6.2 AC2, D7

## 7. 포트/어댑터 — "U1 통합 비용을 1개 파일로"
```text
React 뷰 -> KnowsMeApi(포트) -> { MockApi | TauriApi }
Rust 서비스 -> KnowledgeApi(U1 trait) -> { InMemoryKnowledge(mock) | U3 실제 구현 }
```
양쪽 모두 인터페이스에만 의존하므로, U3/U1이 실물을 내놓을 때 **주입 지점만 교체**하면 된다.

> 충족: U4-NFR-M5

## 8. PBT 설계 (PBT-07/08/09/10)
| 항목 | 설계 |
|---|---|
| 생성기 (PBT-07) | `src-tauri/src/persona/testgen.rs`(Rust)와 `src/features/u4-shared/testgen.ts`(TS)에 도메인 생성기 집중: `arb_fact`(유효 UUID·비어있지 않은 제목·confirmed 비율 혼합), `arb_graph`(노드 집합에서만 엣지 생성 + 의도적 dangling 주입), `arb_draft_request`. 원시 타입 단독 생성기 사용 금지. **id는 `arb_fact_id()`(시드 제어)로 생성** — `Uuid::new_v4()`는 proptest 시드가 제어하지 못하는 전역 RNG라 재현이 깨진다 |
| shrinking (PBT-08) | 기본값 사용, 비활성화 금지. proptest 실패 케이스는 `src-tauri/proptest-regressions/`에 기록·커밋 |
| 재현성 (PBT-08) | proptest: `PROPTEST_SEED` 환경변수 / fast-check: 실패 시 seed 출력 → `fc.assert(prop, { seed })`로 재현. 두 러너 모두 CI 기본 명령(`cargo test`, `npm test`)에 포함 |
| 상보성 (PBT-10) | 파일/이름 분리 — Rust `*_properties` 모듈, TS `*.property.test.ts` vs 예제 `*.test.tsx`. 핵심 시나리오(빈 맥락→LLM 미호출, 403 거부, 502 매핑)는 예제 테스트로 고정 |

> 충족: U4-NFR-M1~M4, NFR-8
