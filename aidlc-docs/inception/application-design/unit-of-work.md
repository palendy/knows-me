# Unit of Work (knows-me)

> **성격**: 단일 Tauri 앱 = 모듈러 모놀리스. Unit = 논리 모듈(독립 배포 아님).
> **팀**: 4명, 1인 1오너, contract-first 병렬 개발.
> 근거: `unit-of-work-plan.md`(Q1=B→4개, Q2=A, Q3=A, Q4=A+병렬 조정).

## U1 — Core Platform & Security  〔Owner: Dev A〕
- **책임**:
  - Tauri 앱 셸, CommandRouter, AppState, Scheduler
  - **공유 도메인 타입**: Fact, FactChange, QueueItem, DTO(Dashboard/Graph/…), SourceKind, AppError
  - **전 서비스 trait 계약**: Ingestion/Processing/Knowledge/Interview/Persona/Security 인터페이스
  - **보안**: KeyManager(비밀번호→KDF→키, 잠금/해제), Vault(AES-256-GCM 저장·자격증명)
  - **공유 LLM 게이트웨이**: Masker(마스킹/복원), LlmClient(클라우드 호출)
  - Onboarding/Unlock UI
- **인터페이스 제공(다른 Unit이 의존)**: 도메인 타입, 서비스 trait, `EncryptedStore` 게이트, `Masker`/`LlmClient` trait
- **스토리**: US-7.1~7.3 (+ 횡단 계약·보안)
- **Milestone 0(선행)**: 위 타입·trait·mock을 먼저 확정·공개 → U2/U3/U4 병렬 착수 가능

## U2 — Ingestion & Processing  〔Owner: Dev B〕
- **책임**:
  - Connector trait + Session/Notion/Gmail/File 구현, CursorStore(증분·멱등), IngestionService
  - ProcessingService: Masker(U1) 적용 → LlmClient(U1) 요약·분류 → SummarizerClassifier, TransferLog(전송 투명성)
  - 확실=Knowledge(U3)로, 불확실=Interview(U3)로 전달
- **의존**: U1(타입·crypto·Masker·LlmClient), U3(Knowledge/Interview trait — 개발 중 mock)
- **스토리**: US-1.1~1.5, US-2.1~2.3

## U3 — Knowledge & Interview  〔Owner: Dev C〕
- **책임**:
  - FactStore(파일 위키 md/json + 링크), HistoryTracker(이력), SearchIndex, KnowledgeService
  - QueueManager(확인형/심화형·우선순위·만료), AnswerIntake(전환 없는 답변), InterviewService
  - Queue UI(전환 없는 답변: 선택지+직접입력)
- **의존**: U1(타입·EncryptedStore)
- **제공(U2·U4가 의존)**: Knowledge 읽기/쓰기 API, Interview enqueue API — **우선 확정·mock 제공**
- **스토리**: US-3.1~3.3, US-4.1~4.3

## U4 — Interface & Persona  〔Owner: Dev D〕
- **책임**:
  - Dashboard/MiniHome/Graph 뷰 + 조회 command
  - PersonaService(맥락 조합·챗·초안), LocalApiServer(localhost, `POST /chat` `POST /draft`, MVP 외부 노출 없음)
  - PersonaChat UI
- **의존**: U1(타입·Masker·LlmClient), U3(Knowledge 읽기 — 개발 중 mock)
- **스토리**: US-5.1~5.3, US-6.1~6.2

---

## 코드 조직 전략 (Greenfield, Q3=A)
```
knows-me/
  src-tauri/                 # Rust 단일 crate (Tauri 코어)
    Cargo.toml
    tauri.conf.json
    src/
      main.rs                # 진입점
      core/                  # U1: types, traits, command_router, app_state, scheduler
      security/              # U1: key_manager, vault
      llm/                   # U1: masker, llm_client
      ingestion/             # U2: connector(trait)+session/notion/gmail/file, cursor_store, service
      processing/            # U2: summarizer_classifier, transfer_log, service
      knowledge/             # U3: fact_store, history, search_index, service
      interview/             # U3: queue_manager, answer_intake, service
      persona/               # U4: persona_service, local_api_server
  src/                       # React + TypeScript (Vite)
    features/
      onboarding/            # U1
      queue/                 # U3
      dashboard/ minihome/ graph/   # U4
      persona-chat/          # U4
    shared/                  # 공통 UI/타입
  screenshots/  (or result/) # 시연 스크린샷 (심사 기준 ④)
```
- **모듈 경계 = Unit 경계**. U1의 `core/traits.rs`가 계약의 단일 출처.
- **시크릿**: 코드 하드코딩 금지 → Vault/OS 키체인·환경변수 (심사 기준 ⑥).

## Unit별 오너십 & 병렬성 요약
| Unit | Owner | 선행/제공 | 개발 병렬성 |
|---|---|---|---|
| U1 | Dev A | Milestone 0로 계약·mock 선공개 | 선행(짧게) 후 통합·보안 심화 |
| U2 | Dev B | U1 계약 + U3 mock 사용 | 병렬 |
| U3 | Dev C | U1 계약 사용, 자신의 API mock 선제공 | 병렬 |
| U4 | Dev D | U1 계약 + U3 mock 사용 | 병렬 |
