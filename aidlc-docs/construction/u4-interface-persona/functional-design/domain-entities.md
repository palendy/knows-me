# U4 Domain Entities (Interface & Persona)

> 기술 중립. U1 공유 타입(`knows_me_core::core::types`)은 **재정의하지 않고 소비**한다.
> U4가 새로 정의하는 것은 "조회 결과를 화면/페르소나가 쓰기 좋은 형태로 만든 파생 엔티티"뿐이다.

## 1. 소비하는 U1 공유 타입 (읽기 전용)
| 타입 | 출처 | U4 용도 |
|---|---|---|
| `Fact`, `FactSummary`, `FactId` | U1 `core::types` | 맥락 조합·뷰 렌더링 |
| `FactMetadata`, `Scope`, `Provenance` | U1 | 확정 여부·범위 필터 |
| `DashboardDto`, `MiniHomeDto`, `GraphDto`, `GraphNode`, `GraphEdge` | U1 | 조회 command 반환 |
| `PersonaReply`, `DraftRequest`, `Draft`, `DraftKind` | U1 | 페르소나 입출력 |
| `MaskedText`, `UnmaskMap` | U1 | LLM 전송 전 마스킹 |
| `GraphFilter`, `FactFilter` | U1 | 그래프/검색 필터 |
| `AppError`, `Result<T>` | U1 | 오류 전파 |

## 2. U4 신규 엔티티

### E1. `PersonaContext`
페르소나가 답할 때 근거로 삼는 **확정 맥락의 스냅샷**.

| 필드 | 타입 | 설명 |
|---|---|---|
| `entries` | `Vec<ContextEntry>` | 선택된 확정 사실들 (상한 `max_facts`) |
| `total_confirmed` | `usize` | 전체 확정 사실 수 (선택 전 **모집단** 크기). `select_context`가 받은 후보 슬라이스에서 세지 않고 **호출자가 측정해 넘긴다** — 후보는 `fetch_cap`으로 잘린 부분집합이라 여기서 세면 과소 보고된다 |

- **불변식 C1**: `entries`의 모든 항목은 `metadata.confirmed == true`.
- **불변식 C2**: `entries.len() <= max_facts`.
- **불변식 C3**: `entries` 안에 동일 `FactId`가 두 번 나타나지 않는다.
- **불변식 C4**: `entries.is_empty()` ⟺ 근거 없음 → LLM 호출 금지(BR-P4).

### E2. `ContextEntry`
| 필드 | 타입 | 설명 |
|---|---|---|
| `id` | `FactId` | 출처 사실 식별자(응답 근거 추적용) |
| `title` | `String` | 사실 제목 |
| `body` | `String` | 사실 본문 (LLM 프롬프트에 들어가는 실제 텍스트) |
| `scope` | `Scope` | Company/Personal/Unknown |
| `relevance` | `u32` | 질의 대비 연관 점수 (내림차순 정렬 키) |

### E3. `ContextSelection` (정책 값 객체)
| 필드 | 타입 | 기본값 | 설명 |
|---|---|---|---|
| `max_facts` | `usize` | 12 | 맥락에 넣을 최대 사실 수 (NFR-4 지연 통제) |
| `scope` | `Option<Scope>` | `None` | 범위 제한(예: 업무용 초안은 Company만) |

### E4. `PersonaPrompt`
LLM에 보내기 **직전** 형태. 소유자 데이터가 있는 쪽과 없는 쪽을 필드로 분리한다.

| 필드 | 타입 | 설명 |
|---|---|---|
| `system` | `String` | 페르소나 지시문 + 초안 종류별 형식 지시. **정적 템플릿 — 소유자 데이터 없음** |
| `user_document` | `String` | 맥락 본문 + 소유자 질문. **마스킹 대상 전부가 여기 모인다** |

- **불변식 C5**: 소유자 데이터는 전부 `user_document`에 있고, 이것만 `Masker.mask()`를 거쳐 `LlmClient`로 전달된다. `system`은 소유자 데이터를 포함하지 않으므로 마스킹이 필요 없다(BR-P2/BR-P2a).
- **불변식 C5a**: 요청당 `mask()` 호출은 **정확히 한 번**이다. 두 번 이상이면 서로 다른 `UnmaskMap`의 placeholder가 충돌할 수 있다.

### E5. `GraphLayout` / `PositionedNode` (뷰 파생, 프론트엔드)
`GraphDto`(논리 그래프)를 화면 좌표로 옮긴 **순수 함수 결과**.

| 필드 | 타입 | 설명 |
|---|---|---|
| `nodes` | `PositionedNode[]` | `{ id, label, x, y, degree }` |
| `edges` | `LayoutEdge[]` | `{ from, to, x1, y1, x2, y2 }` |
| `width`, `height` | `number` | 뷰박스 크기 |

- **불변식 C6**: 레이아웃은 결정적이다 — 같은 `GraphDto` + 같은 크기 → 같은 좌표 (난수·시간 미사용).
- **불변식 C7**: `layout(g).nodes.length == g.nodes.length` (노드 보존).
- **불변식 C8**: 모든 좌표는 `[0, width] x [0, height]` 안에 있다.
- **불변식 C9**: 양 끝 노드가 모두 존재하는 엣지만 남는다(dangling edge 제거).

### E6. `LocalApiRequest` / `LocalApiResponse` (로컬 API 경계 DTO)
| 엔드포인트 | 요청 | 응답 |
|---|---|---|
| `POST /chat` | `{ "prompt": string }` | `{ "text": string }` |
| `POST /draft` | `{ "kind": "Email"\|"Message"\|"Post", "prompt": string }` | `{ "text": string }` |
| `GET /health` | — | `{ "status": "ok", "persona": bool }` |

- **불변식 C10**: 요청/응답 JSON은 왕복 무손실이다 — `decode(encode(x)) == x` (PBT-02).

## 3. 엔티티 관계
```text
   U3 KnowledgeApi (mock during dev)
            |  search / dashboard / graph
            v
   +--------------------+        select (ContextSelection)
   |  PersonaService    |------------------------------+
   +--------------------+                              v
            |  build PersonaPrompt              PersonaContext
            v                                    (ContextEntry*)
     U1 Masker.mask(user_document)  --> MaskedText + UnmaskMap (memory only)
            |                              (system은 정적 템플릿 — 마스킹 불필요)
            |
            v
     U1 LlmClient.chat()  --> raw reply
            |
            v
     U1 Masker.unmask()   --> PersonaReply / Draft
            ^
            |  delegate
   +--------------------+
   | LocalApiServer     |  127.0.0.1 only (D7)
   +--------------------+
```

## 4. Testable Properties (PBT-01)

| 컴포넌트 | 속성 | 카테고리 | 강제 |
|---|---|---|---|
| `LocalApiRequest`/`Response` JSON | `decode(encode(x)) == x` | Round-trip | PBT-02 (blocking) |
| `PersonaContext` 조합 | 결과가 항상 확정 사실만 포함 (C1) | Invariant | PBT-03 (blocking) |
| `PersonaContext.total_confirmed` | 호출자가 측정한 모집단 크기를 그대로 보고 (E1) | Invariant | PBT-03 (blocking) |
| `PersonaContext` 조합 | `len <= max_facts` (C2) | Invariant | PBT-03 (blocking) |
| `PersonaContext` 조합 | `FactId` 중복 없음 (C3) | Invariant | PBT-03 (blocking) |
| 미니홈피 하이라이트 선정 | 입력 순서를 섞어도 결과 동일(전순서) | Invariant | PBT-03 (blocking) |
| 미니홈피 하이라이트 선정 | 출력 ⊆ 입력, 길이 `<= limit` | Invariant | PBT-03 (blocking) |
| `GraphLayout` | 노드 수 보존 (C7) | Invariant | PBT-03 (blocking) |
| `GraphLayout` | 좌표가 뷰박스 안 (C8) | Invariant | PBT-03 (blocking) |
| `GraphLayout` | 결정적 — 두 번 호출 결과 동일 (C6) | Idempotence/Invariant | PBT-03 (blocking) |
| `GraphLayout` | dangling edge 제거 (C9) | Invariant | PBT-03 (blocking) |
| 마스킹 경유 | 어떤 입력에도 `LlmClient` 인자에 원문 식별자가 없음 | Invariant | PBT-03 (blocking) |

**No PBT properties identified**: `LocalApiServer`의 소켓 바인딩/수명 관리(순수 로직이 아닌 I/O — 예제 기반 통합 테스트로 커버), React 컴포넌트 렌더 자체(스냅샷/RTL 예제 테스트로 커버).
