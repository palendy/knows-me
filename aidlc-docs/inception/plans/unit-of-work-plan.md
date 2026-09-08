# Unit of Work Plan (knows-me)

**목적**: 시스템을 독립적으로 설계·구현 가능한 **작업 단위(Unit)** 로 분해.
**성격**: 단일 Tauri 앱 = **모듈러 모놀리스**(Unit = 논리 모듈, 독립 배포 아님).

---

## 실행 체크리스트 (승인 후 생성)
- [x] `unit-of-work.md` — Unit 정의·책임 + (greenfield) 코드 조직 전략
- [x] `unit-of-work-dependency.md` — Unit 의존성 매트릭스·순서
- [x] `unit-of-work-story-map.md` — 스토리 ↔ Unit 매핑(전 스토리 할당)
- [x] Unit 경계·의존성 검증, 전 스토리 할당 확인 (총 22 스토리 전부 할당, 순환 없음)

---

## 예비 Unit 분해 (초안 — 도메인 서비스 정렬)
| Unit | 포함(컴포넌트/서비스) | 스토리 |
|---|---|---|
| **U1 Security & Onboarding** | KeyManager, Vault, SecurityService, Onboarding/Unlock UI | US-7.1~7.3 |
| **U2 Knowledge** | FactStore, HistoryTracker, SearchIndex, KnowledgeService | US-3.1~3.3 |
| **U3 Ingestion** | Connector trait + Session/Notion/Gmail/File, CursorStore, IngestionService | US-1.1~1.5 |
| **U4 Processing** | Masker, LlmClient, SummarizerClassifier, TransferLog, ProcessingService | US-2.1~2.3 |
| **U5 Interview** | QueueManager, AnswerIntake, InterviewService, Queue UI | US-4.1~4.3 |
| **U6 Interface** | Dashboard/MiniHome/Graph 뷰 + 관련 조회 command | US-5.1~5.3 |
| **U7 Persona & Local API** | PersonaService, LocalApiServer, PersonaChat UI | US-6.1~6.2 |

의존 순서(초안): **U1 → U2 → {U3, U4} → U5 → {U6, U7}**

---

## 확인 질문 (Step 3)
각 `[Answer]:` 뒤에 보기 letter. 맞는 게 없으면 Other. 모두 답하면 "완료"(또는 "추천대로").

## Question 1
Unit 분해 입도(개수)는?

A) 위 초안대로 **7개 도메인 Unit**(Security/Knowledge/Ingestion/Processing/Interview/Interface/Persona) — AD와 1:1, 설계·구현 단위 명확 (추천)

B) 더 굵게 **3~4개**로 묶기(예: Backend Core / Frontend / Persona-API)

C) 더 잘게(커넥터별 분리 등)

D) Other (please describe after [Answer]: tag below)

[Answer]: B) 

## Question 2
Unit 구현 순서(의존 기반)는?

A) **U1 Security → U2 Knowledge → U3 Ingestion / U4 Processing → U5 Interview → U6 Interface / U7 Persona** (기반→상위 순, 추천)

B) 인터페이스 먼저(UI 스켈레톤) 후 백엔드 채우기

C) Other (please describe after [Answer]: tag below)

[Answer]: A)

## Question 3
디렉터리/코드 조직(Greenfield)은?

A) **Tauri 표준**: `src-tauri/`(Rust, Unit별 모듈 `mod`) + `src/`(React, 기능별 폴더). 단일 crate (추천)

B) **Cargo 워크스페이스**(Unit별 멀티 crate) + `src/`(React) — 경계 강제 강함, 초기 복잡도↑

C) Other (please describe after [Answer]: tag below)

[Answer]: A)

## Question 4
프런트엔드(뷰) 배치는?

A) **단일 Interface Unit(U6)** 에 조회 뷰 모음, 각 기능 Unit은 자신의 전용 UI(온보딩·Queue·챗)만 포함 (추천)

B) 프런트엔드를 별도 통합 Unit으로 완전히 분리(모든 뷰)

C) 각 기능 Unit이 자신의 프런트+백엔드를 모두 포함(수직 슬라이스)

D) Other (please describe after [Answer]: tag below)

[Answer]: A)

---

## 답변 분석 & 4인 병렬 조정 (확정 분해)

**답변**: Q1=B(3~4개 굵게), Q2=A(의존 순서), Q3=A(Tauri 표준 단일 crate), Q4=A(단일 Interface Unit).
**추가 지시**: 팀 4명, **4명이 병렬 개발** 가능하도록 분해.

**조정 결정**:
- Q1=B + 4명 → **정확히 4개 Unit**(1인 1오너)로 확정.
- Q2=A의 의존 순서는 **"통합 순서"** 로 사용하고, 개발은 **계약 우선(contract-first)** 으로 병렬화.
- Q4=A는 유지하되 균형을 위해 **읽기 전용 Interface 뷰를 Persona와 한 Unit으로 결합**(각 기능 전용 UI는 해당 Unit에 유지 → Q4 취지 보존). ← 초안(7 Unit) 대비 변경점.

### 확정 4개 Unit (1인 1오너)
| Unit | 오너 | 포함(서비스/컴포넌트) | 스토리 |
|---|---|---|---|
| **U1 Core Platform & Security** | Dev A | Tauri 앱 셸, CommandRouter/AppState/Scheduler, **공유 도메인 타입(Fact/QueueItem/DTO/SourceKind/Error)**, **서비스 trait 계약 전체**, KeyManager/Vault/SecurityService, **Masker + LlmClient(공유 LLM 게이트웨이)**, Onboarding/Unlock UI | US-7.1~7.3 (+ 공유 계약·보안) |
| **U2 Ingestion & Processing** | Dev B | Connector trait+Session/Notion/Gmail/File, CursorStore, IngestionService, SummarizerClassifier, TransferLog, ProcessingService | US-1.1~1.5, US-2.1~2.3 |
| **U3 Knowledge & Interview** | Dev C | FactStore/HistoryTracker/SearchIndex/KnowledgeService, QueueManager/AnswerIntake/InterviewService, Queue UI | US-3.1~3.3, US-4.1~4.3 |
| **U4 Interface & Persona** | Dev D | Dashboard/MiniHome/Graph 뷰 + 조회 command, PersonaService, LocalApiServer, PersonaChat UI | US-5.1~5.3, US-6.1~6.2 |

### 병렬 개발 전략 (contract-first)
- **Milestone 0 (짧게, Dev A 주도 + 4인 합의)**: U1이 **공유 타입 + 전 서비스 trait + 암호화 저장 게이트 + LLM/Masker trait**의 시그니처와 스텁(mock)을 먼저 확정·공개.
- 이후 **U2·U3·U4는 trait/mock에 대고 동시 개발**. 실제 구현은 통합 시 교체.
- **통합 순서(Q2=A)**: U1 → U3(Knowledge) → U2 → U4. (개발은 병렬, 합류만 이 순서)
- **인터페이스 경계로 결합 최소화**: U2/U4는 U3를 Knowledge/Interview trait로만 호출, U2/U4는 U1의 crypto·LLM·타입에만 의존. 순환 없음.

### 코드 조직 (Q3=A, greenfield)
```
src-tauri/                # Rust 단일 crate
  src/
    core/                 # U1: types, traits, command router, scheduler, app state
    security/             # U1: key_manager, vault
    llm/                  # U1: masker, llm_client (공유)
    ingestion/            # U2: connector(trait)+impls, cursor_store, service
    processing/           # U2: summarizer_classifier, transfer_log, service
    knowledge/            # U3: fact_store, history, search_index, service
    interview/            # U3: queue_manager, answer_intake, service
    persona/              # U4: persona_service, local_api_server
src/                      # React + TS
  features/
    onboarding/           # U1
    queue/                # U3
    dashboard/ minihome/ graph/   # U4
    persona-chat/         # U4
  shared/                 # 공통 컴포넌트/타입(프런트)
```

### 병렬성 리스크 & 완화
- **U1 선행 부담**: Milestone 0을 최소 계약으로 짧게 끊고, 이후 Dev A는 통합·보안 심화로 이동.
- **공유 타입/trait 변경 파급**: 계약 변경은 U1 오너가 게이트키핑, 버전 태그로 관리.
- **U3가 U2·U4의 공통 의존**: U3의 읽기/쓰기 API를 우선 확정·mock 제공.
