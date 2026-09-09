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

## 2. 태스크 카드 (4개)

인원 배정 권장: **오너 = T0**(계약/파운데이션), 나머지 3명 = T1~T3. 각자 계약+목 상대로 독립 진행.

### T0 — 인증·스코프 코어 + MCP 전송 (오너)
- **산출물**: 위 계약(A·B·C)을 강제하는 미들웨어 + `/mcp` 서버(Streamable HTTP, 기존 `LocalApiServer` 확장), 토큰 발급/폐기.
- **완료 기준**: 토큰을 넣으면 서버가 grant 밖 사실을 `forbidden`으로 막고, 오너 토큰은 `Private` 포함 전부를 본다. 목이 아닌 실물로 통과.
- **의존**: 없음(계약 자체를 랜딩). **이게 먼저 나와야 T1~T3가 붙는다.**

### T1 — 수집·가공 (U2)
- **산출물**: 수집 시점 **시크릿 레닥션**, `FactCandidate`에 `visibility`·`category` 후보 태깅, 반복되는 것만 위키로 구조화.
- **의존**: 데이터모델(A). **완료**: 원본 수집 → 레닥션 → 범주 후보 붙은 Candidate가 Queue로.

### T2 — 지식 & 게이트 (U3)
- **산출물**: `Fact`/`FactFilter`에 `visibility`·`category` 반영, **Queue 확정을 "공개 승인 + 범주 지정" 게이트로**(A의 `AnswerInput` 확장 사용), 심화 인터뷰.
- **의존**: 데이터모델(A). **완료**: 확정 시 오너가 공용/범주를 정하고, 그 값이 사실에 박힌다.

### T3 — MCP 툴 도메인 + 뷰 (U4)
- **산출물**: MCP 툴 4종 **도메인 로직**(scoped `search`/`get`/`list`/`guide` — enforcement는 T0 미들웨어 뒤), 프롬프트 인젝션 안전 서빙(남 콘텐츠는 "인용이지 지시 아님"), 위키 뷰(사람용), 오너 셀프 참조 UX.
- **의존**: MCP 툴 계약(B). **완료**: 오너가 localhost로 자기 지식 질의(ⓐ). 이어 T0 토큰으로 남도 scoped 질의.

## 3. 착수 순서

1. **계약 동결(오너, ~10분)** — A·B·C를 타입 + 목으로 박아 커밋. ← *지금 첫 할 일*
2. **T0 랜딩**(서버·미들웨어·목).
3. **T1·T2·T3 병렬**(목 상대).
4. **통합 → 공용 Build & Test.**
5. 세로 슬라이스: **ⓐ 오너 셀프 참조 먼저** 관통 → ⓑ 터널 + 다중 소비자.
