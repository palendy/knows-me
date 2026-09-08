# Unit of Work — Dependencies (knows-me)

## ⭐ 한눈에 보는 병렬 모델 (핵심 요약)
> **"U1 전체를 먼저 끝내고 나머지"가 아닙니다.**
> **① U1의 "계약(Milestone 0)"만 먼저(짧게) → ② 4명 전원 병렬 개발 → ③ 통합(합류)만 순서대로.**

- **① Milestone 0 (선행, 짧게 · Dev A 주도)**: U1이 **공유 타입 + 서비스 trait + Masker/LlmClient/EncryptedStore trait + mock**만 확정·공개. (코드: `src-tauri/src/core/*` + `src-tauri/src/mocks.rs`, 프런트 `src/shared/contracts.ts`) — **이미 작성 완료.**
- **② 병렬 개발 (전원 동시)**: Milestone 0 직후 **U1 실구현(Dev A) · U2(Dev B) · U3(Dev C) · U4(Dev D)를 동시에** 진행. 아직 없는 의존은 **mock**으로 대체하고 개발.
- **③ 통합 순서 (합류만 순차)**: **U1 → U3 → U2 → U4** 순으로 mock을 실구현으로 교체하며 합류.

즉 **먼저 끝내야 하는 것은 U1 "전체"가 아니라 U1의 "계약"** 이고, 그 계약이 나오면 4명이 곧바로 병렬로 달릴 수 있습니다.

---

## 의존성 매트릭스 (행이 열에 의존)
| ↓ 의존 \ 대상 → | U1 Core/Security | U2 Ingestion/Processing | U3 Knowledge/Interview | U4 Interface/Persona |
|---|:--:|:--:|:--:|:--:|
| **U1 Core & Security** | — | | | |
| **U2 Ingestion & Processing** | ✔ (타입·crypto·Masker·LlmClient) | — | ✔ (Knowledge 쓰기·Interview enqueue) | |
| **U3 Knowledge & Interview** | ✔ (타입·EncryptedStore) | | — | |
| **U4 Interface & Persona** | ✔ (타입·Masker·LlmClient) | | ✔ (Knowledge 읽기) | — |

- **순환 없음**: U1 ← (U2,U3,U4), U3 ← (U2,U4). 단방향.
- U1은 모두의 기반(계약·crypto·LLM). U3는 U2·U4의 공통 데이터 의존.

## 통합 순서(Q2=A) vs 개발 순서(병렬)
- **개발(병렬)**: U1 Milestone 0 완료 즉시 U2·U3·U4 동시 진행(서로/자신 API를 mock).
- **통합(합류) 순서**: **U1 → U3 → U2 → U4**
  1. U1 실구현(타입·crypto·Masker·LlmClient·셸) 확정
  2. U3 실구현(Knowledge/Interview) 합류 → U2·U4의 mock 대체
  3. U2 합류(수집→가공→U3에 실제 기록)
  4. U4 합류(U3 실데이터 조회 + Persona)

## 계약(contract) 경계 — mock 대상
| 제공 Unit | 계약(trait/API) | 소비 Unit |
|---|---|---|
| U1 | 도메인 타입, `EncryptedStore`, `Masker`, `LlmClient`, 서비스 trait | U2, U3, U4 |
| U3 | `KnowledgeApi`(upsert/get/search/graph), `InterviewApi`(enqueue) | U2, U4 |

## 병렬 개발 다이어그램
```mermaid
flowchart LR
    M0["U1 Milestone 0<br/>types + traits + mocks"]
    U1["U1 Core & Security"]
    U2["U2 Ingestion & Processing"]
    U3["U3 Knowledge & Interview"]
    U4["U4 Interface & Persona"]

    M0 --> U2
    M0 --> U3
    M0 --> U4
    M0 --> U1
    U1 -->|integrate 1| INT["Integration"]
    U3 -->|integrate 2| INT
    U2 -->|integrate 3| INT
    U4 -->|integrate 4| INT

    style M0 fill:#FFA726,stroke:#E65100,color:#000
    style INT fill:#4CAF50,stroke:#1B5E20,color:#fff
    linkStyle default stroke:#333,stroke-width:2px
```

### Text Alternative
```
Milestone 0 (U1 contracts+mocks) -> unblocks U1,U2,U3,U4 development in PARALLEL
Integration order: U1 -> U3 -> U2 -> U4
```

## 리스크 & 완화
- **U1 계약 변경 파급**: Dev A가 게이트키핑, 계약 버전 태깅, 변경 시 broadcast.
- **U3 공통 의존 병목**: U3의 API/mock을 Milestone 0 직후 최우선 확정.
- **mock ↔ 실구현 불일치**: 계약에 대한 공용 테스트(contract test) 공유.
