# U4 통합 핸드오프 (U1 / U3 대상)

> U4는 다른 유닛의 파일을 수정하지 않았다. 통합 시 U1/U3 쪽에서 해야 할 일만 아래에 정리한다.

## U1(Dev A)이 해야 할 일

### 1. Tauri command 5개 등록
`src/features/u4-shared/tauri-api.ts`의 `COMMANDS`가 기대하는 이름:

| command | 인자 | 반환 | 위임 대상 |
|---|---|---|---|
| `get_dashboard` | — | `DashboardDto` | `QueryService::dashboard()` |
| `get_minihome` | `limit: Option<usize>` | `MiniHomeDto` | `QueryService::minihome(limit)` |
| `get_graph` | `filter: GraphFilter` | `GraphDto` | `QueryService::graph(filter)` |
| `persona_chat` | `prompt: String` | `PersonaReply` | `PersonaService::chat(prompt)` |
| `persona_draft` | `req: DraftRequest` | `Draft` | `PersonaService::draft(req)` |

```rust
use knows_me_core::persona::{PersonaService, QueryService};

let query = QueryService::new(knowledge.clone());
let persona = Arc::new(PersonaService::new(knowledge, masker, llm));
```
잠금 상태에서 조회/페르소나 command를 거부하는 게이트는 U1 `CommandRouter` 책임이다(BR-E1). U4는 `AppError::Locked`를 그대로 전파하고 UI에 안내만 한다.

### 2. 앱 셸에서 U4 뷰 마운트
U4는 `index.html` / `src/main.tsx` / `App.tsx` / 라우팅을 만들지 않았다(경계 준수). 마운트 예시:
```tsx
import { TauriApi } from "./features/u4-shared/tauri-api";
import { invoke } from "@tauri-apps/api/core";
import { DashboardView } from "./features/dashboard/DashboardView";
import { MiniHomeView } from "./features/minihome/MiniHomeView";
import { GraphView } from "./features/graph/GraphView";
import { PersonaChatView } from "./features/persona-chat/PersonaChatView";

const api = new TauriApi(invoke);
// <DashboardView api={api} /> 등으로 라우팅에 배치
```
개발 중에는 `new MockApi()`를 주입하면 백엔드 없이 4개 화면이 그대로 동작한다.

### 3. 프론트엔드 번들러 설정
U4는 **테스트 툴체인만** 추가했다(`package.json`의 test/typecheck 스크립트, `tsconfig.json`, `vitest.config.ts`). `vite.config.ts`와 `dev`/`build` 스크립트, `@tauri-apps/*` 의존성 추가는 U1 몫이다. `tsconfig.json`의 `include`는 `src`이므로 엔트리 추가만으로 커버된다.

### 4. 로컬 API 수명 관리
`LocalApiServer::start(persona, DEFAULT_PORT)`를 앱 시작 시 호출하고, 종료 시 `handle.stop().await`를 부른다. 실제 바인딩 포트(`handle.port()`)를 UI에 표시하면 사용자가 클라이언트를 붙일 수 있다.

**핸들 수명 주의**: `LocalApiHandle`을 drop하면 서버도 함께 종료된다(shutdown sender가 같이 drop되어 graceful shutdown이 발동). 의도된 동작이지만, 핸들을 임시 변수에 받아 그대로 버리면 서버가 즉시 죽는다는 뜻이므로 **`AppState` 같은 곳에 보관**해야 한다. 인플라이트 요청을 기다리는 정돈된 종료가 필요하면 `stop().await`를 쓰면 되고, 두 번 불러도 안전하다. (테스트 `dropping_the_handle_also_shuts_the_server_down`이 이 동작을 고정한다.)

### 5. Masker 관련 확인 사항 (조율 필요)
U4는 `mask()`를 **요청당 한 번만** 호출한다(`system`은 소유자 데이터가 없는 정적 템플릿). 실제 `Masker` 구현이 호출마다 독립적인 placeholder 네임스페이스를 쓰더라도 U4 경로에서는 충돌이 없다. 다만 다른 유닛이 한 요청 안에서 `mask()`를 여러 번 부른다면 placeholder 충돌 가능성이 있으니, U1이 placeholder를 내용 기반(해시)으로 만들거나 배치 마스킹 API를 제공하는 편이 안전하다.

## U3(Dev C)이 알아둘 것

### `KnowledgeApi.search`의 질의 의미론
U4는 `search(prompt, filter)`를 자연어 질문으로 호출한다. 현재 U1 mock은 **질의 문자열 전체**를 제목/본문 부분문자열로 매칭하므로 "배포는 어떻게 해?" 같은 입력에 아무것도 반환하지 않는다. U4는 결과가 `max_facts`보다 적으면 확정 집합 전체로 넓혀 자체 랭킹하는 폴백을 두어 이를 흡수하고 있다.

실제 구현에서 **토큰 단위 매칭 + 연관도 점수**를 제공하면 U4의 폴백이 거의 발동하지 않아 대용량에서 유리하다. 폴백은 `fetch_cap`(64)으로 상한이 걸려 있어 폴백이 남아 있어도 성능이 무너지지는 않는다.

### U4가 쓰는 메서드 (읽기 전용)
`search`, `get`, `graph`, `dashboard`. **`upsert` / `links` / `history`는 호출하지 않는다.** 쓰기 경로는 전적으로 U3 소유다.

호출 패턴:
- `minihome`: `search("")` 1회 + `graph()` 1회. 사실 수와 무관하게 **왕복 2회 고정**(사실마다 `get()`을 부르지 않는다)
- `build_context`: `search(prompt)` 1회 + `search("")` 1회 + `get()` 최대 64회(연관도 상위만)

### 요청: `FactSummary`에 `confirmed_at` 추가 검토 (U1/U3 공동)
미니홈피 하이라이트 정렬은 원래 `(연결도 desc, 확정일 desc, 제목 asc)`였으나, `FactSummary`에 타임스탬프가 없어 확정일을 쓰려면 사실마다 `get()`을 호출해야 했다(N+1 → U4-NFR-P2 위반). 지금은 `(연결도 desc, 제목 asc, id asc)`로 낮춰 잡았다. `FactSummary`에 `confirmed_at: Option<DateTime<Utc>>`가 추가되면 U4가 recency 타이브레이커를 바로 복원한다. `core::types`는 U1 소유이므로 U4가 직접 바꾸지 않았다.

`dashboard()`의 `pending_queue`는 U3의 인터뷰 Queue 수를 반영해야 한다(현재 mock은 0 고정). U4는 이 값을 그대로 표시하고 재계산하지 않는다(BR-V1).

## 공용 파일에서 U4가 건드린 부분 (머지 충돌 최소화용)
| 파일 | 변경 |
|---|---|
| `src-tauri/Cargo.toml` | `[dependencies]`에 axum/tokio 2줄 추가, `[dev-dependencies]`를 proptest로 교체(기존 tokio dev-dep은 정식 dependency로 승격되어 불필요) |
| `src-tauri/src/lib.rs` | `pub mod persona;` 1줄 + 모듈 문서 주석 3줄 |
| `package.json`, `tsconfig.json`, `vitest.config.ts`, `src/test-setup.ts` | 신규(이전에 없던 파일) |

그 외 U1/U2/U3 소유 파일은 하나도 수정하지 않았다.
