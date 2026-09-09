# 강화 컨셉 — 계약 + 4개 태스크 카드

> 목적: 4명이 서로 안 기다리고 병렬로 가기 위한 **계약 동결** + 그 위에서 쪼갠 태스크.
> 착수 전 유일한 전제 = **계약(아래 1)**. 이것만 타입+목으로 박히면 태스크가 독립적으로 굴러간다.

## 1. 먼저 동결할 계약 (병렬의 전제)

기존 타입에 얹는 최소 변경만 정의한다. (파일: `src-tauri/src/core/types.rs`, `traits.rs`)

### A. 데이터모델 (공유 타입)

- **접근 축을 새로 추가.** 기존 `Scope { Company, Personal, Unknown }`은 *주제* 분류이지 접근제어가 아니다. 별개로:
  - `Visibility { Private, Shared }` — 기본 `Private`(오너·오너 에이전트만). `Shared`만 남에게 노출.
  - `Fact.metadata`에 `visibility: Visibility` + `category: String` 추가. (`category`가 grant 단위, 예: `deploy`, `project-x`, `workstyle`)
- `FactFilter`에 `category: Option<String>` 추가 (기존 `scope: Option<Scope>` 옆에).
- **Queue 확정 = 공개 승인 + 범주 지정 게이트.** `AnswerInput`에 확정 시 `visibility`·`category`를 실어 보낼 수 있게 확장(예: `Confirm { visibility, category }`).

### B. MCP 툴 계약 (surface — 팀이 의존하는 유일한 표면)

| 툴 | 시그니처 | 의미 |
|---|---|---|
| `list_categories` | `() -> [Category]` | 이 토큰이 접근 가능한 범주 |
| `search_knowledge` | `(query, category?) -> [FactSummary]` | 스코프 안에서만 검색 |
| `get_fact` | `(id) -> Fact` | 스코프 밖이면 `forbidden` |
| `get_guide` | `(category?) -> string` | 오너 가이드(온보딩 대신) |

- 에러 enum: `unauthorized`(토큰 없음/무효) vs `forbidden`(인증됐으나 범위 밖).
- **불변식**: 신원·grant는 토큰에서만. 요청 인자는 이미 허용된 범위를 *좁히기만*. 인가는 서버 단일 지점.

### C. 토큰 ↔ 범주 계약

- `Token { id, granted_categories: Set<String>, owner: bool }`
- **오너 토큰** = 전 범주 + `Private` 포함(셀프 참조). **그 외** = `granted_categories`에 속한 `Shared` 사실만.

> 이 계약의 **enforcement 구현**(스코프 판정 로직 + MCP 전송 뼈대 + 전송경계 강제)은 **오너가 제공**한다(아래 T0). 팀은 위 surface에만 의존한다.

## 2. 실제 분담 (진행 현황 반영)

계약(§1)이 동결됐으므로, 현재 각자 진행 중인 작업 위에서 이렇게 나뉜다. **새 필드는 기본 `Private`(`#[serde(default)]`)라 아래 상류 작업자들은 필드를 몰라도 안 깨진다** — 조율 부담이 사실상 없다.

### MCP 벌티컬 — 오너(나) solo
계약(§1) 위 전 구간을 한 명이 세로로 소유한다.
- **MCP 전송** — Streamable HTTP(기존 `LocalApiServer` 확장), 토큰 발급/폐기.
- **스코프 강제** — `sharing::Token` 불변식을 서버 단일 지점에서. grant 밖은 `forbidden`(존재도 은닉), 오너 토큰은 `Private` 포함 전부.
- **MCP 툴 도메인 로직** — scoped `search`/`get`/`list`/`guide`, 실 저장소 기반.
- **전송경계 안전 서빙** — 남 콘텐츠는 "인용이지 지시 아님".
- **분류는 소비만** — 사실의 `Shared`/`category` 지정은 수집+활용 담당(아래)이 한다. MCP는 그 분류를 **읽어 스코프를 강제**할 뿐, 게이트를 소유하지 않는다.
- **터널 + 공유 설정** — cloudflared/Tailscale, 토큰 발급·URL/상태 표시.
- **완료**: ⓐ 오너 localhost 셀프 질의 → ⓑ 토큰으로 남도 scoped 질의.

### 수집 + 활용 + 개인/공용 분리 — (진행 중, 담당 지정됨)
- 상류: 사실 생산·요약·활용. **개인용/공용 데이터 분리를 이 담당이 맡는다** — 사실에 `visibility`(Private/Shared)와 `category`(§1 필드)를 지정.
- **교차점(오너↔이 담당)**: 단일 저장소(`EncryptedStore`)를 공유 소스로 쓰고, **범주 vocabulary와 "언제 Shared가 되는지"를 합의**한다. MCP는 그 결과를 읽어 강제한다.

### 앱 UI 다듬기 — (진행 중)
- 화면 완성도. MCP 벌티컬과 **직접 교차 없음**. (분류 UI가 필요하면 수집+활용 담당과 UI 담당이 조율.)

### Notion 연결 — (진행 중)
- 커넥터 → RawItem. 상류라 MCP와 무관. Notion 사실도 기본 `Private`.

## 3. 착수 순서

1. **이 PR(#6) 머지** — 네 명이 같은 데이터모델을 공유(아무도 안 깨짐).
2. **오너: MCP 벌티컬 solo 착수** — 전송·토큰·스코프 → 툴 로직 → 터널. (분류는 수집+활용 담당이 상류에서.)
3. 나머지 셋은 현재 트랙 그대로 병렬(상류+분류 / UI / Notion). **조율 2건: (a) 단일 저장소 공유, (b) 오너↔수집담당의 분류 semantics — 범주·Shared 기준.**
4. 세로 슬라이스: **ⓐ 오너 셀프 참조 먼저** → ⓑ 터널 + 다중 소비자.
