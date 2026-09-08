# U4 Frontend Components (Interface & Persona)

> 범위: US-5.1 대시보드, US-5.2 미니홈피, US-5.3 지식 그래프, US-6.1 페르소나 챗
> **경계**: 앱 엔트리(`index.html`, `main.tsx`, `App.tsx`)·라우팅·온보딩/잠금 UI·번들러 설정(`vite.config.ts`)은 **U1 소유**(main에 머지 완료). U4는 마운트 방식에 무관한 순수 export 컴포넌트만 제공하고, 테스트 러너 설정(`vitest.config.ts`)만 따로 둔다. U3의 Queue UI(`src/features/queue/`)는 건드리지 않는다.

## 1. 컴포넌트 계층

```text
(U1 App shell / router)          <- U1 소유, U4는 미생성
  |
  +-- <DashboardView/>           src/features/dashboard/        [US-5.1]
  |     +-- <StatCard/>          (수집 현황 / 대기 Queue / 확정 사실 수)
  |     +-- <RecentFactList/>    (최근 확정 사실)
  |
  +-- <MiniHomeView/>            src/features/minihome/         [US-5.2]
  |     +-- <ProfileCard/>       (한 줄 소개 + 확정 사실 수)
  |     +-- <HighlightGrid/>     (3x3)
  |           +-- <HighlightCard/>
  |
  +-- <GraphView/>               src/features/graph/            [US-5.3]
  |     +-- <ScopeFilter/>
  |     +-- <GraphCanvas/>       (자체 SVG, 의존성 없음)
  |           +-- <GraphEdgeLine/> <GraphNodeCircle/>
  |     +-- <NodeDetailPanel/>   (선택 노드 + 이웃)
  |
  +-- <PersonaChatView/>         src/features/persona-chat/     [US-6.1]
        +-- <MessageList/>
        +-- <MessageBubble/>
        +-- <ChatComposer/>
        +-- <MaskingNotice/>     (외부 전송 시 마스킹 적용 고지 — NFR-2 투명성)

공용(U4 소유):  src/features/u4-shared/
        api.ts        KnowsMeApi 인터페이스 (포트)
        mock-api.ts   MockApi 구현 (결정적 픽스처)
        tauri-api.ts  TauriApi 구현 (U1 command 연결 지점)
        view-state.ts loading/ready/empty/error 상태 헬퍼
        selection.ts  하이라이트 선정 순수 함수 (BR-V2)
        graph-layout.ts 결정적 레이아웃 순수 함수 (BR-V3/V4/V5)
```

## 2. 데이터 접근 포트 (`KnowsMeApi`)

```ts
interface KnowsMeApi {
  getDashboard(): Promise<DashboardDto>;
  getMiniHome(limit?: number): Promise<MiniHomeDto>;
  getGraph(filter: GraphFilter): Promise<GraphDto>;
  personaChat(prompt: string): Promise<PersonaReply>;
  personaDraft(req: DraftRequest): Promise<Draft>;
}
```
- 타입은 U1 `src/shared/contracts.ts`에서 import (U4가 재정의하지 않음).
- `TauriApi`는 U1의 command 이름(`get_dashboard`, `get_minihome`, `get_graph`, `persona_chat`, `persona_draft`)으로 `invoke`한다. U1 셸이 준비되기 전에는 `MockApi`가 주입된다.

## 3. 컴포넌트 명세

### 3.1 `<DashboardView api limit? />` — US-5.1
| 항목 | 내용 |
|---|---|
| props | `api: KnowsMeApi` |
| state | `ViewState<DashboardDto>` = `{status:'loading'\|'ready'\|'empty'\|'error', data?, error?}` |
| 마운트 시 | `api.getDashboard()` 호출 |
| 상호작용 | "새로고침" 버튼 → 재조회 (BR-V1: 재집계 금지, pull 방식 — AC2) |
| 렌더 | StatCard×3(수집 현황, 대기 Queue 개수, 최근 확정 사실 수) + RecentFactList |
| empty | 세 수치가 모두 0이고 최근 사실이 없을 때 "아직 수집된 내용이 없습니다" |
| error | 메시지 + 재시도 버튼 (BR-E2: 다른 뷰에 영향 없음) |

### 3.2 `<MiniHomeView api limit=9 />` — US-5.2
| 항목 | 내용 |
|---|---|
| props | `api: KnowsMeApi`, `limit?: number` (기본 9) |
| state | `ViewState<MiniHomeDto>` |
| 로직 | `api.getMiniHome(limit)`; 백엔드가 없을 때는 `selectHighlights()` 순수 함수가 동일 규칙 적용 (BR-V2). Rust `rank_highlights`와 정렬 키가 **정확히 같아야** 한다 — `(연결도 desc, 제목 asc, id asc)` |
| 렌더 | ProfileCard(확정 사실 수) + 3x3 HighlightGrid, 카드마다 `Scope` 배지 |
| empty | "확정된 사실이 아직 없습니다 — Queue에서 질문에 답하면 채워집니다" |

### 3.3 `<GraphView api />` — US-5.3
| 항목 | 내용 |
|---|---|
| props | `api: KnowsMeApi` |
| state | `ViewState<GraphDto>`, `scope: Scope\|null`, `selected: FactId\|null` |
| 로직 | `api.getGraph({scope})` → `normalizeGraph()`(BR-V3) → `layoutGraph(g, w, h)`(BR-V4/V5) |
| 상호작용 | ScopeFilter 변경 → 재조회 / 노드 클릭 → `selected` 토글, 선택 노드+이웃 강조, 나머지 흐리게 / 배경 클릭 → 선택 해제 |
| 렌더 | `<svg viewBox="0 0 w h">` — 엣지 line, 노드 circle+label. 라이브러리 없음 |
| empty | "링크된 사실이 없습니다" |
| 접근성 | 노드에 `role="button"`, `aria-label={label}`, 키보드 Enter/Space 선택 |

### 3.4 `<PersonaChatView api />` — US-6.1
| 항목 | 내용 |
|---|---|
| props | `api: KnowsMeApi` |
| state | `messages: {role:'user'\|'persona', text, error?}[]`, `input: string`, `sending: boolean` |
| 폼 검증 | 공백만 있는 입력 → 전송 비활성. 최대 4000자 |
| 전송 | 낙관적으로 user 메시지 append → `api.personaChat(prompt)` → persona 메시지 append |
| 실패 | 해당 메시지에 오류 배지 + "다시 시도". 대화는 유지 (BR-E2) |
| 고지 | `<MaskingNotice/>`로 "외부 LLM 전송 시 식별자는 마스킹됩니다" 상시 표시 (US-6.1 AC2 / NFR-2 투명성) |
| empty | 대화 없음 상태에 예시 질문 3개 제시 |

## 4. 상호작용 흐름 (챗)
```text
사용자 입력 -> [검증: 비공백/<=4000] -> user 메시지 append -> sending=true
   -> api.personaChat()
        Ok  -> persona 메시지 append, sending=false
        Err -> 마지막 user 메시지에 error 표시, sending=false, 재시도 버튼
```

## 5. API 연동 지점 요약
| 컴포넌트 | 호출 | 대응 백엔드(U4 Rust) | 스토리 |
|---|---|---|---|
| DashboardView | `getDashboard()` | `get_dashboard` → `KnowledgeApi.dashboard()` | US-5.1 |
| MiniHomeView | `getMiniHome(limit)` | `get_minihome` → `KnowledgeApi.search` + 선정 | US-5.2 |
| GraphView | `getGraph(filter)` | `get_graph` → `KnowledgeApi.graph()` | US-5.3 |
| PersonaChatView | `personaChat(prompt)` | `persona_chat` → `PersonaService.chat()` | US-6.1 |
| (로컬 API 클라이언트) | `POST /draft` | `LocalApiServer` → `PersonaService.draft()` | US-6.2 |

## 6. Testable Properties (PBT-01)
| 대상 | 속성 | 카테고리 | 강제 |
|---|---|---|---|
| `selectHighlights()` | 출력 ⊆ 확정 입력, `len <= limit`, 입력 순열 불변 | Invariant | PBT-03 |
| `MockApi.personaChat()` | 확정 사실이 있으면 예시 질문 전부가 근거 있는 답을 받는다 (한국어 조사 때문에 항 단위 매칭만으로는 부족 → Rust와 동일한 recall 폴백) | 예제 | RTL |
| `normalizeGraph()` | 남은 엣지의 양 끝이 노드 집합에 존재, 중복 없음. 절단 랭킹은 **정리된 엣지 기준 연결도**를 쓴다(self-loop·중복이 순위를 부풀리지 않게) | Invariant | PBT-03 |
| `layoutGraph()` | 노드 수 보존, 좌표 ∈ 뷰박스, 두 번 호출 시 동일 | Invariant | PBT-03 |
| DTO JSON | `parse(stringify(x)) == x` | Round-trip | PBT-02 |

**No PBT properties identified**: 렌더 컴포넌트 자체(DOM 출력) — React Testing Library 예제 기반 테스트로 커버. 근거: 렌더 결과에 대한 범용 불변식보다 구체 시나리오 검증이 적합(PBT-10 상보성).
