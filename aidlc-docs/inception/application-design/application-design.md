# Application Design (knows-me) — 통합본

> `components.md` · `component-methods.md` · `services.md` · `component-dependency.md` 통합 요약.
> **범위**: MVP(요구사항 §5.1). **상세 로직/NFR은 CONSTRUCTION(Unit별)에서.**

## 1. 확정된 아키텍처 결정
| # | 결정 | 내용 |
|---|---|---|
| AD1 | 프런트엔드 | **React + TypeScript** (Tauri 웹뷰), 상태 비보유·명령/조회만 |
| AD2 | 백엔드 | **Rust 코어 단일** — 수집·가공·저장·암호화·서버 전부 Rust, 프런트는 Tauri command |
| AD3 | 커넥터 | **공통 `Connector` trait + 소스별 구현**(플러그형) |
| AD4 | 페르소나 API | **Rust 내장 HTTP 서버**(localhost), MVP 외부 노출 없음 |
| AD5 | 통신/결합 | **계층형 + 서비스 오케스트레이션**(순환 의존 없음) |

## 2. 컴포넌트/서비스 개요
- **Frontend**: OnboardingView, DashboardView, InterviewQueueView, MiniHomeView, GraphView, PersonaChatView, SettingsView
- **App Layer**: CommandRouter, AppState, Scheduler
- **Domain Services**: IngestionService, ProcessingService, KnowledgeService, InterviewService, PersonaService(+LocalApiServer), SecurityService
- **Components**: Connector(trait)+Session/Notion/Gmail/File, CursorStore / Masker, LlmClient, SummarizerClassifier, TransferLog / FactStore, HistoryTracker, SearchIndex / QueueManager, AnswerIntake / PersonaService, LocalApiServer / KeyManager, Vault

## 3. 핵심 흐름 (요약)
1. **수집** Scheduler/Command → IngestionService(Connector.sync, 증분·멱등)
2. **가공** ProcessingService → Masker → LlmClient(요약·분류) → 확실=Knowledge / 불확실=Interview (+TransferLog)
3. **인터뷰** Frontend → InterviewService(전환 없는 답변) → 확정 사실 KnowledgeService 반영
4. **조회** Frontend → KnowledgeService(대시보드/미니홈피/그래프/검색)
5. **페르소나** Frontend/LocalApiServer → PersonaService → Knowledge + (Masker+LLM)
6. **보안** 모든 저장/자격증명 I/O → SecurityService(KeyManager/Vault) 게이트

## 4. 횡단 규칙(불변식)
- 외부 egress = 커넥터 동기화 + LLM 호출뿐. **LLM 경로는 Masker 필수 + TransferLog 기록.**
- **확정 사실 기록은 KnowledgeService 단일 경로.**
- **잠금 해제 상태에서만** 저장/수집/조회 수행.
- **새 소스 확장 = Connector 구현 추가**(타 계층 무변경).

## 5. PBT 대상 예고(NFR-8 / Functional Design PBT-01에서 확정)
- Masker mask/unmask — 왕복 + "출력에 원본 식별정보 없음" 불변식
- Vault encrypt/decrypt — 왕복
- FactStore 직렬화/역직렬화 — 왕복

## 6. 스토리 커버리지
E1→Ingestion, E2→Processing, E3→Knowledge, E4→Interview, E5→Frontend+Knowledge, E6→Persona, E7→Security. (personas P1 소유자 전부; P2 제3자·원격 노출은 범위 외)

## 7. Units Generation 입력 힌트(다음 단계)
서비스 경계가 자연스러운 Unit 후보: **Security/온보딩**, **Ingestion(커넥터)**, **Processing/마스킹**, **Knowledge**, **Interview**, **Interface(Frontend)**, **Persona/LocalApi**. 의존 순서상 Security→Knowledge→Ingestion/Processing→Interview→Interface/Persona 경향.
