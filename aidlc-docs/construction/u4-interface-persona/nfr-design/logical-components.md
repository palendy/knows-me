# U4 Logical Components (Interface & Persona)

## 컴포넌트 맵
```text
+----------------------------- U4 -----------------------------+
|                                                              |
|  C1 QueryService        C2 PersonaService     C3 LocalApi     |
|  (조회 3종)              (챗/초안)              Server        |
|      |                       |                     |         |
|      |                       +---------------------+         |
|      |                       |  (위임)                       |
|      v                       v                               |
|  C4 selection.rs        C5 context.rs                        |
|  (하이라이트 선정)        (맥락 조합)                          |
|                                                              |
|  C6 graph_layout.ts / normalize  (프론트엔드 순수 로직)        |
|  C7 React Views (Dashboard/MiniHome/Graph/PersonaChat)        |
|  C8 KnowsMeApi 어댑터 (Mock / Tauri)                          |
+--------------------------------------------------------------+
        |  consumes (읽기 전용)            |  consumes
        v                                  v
   U3 KnowledgeApi (mock)          U1 Masker / LlmClient / types
```

## C1. `QueryService` — `src-tauri/src/persona/query.rs`
| 항목 | 내용 |
|---|---|
| 책임 | 대시보드/미니홈피/그래프 조회 (US-5.1~5.3) |
| 의존 | `KnowledgeApi` **만** (LlmClient 미주입 → 오프라인 보장) |
| 인터페이스 | `dashboard() -> Result<DashboardDto>`, `minihome(limit) -> Result<MiniHomeDto>` (`search("")` + `graph()` 2회 고정), `graph(GraphFilter) -> Result<GraphDto>` |
| NFR | U4-NFR-A1, P1~P3, BR-V1/V8 |

## C2. `PersonaService` — `src-tauri/src/persona/service.rs`
| 항목 | 내용 |
|---|---|
| 책임 | 맥락 조합 → 마스킹 → LLM → 복원 (US-6.1, US-6.2) |
| 의존 | `KnowledgeApi`, `Masker`, `LlmClient` |
| 구현 | U1 `PersonaApi` trait (`chat`, `draft`) |
| 추가 인터페이스 | `build_context(prompt, ContextSelection) -> Result<PersonaContext>` |
| NFR | U4-NFR-SEC3, P5, R2, BR-P1~P8 |

## C3. `LocalApiServer` — `src-tauri/src/persona/local_api.rs`
| 항목 | 내용 |
|---|---|
| 책임 | `POST /chat`, `POST /draft`, `GET /health`를 loopback에 노출, PersonaService로 위임 |
| 의존 | `PersonaApi`(trait 객체), axum, tokio |
| 인터페이스 | `start(port) -> Result<LocalApiHandle>`, `LocalApiHandle::stop()`(멱등), `::port()`. **핸들을 drop해도 서버가 종료된다**(shutdown sender가 함께 drop) — 리스닝 소켓이 주인 없이 남지 않게 하는 의도된 동작 |
| NFR | U4-NFR-SEC1/SEC2/SEC6, A4, R3, BR-A1~A7 |

## C4. `selection` — `src-tauri/src/persona/selection.rs`
| 항목 | 내용 |
|---|---|
| 책임 | 확정 사실 → 미니홈피 하이라이트 선정 (순수 함수) |
| 인터페이스 | `rank_highlights(&[FactSummary], &HashMap<FactId, usize>, limit) -> Vec<FactSummary>` (주 경로, 요약+연결도만 사용) / `select_highlights(&[Fact], limit)` (전체 사실을 이미 쥔 호출자용 래퍼) |
| 속성 | 부분집합성 / 길이 상한 / 순열 불변 (PBT-03) |

## C5. `context` — `src-tauri/src/persona/context.rs`
| 항목 | 내용 |
|---|---|
| 책임 | 검색 결과 → `PersonaContext` 조합, 프롬프트 렌더 (순수 함수) |
| 인터페이스 | `select_context(&[Fact], &str, &ContextSelection, total_confirmed) -> PersonaContext`, `title_relevance(&str, &str) -> u32` (요약만으로 pre-rank), `render_prompt(&PersonaContext, &str, Option<DraftKind>) -> PersonaPrompt` |
| 속성 | 확정만 / 상한 / 중복 없음 / 순열 불변 (PBT-03) |

## C6. 프론트엔드 순수 로직 — `src/features/u4-shared/`
| 파일 | 책임 | 속성 |
|---|---|---|
| `graph-layout.ts` | `normalizeGraph`, `layoutGraph` | 노드 보존·경계 내·결정성·dangling 제거 (PBT-03) |
| `selection.ts` | `selectHighlights` (C4와 동일 규칙의 프론트엔드 사본) | 부분집합·상한·순열 불변 (PBT-03) |
| `view-state.ts` | `ViewState` 4상태 헬퍼 | — |

## C7. React Views — `src/features/{dashboard,minihome,graph,persona-chat}/`
`frontend-components.md` §3 참조. 모두 `KnowsMeApi`에만 의존하며 상태를 소유하지 않는다(BR-V7).

## C8. `KnowsMeApi` 어댑터 — `src/features/u4-shared/{api,mock-api,tauri-api}.ts`
| 구현 | 용도 |
|---|---|
| `MockApi` | 개발·테스트. 결정적 픽스처 |
| `TauriApi` | U1 command 연결 (`invoke`). U1 셸 준비 전까지 미사용 |

## 컴포넌트 ↔ 스토리 ↔ NFR 추적
| 컴포넌트 | 스토리 | 주요 NFR |
|---|---|---|
| C1, C7-Dashboard | US-5.1 | P1, A1, U1 |
| C4, C7-MiniHome | US-5.2 | P2, A1, R2 |
| C6, C7-Graph | US-5.3 | P3, P4, A1, R2, U3 |
| C2, C5, C7-PersonaChat | US-6.1 | SEC3, SEC7, P5, P6, A2 |
| C3, C2 | US-6.2 | SEC1, SEC2, SEC6, A4, R3, S3 |

## 유닛 경계 준수
- U4는 `KnowledgeApi`를 **읽기로만** 사용한다. `upsert`/`enqueue` 등 쓰기 경로는 호출하지 않는다(U3 소유).
- U4는 `Masker`/`LlmClient`를 **소비만** 한다. 구현은 U1 소유.
- U4는 `core/types.rs`에 타입을 추가하지 않는다. 필요한 파생 타입은 `persona/` 안에 둔다.
