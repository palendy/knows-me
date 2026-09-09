# 강화 컨셉 — 계약 + 분담

> 목적: 병렬 진행을 위한 계약(정본 `mcp-contract.md`) + 그 위에서 쪼갠 분담.
> 계약 surface는 `sharing::`로 동결됨(PR #6). 각자 `sharing::MockSharing` 상대로 독립 진행.

## 1. 계약 (정본: `mcp-contract.md`)

계약 정본은 **[`mcp-contract.md`](mcp-contract.md)** 하나다 — 툴 4종(`list_categories`/`search_knowledge`/`get_page`/`get_guide`), 인가 시맨틱, 에러 코드, 서빙 안전 규칙, 요구 공유 타입. **여기서 중복 기술하지 않는다**(둘로 갈리면 계약이 둘이 된다).

코드 상태(PR #6, **동결·구현됨**):
- 공유 타입: `core::types::{Visibility(기본 Private), Category(정규화 생성자)}`, `FactMetadata`에 `visibility`·`category`.
- 계약 surface: `sharing::{Token, SharingApi, MockSharing, AccessError}` — `mcp-contract.md`의 Rust 인코딩. 불변식(토큰 파생 grant, `Shared ∩ 부여범주`, 인자는 좁히기만)이 테스트로 고정됨.

각 유닛은 이 `sharing::` 계약 + `MockSharing` 상대로 병렬 진행한다.

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
