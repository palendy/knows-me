# U4 Implementation Summary (Interface & Persona)

> Branch: `construction/u4-interface-persona` · Owner: Dev D
> 검증 상태(main rebase + 코드리뷰 반영 후): `cargo test` **151 pass**(전 유닛), `cargo fmt --check` **clean**, `cargo clippy --all-targets -- -D warnings` **clean**, `npm test` **41 pass**, `tsc --noEmit` **clean** (2026-09-08 로컬 실행)

## 생성한 파일

### Rust (`src-tauri/src/persona/`)
| 파일 | 책임 | 스토리 |
|---|---|---|
| `mod.rs` | U4 파생 타입(`PersonaContext`, `ContextEntry`, `ContextSelection`, `PersonaPrompt`), 상수 | — |
| `context.rs` | 맥락 조합·프롬프트 렌더 (순수) | US-6.1, US-6.2 |
| `selection.rs` | 미니홈피 하이라이트 선정 (순수) | US-5.2 |
| `query.rs` | `QueryService` — 대시보드/미니홈피/그래프 | US-5.1~5.3 |
| `service.rs` | `PersonaService` — `PersonaApi` 구현 | US-6.1, US-6.2 |
| `local_api.rs` | `LocalApiServer` — loopback 전용 REST | US-6.2 |
| `testgen.rs` | proptest 도메인 생성기 + 픽스처 (PBT-07) | — |
| `properties.rs` | 속성 기반 테스트 (PBT-02/03) | — |

### 공용 파일 (추가만, 기존 내용 미변경)
- `src-tauri/Cargo.toml` — axum/tokio 추가, proptest dev-dep 추가
- `src-tauri/src/lib.rs` — `pub mod persona;` 1줄 + 모듈 문서 1항목

### 프론트엔드
| 파일 | 책임 | 스토리 |
|---|---|---|
| `src/features/u4-shared/api.ts` | `KnowsMeApi` 포트 | — |
| `src/features/u4-shared/mock-api.ts` | `MockApi`/`FailingApi`/`withOverrides` | — |
| `src/features/u4-shared/tauri-api.ts` | U1 command 어댑터(통합 지점) | — |
| `src/features/u4-shared/view-state.ts` | 4상태 헬퍼 | — |
| `src/features/u4-shared/StateShell.tsx` | loading/empty/error 셸 | — |
| `src/features/u4-shared/selection.ts` | 하이라이트 선정 (순수) | US-5.2 |
| `src/features/u4-shared/graph-layout.ts` | 정규화 + 결정적 링 레이아웃 (순수) | US-5.3 |
| `src/features/u4-shared/styles.ts` | 인라인 스타일 토큰 | — |
| `src/features/u4-shared/testgen.ts` | fast-check 도메인 생성기 (PBT-07) | — |
| `src/features/dashboard/DashboardView.tsx` | 대시보드 | US-5.1 |
| `src/features/minihome/MiniHomeView.tsx` | 미니홈피 3x3 | US-5.2 |
| `src/features/graph/GraphView.tsx` | 자체 SVG 그래프 | US-5.3 |
| `src/features/persona-chat/PersonaChatView.tsx` | 페르소나 챗 + 마스킹 고지 | US-6.1 |

### 테스트 툴체인 (미할당 영역, U4가 최소한만 추가)
- `package.json` (test/typecheck 스크립트만), `tsconfig.json`, `vitest.config.ts`, `src/test-setup.ts`
- **의도적으로 만들지 않음**: `vite.config.ts`, `index.html`, `src/main.tsx`, `src/App.tsx`, `tauri.conf.json` — U1 앱 셸 소유

## 스토리별 충족 근거
| 스토리 | AC | 충족 방식 | 검증 테스트 |
|---|---|---|---|
| US-5.1 | AC1 | `get_dashboard` 결과의 3수치 + 최근 사실 렌더 | `DashboardView.test.tsx` "shows collection status…" |
| US-5.1 | AC2 | 진입/새로고침마다 최신 스냅샷 pull | `…"reflects the latest numbers when refreshed"` |
| US-5.2 | AC1 | 확정 사실을 연결도 기준 3x3 카드로 시각화 | `MiniHomeView.test.tsx` ×3, `selection.property.test.ts` ×4 |
| US-5.3 | AC1 | 노드=circle, 엣지=line, 클릭/키보드 탐색 | `GraphView.test.tsx` ×7, `graph-layout.property.test.ts` ×9 |
| US-6.1 | AC1 | 확정 맥락만 근거, 맥락 없으면 LLM 미호출 | `service.rs` ×3, `PersonaChatView.test.tsx` ×2 |
| US-6.1 | AC2 | LLM 호출 전 단일 `mask()` 통과 + 상시 고지 | `identifiers_are_masked_before_the_call_and_restored_after`, `identifiers_never_reach_the_gateway` (PBT) |
| US-6.2 | AC1 | `POST /chat`, `POST /draft`가 페르소나 초안 반환 | `local_api.rs` ×3 |
| US-6.2 | AC2 | `127.0.0.1` 전용 바인딩 + 비-loopback Host 403 | `non_loopback_host_is_rejected`, `loopback_host_detection` |

## 설계상 눈여겨볼 결정
1. **오프라인 보장을 구조로** — `QueryService`는 `LlmClient`를 주입받지 않는다. 조회 3종이 네트워크에 의존하는 것이 컴파일 타임에 불가능하다(NFR-3).
2. **단일 마스킹 패스** — `system` 프롬프트를 소유자 데이터가 전혀 없는 정적 템플릿으로 만들고, 가변 데이터(맥락+질문)를 하나의 문자열로 합쳐 `mask()`를 **한 번만** 호출한다. 두 번 마스킹하면 두 개의 `UnmaskMap` 사이에서 placeholder 네임스페이스가 충돌할 수 있는데, 그 문제 자체가 발생하지 않는다.
3. **recall 폴백** — 타깃 검색 결과가 `max_facts`보다 적으면 확정 집합 전체로 넓힌 뒤 U4의 연관도 랭킹으로 좁힌다. U3 mock의 `search`는 질의 전체를 부분문자열로 매칭하므로 자연어 질문("배포는 어떻게 해?")에 아무것도 반환하지 않는데, 실제 U3 인덱스가 들어와도 동작이 유지된다. 읽기 증폭은 `fetch_cap`(64)으로 상한.
4. **결정적 레이아웃** — force 시뮬레이션 대신 연결도 기준 동심 링. 같은 그래프가 항상 같은 자리에 놓여 소유자의 공간 기억이 유지되고, 속성 테스트가 가능해진다.
5. **이중 방어 노출 차단** — 소켓을 `127.0.0.1`에만 열고(외부 인터페이스에 소켓 자체가 없음), 추가로 `Host` 헤더를 검사해 DNS rebinding을 막는다.

## 동료 코드리뷰(xhigh, coolfebreeze) 반영 — 9건

| # | 심각도 | 지적 | 조치 |
|---|---|---|---|
| 1 | 🔴 | `build_context`가 `fetch_cap` 절단을 **uuid 순**으로 해서, 확정 사실이 64개를 넘으면 연관도 높은 사실이 랭킹도 되기 전에 버려짐 | 제목 연관도(`title_relevance`)로 pre-rank한 뒤 절단. 요약만으로 계산 가능한 신호를 써서 fetch 예산을 연관 사실에 씀 |
| 2 | 🔴 | 넓히기 판단(`len < max_facts`)이 `retain(confirmed)` **이전** 개수를 써서, 미확정 매칭이 많으면 폴백이 스킵되고 전부 탈락 → 확정 사실이 있는데도 "맥락 없음" 오응답 | confirmed 필터를 개수 판단보다 **앞으로** 이동 |
| 3 | 🟡 | BR-P2/C5/SEC3/E4는 "system도 마스킹"인데 구현은 system을 정적 템플릿으로 두고 마스킹하지 않음 — 보안 실질은 보존되나 문서와 불일치 | 구현이 더 낫다고 판단해 **문서를 개정**. BR-P2 개정 + BR-P2a 신설, E4를 `user`→`user_document`로 재정의, C5 개정 + C5a(요청당 mask 1회) 신설, U4-NFR-SEC3·P2 파이프라인·NFR 설계 패턴 1 모두 개정. 개정 근거(placeholder 네임스페이스 충돌 회피)를 각 문서에 명시 |
| 4 | 🟡 | Rust 생성기가 `FactId::new()`(=`Uuid::new_v4()`, proptest 시드 밖 전역 RNG)를 써서 PBT-08 재현성 미충족 | `arb_fact_id()`(=`any::<u128>().prop_map(Uuid::from_u128)`) 도입. `arb_fact`/`arb_graph`/마스킹 속성이 전부 시드 제어 id를 사용. `fact_with_id` 픽스처 추가 |
| 5 | 🟡 | `total_confirmed`가 E1 정의(모집단)와 달리 절단된 후보 슬라이스에서 세어 과소 보고 | `select_context`가 `total_confirmed`를 **인자로 받도록** 변경. `build_context`가 `search("")`로 확정 모집단을 측정해 넘김. 속성 테스트도 "호출자가 준 모집단을 그대로 보고"로 교정 |
| 6 | 🟡 | `LocalApiHandle` 주석이 "drop해도 안 멈춘다"고 했으나 실제로는 drop 시 종료됨 — 수명을 맡는 U1이 오해하면 서버가 즉시 죽음 | 주석을 사실대로 고치고 **의도된 동작**임을 명시(주인 없는 리스닝 소켓 방지). `dropping_the_handle_also_shuts_the_server_down` 테스트로 고정. integration-handoff에 "핸들을 AppState에 보관하라" 경고 추가 |
| 7 | 🟡 | `minihome`이 전체 사실을 사실당 `get()`으로 조회(N+1). 1만 사실 시 1만 회 왕복 — U4-NFR-P2와 충돌 | 연결도를 `graph()` **1회**로 집계. `rank_highlights(&[FactSummary], &degree, limit)` 신설로 **왕복 2회 고정**. 대가로 recency 타이브레이커를 잃어(요약에 타임스탬프 없음) BR-V2를 개정하고, `FactSummary`에 `confirmed_at` 추가를 handoff에 요청 |
| 8 | 🟢 | `MockApi.personaChat`이 질의 전체를 부분문자열 매칭해 예시 버튼이 항상 "맥락 없음" — 테스트가 없어 CI 통과 | 항 단위 스코어링 + Rust와 동일한 recall 폴백 적용. `EXAMPLE_QUESTIONS`를 export해 **예시 전부가 근거 있는 답을 받는지** 검증하는 회귀 테스트 추가. 그 과정에서 기존 "맥락 없음" 테스트가 실제 서비스 동작과 어긋난 것도 발견해 교정(BR-P4는 *확정 사실 0개*일 때만 발동) |
| 9 | 🟢 | `normalizeGraph` 절단 랭킹이 self-loop·중복·dangling 정리 **이전** 연결도를 사용 | 3-pass로 재구성: 엣지 정리 → 정리된 엣지로 연결도 산출·절단 → 생존 노드로 엣지 재확인 |

## 최초 검증 중 잡은 결함 3건
| 증상 | 원인 | 조치 |
|---|---|---|
| 페르소나가 시드된 사실을 못 찾음 | U3 mock의 `search`가 질의 전체를 부분문자열 매칭 | recall 폴백 도입(위 3번) |
| `stop()` 두 번 호출 시 panic | 완료된 `JoinHandle`을 재차 poll | `task`를 `Option`으로 바꿔 idempotent화 |
| `selectHighlights` 순열 불변 속성이 간헐 실패 (PBT-08: flaky 억제 금지) | 생성기가 같은 `FactId`를 가진 서로 다른 fact를 생성 → dedupe 결과가 입력 순서에 좌우 | 생성기를 `uniqueArray`로 교정(도메인 제약), dedupe를 정렬 **이후**로 이동 |

## PBT 준수 요약 (Partial 모드: PBT-02/03/07/08/09 blocking)
| 규칙 | 상태 | 근거 |
|---|---|---|
| PBT-01 (설계 시 속성 식별) | ✅ *advisory* | 4개 functional-design 산출물 모두에 "Testable Properties" 절 |
| **PBT-02 (왕복)** | ✅ | Rust 5개 + TS 4개 왕복 속성 (`ChatRequestBody`/`DraftRequestBody`/`TextResponseBody`/`ErrorBody`/`HealthBody`/`Fact`, `GraphDto`/`MiniHomeDto`/`DashboardDto`/`DraftRequest`) |
| **PBT-03 (불변식)** | ✅ | Rust 8개 + TS 9개 (확정만·상한·중복없음·순열불변·부분집합·노드보존·경계내·결정성·dangling제거·마스킹) |
| PBT-04 (멱등성) | ✅ *advisory* | `stop()` 멱등성은 예제 테스트로 커버. U4에 다른 멱등 연산 없음 |
| PBT-05 (Oracle) | N/A *advisory* | 참조 구현·최적화 대체 대상 없음 |
| PBT-06 (상태 기반) | N/A *advisory* | U4는 영속 상태를 소유하지 않음(U3/U1 소유) |
| **PBT-07 (생성기 품질)** | ✅ | `persona/testgen.rs`, `u4-shared/testgen.ts`에 도메인 생성기 집중. 원시 타입 단독 생성기 없음. `arbFacts`는 id 유일성 보장 |
| **PBT-08 (축소·재현)** | ✅ | shrinking 기본값 유지(비활성화 없음). proptest 실패 시 `proptest-regressions/`에 기록·커밋, `PROPTEST_SEED`로 재현. **생성기가 전역 RNG를 쓰지 않도록 `arb_fact_id()`로 교정**(리뷰 4번) — 이전에는 id가 시드 제어 밖이라 재현이 성립하지 않았다. fast-check는 실패 시 seed 출력. 모두 `cargo test`/`npm test` 기본 명령에 포함. flaky 1건은 억제하지 않고 원인 수정 |
| **PBT-09 (프레임워크)** | ✅ | Rust=proptest 1(dev-dep), TS=fast-check 3(devDep). tech-stack-decisions.md에 기록 |
| PBT-10 (상보성) | ✅ *advisory* | 속성 테스트는 `properties.rs` / `*.property.test.ts`, 예제 테스트는 각 모듈 `tests` / `*.test.tsx`로 분리 |

## 확장 컴플라이언스
| 확장 | Enabled | 결과 |
|---|---|---|
| Security Baseline | No | 미적용. 단, 보안 요구는 requirements.md(NFR-2, D7)에서 직접 유래해 U4-NFR-SEC1~7로 자체 반영 |
| Resiliency Baseline | No | 미적용. NFR-3 기반 저하 정책만 자체 반영 |
| Property-Based Testing | Yes (Partial) | blocking 규칙 5개 전부 준수 — **blocking finding 없음** |
