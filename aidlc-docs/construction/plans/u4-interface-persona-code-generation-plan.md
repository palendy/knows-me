# U4 Code Generation Plan (Interface & Persona)

> **이 계획이 Code Generation의 단일 출처(single source of truth)다.**
> Owner: Dev D · Branch: `construction/u4-interface-persona`
> Workspace root: `/Users/junyung.ahn/Desktop/Work/18_avatar/knows-me` · Greenfield · 애플리케이션 코드는 절대 `aidlc-docs/`에 두지 않는다.

## 유닛 컨텍스트
- **구현 스토리**: US-5.1 대시보드, US-5.2 미니홈피, US-5.3 지식 그래프, US-6.1 페르소나 챗, US-6.2 로컬 API 초안
- **의존**: U1 `core::types`/`core::traits`/`mocks`(읽기 전용), U3 `KnowledgeApi`(개발 중 `InMemoryKnowledge` mock)
- **제공 인터페이스**: `PersonaApi` 구현체, `QueryService`, `LocalApiServer`(`POST /chat`, `POST /draft`, `GET /health`)
- **소유 엔티티**: 없음(영속 엔티티는 U3/U1 소유). U4는 파생 타입(`PersonaContext` 등)만 소유
- **경계**: `core/**`·`mocks.rs`·`ingestion/`·`processing/`·`knowledge/`·`interview/`·`src/features/queue/`·Tauri 셸·`vite.config.ts` **수정/생성 금지**

## 대상 경로
| 종류 | 경로 |
|---|---|
| Rust 비즈니스 로직 | `src-tauri/src/persona/{mod,context,selection,query,service,local_api,testgen}.rs` |
| Rust 공용 파일(추가만) | `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs` |
| 프론트엔드 공용(U4) | `src/features/u4-shared/{api,mock-api,tauri-api,view-state,selection,graph-layout,testgen}.ts` |
| 프론트엔드 뷰 | `src/features/{dashboard,minihome,graph,persona-chat}/*.tsx` |
| 테스트 툴체인 | `package.json`, `tsconfig.json`, `vitest.config.ts`, `src/test-setup.ts` |
| 문서 | `aidlc-docs/construction/u4-interface-persona/code/*.md` (마크다운만) |

## 실행 단계

### Step 1: Rust 모듈 스캐폴드 + 의존성
- [x] `Cargo.toml`에 axum/tokio/tower(dep), proptest/reqwest(dev-dep) **추가만**
- [x] `lib.rs`에 `pub mod persona;` 1줄 추가
- [x] `persona/mod.rs` 작성(파생 타입 `PersonaContext`/`ContextEntry`/`ContextSelection`/`PersonaPrompt` 포함)

### Step 2: Business Logic — 맥락 조합·선정 (US-5.2, US-6.1/6.2)
- [x] `persona/context.rs` — `select_context`, `render_prompt` (BR-P1/P5/P7, BR-P8)
- [x] `persona/selection.rs` — `select_highlights` (BR-V2)

### Step 3: Business Logic Unit Testing (예제 + PBT)
- [x] `context.rs` 예제 테스트: 미확정 제외, 상한 절단, 빈 맥락
- [x] `selection.rs` 예제 테스트: 정렬 순서, limit
- [x] `persona/testgen.rs` — 도메인 생성기 (PBT-07)
- [x] PBT: 확정만/상한/중복없음/순열불변, 하이라이트 부분집합·상한·순열불변 (PBT-03)

### Step 4: Service Layer — QueryService / PersonaService (US-5.1~5.3, US-6.1)
- [x] `persona/query.rs` — `QueryService` (LlmClient 미주입 = 오프라인 보장)
- [x] `persona/service.rs` — `PersonaService` (`PersonaApi` 구현, 마스킹→LLM→복원)

### Step 5: Service Layer Unit Testing
- [x] 빈 맥락 → LLM 미호출 + 결정적 메시지 (BR-P4)
- [x] 마스킹 순서 검증: LLM이 받은 인자에 원문 식별자 없음 (BR-P2)
- [x] LLM 실패 → `External` 전파 (BR-P6)
- [x] 초안 종류별 지시문 반영 (BR-P8)
- [x] PBT: 임의 사실/질문에도 LLM 인자에 원문 식별자 미포함 (PBT-03)

### Step 6: API Layer — LocalApiServer (US-6.2)
- [x] `persona/local_api.rs` — axum 라우터, 127.0.0.1 바인딩, Host 검증, 오류 매핑, graceful shutdown, 포트 탐색

### Step 7: API Layer Unit/Integration Testing
- [x] `POST /chat`, `POST /draft` 정상 200
- [x] 비-loopback Host → 403 (BR-A2)
- [x] 잘못된 JSON → 400, LLM 실패 → 502, 잠금 → 423 (BR-A6)
- [x] `stop()` 후 연결 거부, `stop()` 두 번 호출 안전
- [x] PBT: 요청/응답 DTO JSON 왕복 무손실 (PBT-02)

### Step 8: 프론트엔드 테스트 툴체인 + 공용 모듈
- [x] `package.json`, `tsconfig.json`, `vitest.config.ts`, `src/test-setup.ts` (번들러/엔트리 제외)
- [x] `u4-shared/api.ts`, `mock-api.ts`, `tauri-api.ts`, `view-state.ts`
- [x] `u4-shared/selection.ts`, `graph-layout.ts` (순수 함수)
- [x] `u4-shared/testgen.ts` (fast-check 도메인 생성기, PBT-07)

### Step 9: 프론트엔드 순수 로직 테스트
- [x] `graph-layout.property.test.ts` — 노드 보존·경계 내·결정성·dangling 제거 (PBT-03)
- [x] `selection.property.test.ts` — 부분집합·상한·순열불변 (PBT-03)
- [x] `contracts.property.test.ts` — DTO JSON 왕복 (PBT-02)

### Step 10: 프론트엔드 컴포넌트 (US-5.1~5.3, US-6.1)
- [x] `dashboard/DashboardView.tsx`
- [x] `minihome/MiniHomeView.tsx`
- [x] `graph/GraphView.tsx` (자체 SVG)
- [x] `persona-chat/PersonaChatView.tsx` (+ MaskingNotice)

### Step 11: 프론트엔드 컴포넌트 테스트 (예제 기반, PBT-10 상보성)
- [x] 각 뷰의 loading/ready/empty/error 4상태
- [x] 그래프 노드 클릭 → 이웃 강조, 키보드 선택
- [x] 챗 전송/실패/재시도, 마스킹 고지 표시

### Step 12: 검증
- [x] `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`
- [x] `npm test`, `npx tsc --noEmit`
- [x] 시크릿 하드코딩 검사 (심사 기준 ⑥)

### Step 13: 문서
- [x] `aidlc-docs/construction/u4-interface-persona/code/implementation-summary.md`
- [x] `aidlc-docs/construction/u4-interface-persona/code/local-api.md` (API 문서)
- [x] `aidlc-docs/construction/u4-interface-persona/code/integration-handoff.md` (U1/U3 통합 지침)
- [x] `aidlc-state.md` 갱신, `audit.md` 추가

### Step 14: 커밋 & 푸시 (`construction/u4-interface-persona`)
- [x] 완료

## 스토리 추적성
| 스토리 | 산출물 | 단계 |
|---|---|---|
| US-5.1 | `query.rs::dashboard` + `DashboardView.tsx` | 4, 10 |
| US-5.2 | `selection.rs` + `query.rs::minihome` + `MiniHomeView.tsx` | 2, 4, 10 |
| US-5.3 | `query.rs::graph` + `graph-layout.ts` + `GraphView.tsx` | 4, 8, 10 |
| US-6.1 | `context.rs` + `service.rs::chat` + `PersonaChatView.tsx` | 2, 4, 10 |
| US-6.2 | `service.rs::draft` + `local_api.rs` | 4, 6 |

**총 14단계.** Deployment artifacts는 로컬 데스크탑 앱이라 Infrastructure Design과 함께 SKIP(패키징은 U1 Tauri 셸 소유).
