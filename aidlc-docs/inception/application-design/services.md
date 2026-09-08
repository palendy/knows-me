# Services (knows-me)

> 도메인 서비스 정의·책임·오케스트레이션. 프런트는 명령/조회만, 서비스가 컴포넌트를 조율(계층형).

## S1. IngestionService
- **책임**: 커넥터 등록/스케줄링, 증분·멱등 수집, 원본을 ProcessingService로 전달.
- **오케스트레이션**:
  1. Scheduler tick 또는 `trigger_ingest` → 각 Connector `sync(cursor)` 호출
  2. CursorStore로 신규분만 선별(멱등)
  3. 수집된 RawItem을 ProcessingService로 넘김
  4. IngestReport 반환(수집 수/건너뜀/오류)
- **협력**: Connector*, CursorStore, ProcessingService, Vault(자격증명), TransferLog(간접)

## S2. ProcessingService
- **책임**: 원본 → (마스킹) → 요약·분류 → 사실 후보/Queue 아이템 생성.
- **오케스트레이션**:
  1. RawItem 수신
  2. Masker.mask() 적용 → 마스킹된 텍스트만 LlmClient로
  3. SummarizerClassifier.process()로 반복/필요 항목 선별·요약·분류
  4. 확실→FactCandidate, 불확실→InterviewService(Queue 아이템)
  5. LLM 호출은 TransferLog.record()로 투명성 기록
- **협력**: Masker, LlmClient, SummarizerClassifier, TransferLog, KnowledgeService, InterviewService

## S3. KnowledgeService
- **책임**: 확정 사실의 저장·이력·검색·그래프 조회 제공.
- **오케스트레이션**:
  - 확정 사실 → FactStore.upsert() + HistoryTracker.append() + SearchIndex.index()
  - 조회: search_facts / get_graph / get_minihome / get_dashboard 데이터 제공
- **협력**: FactStore, HistoryTracker, SearchIndex

## S4. InterviewService
- **책임**: Queue 아이템 생성·정렬·만료, 답변 수용→확정 사실 반영, 후속 질문 연쇄.
- **오케스트레이션**:
  1. ProcessingService/자체 분석이 확인형·심화형 아이템 enqueue
  2. `list_queue`로 우선순위 정렬 제공, `expire()` 주기 정리
  3. `answer_item` → AnswerIntake.answer() → 확정 사실은 KnowledgeService로, 후속 질문은 재enqueue
- **협력**: QueueManager, AnswerIntake, KnowledgeService

## S5. PersonaService (+ LocalApiServer)
- **책임**: 확정 맥락으로 페르소나 컨텍스트 구성, 챗/초안 생성. 로컬 REST 노출.
- **오케스트레이션**:
  1. build_context() ← KnowledgeService 조회
  2. chat/draft → Masker 적용 후 LlmClient.chat()
  3. LocalApiServer가 `POST /chat`, `POST /draft`를 PersonaService로 위임(MVP localhost 전용)
- **협력**: KnowledgeService, Masker, LlmClient, TransferLog

## S6. SecurityService
- **책임**: 키 라이프사이클(설정/해제/잠금), 저장·자격증명 암호화 게이트.
- **오케스트레이션**:
  - setup_password/unlock/lock → KeyManager
  - 모든 영속화(FactStore/Vault/CursorStore/TransferLog)는 열린 KeyHandle을 통해 암호화 I/O
  - 잠금 상태에서는 CommandRouter가 조회/수집 command를 거부
- **협력**: KeyManager, Vault, (전 서비스의 저장 경로 게이트)

---

## 서비스 상호작용 요약(주요 흐름)
```
수집:   Scheduler/Command -> IngestionService -> ProcessingService
가공:   ProcessingService -(Masker/LLM)-> {KnowledgeService | InterviewService}
인터뷰: Frontend -> InterviewService -> KnowledgeService (확정 시)
조회:   Frontend -> KnowledgeService
페르소나: Frontend/LocalApiServer -> PersonaService -> KnowledgeService + LLM
보안:   전 서비스의 저장 I/O -> SecurityService(KeyManager/Vault) 게이트
```

## 오케스트레이션 원칙
- **프런트는 상태를 갖지 않고** command/query만 호출.
- **서비스 경계 = 트랜잭션/일관성 경계**. 확정 사실 반영은 KnowledgeService 단일 경로.
- **모든 외부 LLM 호출은 Masker 통과 + TransferLog 기록**(불변식).
- **모든 영속화는 잠금 해제(KeyHandle) 상태에서만** 수행.
