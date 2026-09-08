# U2 Logical Components — Ingestion & Processing

> NFR 패턴을 반영한 U2 논리 컴포넌트 구조. Rust 모듈 `src-tauri/src/ingestion/`, `src-tauri/src/processing/`에 매핑.

## 컴포넌트 목록

### 1. ConnectorRegistry  (ingestion)
- **책임**: `SourceKind → Box<dyn Connector>` 매핑 보유·조회.
- **패턴**: P8(플러그형 레지스트리).
- **계약**: 각 값은 U1 `Connector` trait 구현.

### 2. Connectors: Session / Notion / Gmail / File  (ingestion)
- **책임**: 소스별 `sync(cursor) -> (Vec<RawItem>, Cursor)`, `id()`, `supports_manual()`.
- **의존**: Notion/Gmail → `reqwest`+`oauth2`+`CredentialStore`(U1). File → `notify`+`pdf-extract`/`docx-rs`.
- **패턴**: P5(격리 대상), P4(external_id 안정성 제공).

### 3. IngestionCursorStore  (ingestion)
- **책임**: `load/save_cursor`, `is_seen/mark_seen`. `EncryptedStore`(U1) 위 얇은 계층.
- **패턴**: P4(멱등 게이트).
- **ns**: `ingestion.cursor`, `ingestion.seen`.

### 4. RunLockRegistry  (ingestion)
- **책임**: 소스별 in-memory 실행 잠금(single-flight).
- **패턴**: P6.

### 5. IngestionService  (ingestion)  ← `IngestionApi` 구현
- **책임**: `configure`, `trigger`. Registry+CursorStore+RunLock 오케스트레이션, 소스 오류 격리, IngestReport 집계.
- **패턴**: P4/P5/P6.
- **의존**: ConnectorRegistry, IngestionCursorStore, RunLockRegistry, (하류) ProcessingService.

### 6. LlmGateway  (processing)  — 보안 경계
- **책임**: 텍스트 경로 `mask → TransferLog.append → LlmClient` 강제. 재시도/타임아웃/backoff. `MaskedText`만 수용하는 시그니처.
- **패턴**: P1(마스킹 게이트웨이), P2(재시도), P7(로그).
- **의존**: `Masker`(U1), `LlmClient`(U1), TransferLog.

### 7. TransferLog  (processing)
- **책임**: `TransferLogEntry` append-only 기록/조회. `EncryptedStore` ns=`transfer.log`.
- **패턴**: P7.

### 8. MaskingRuleSpec  (processing, 문서·설정)
- **책임**: 결정적 마스킹 규칙 요구사항(정규식 목록·사용자 사전 형식) 정의. **실구현 Masker는 U1**; U2는 규칙 명세를 제공하고 계약 테스트로 검증.
- **패턴**: P9.

### 9. Router  (processing)
- **책임**: classify 라벨+임계 규칙으로 `ProcessingDecision`(Store/Confirm/Deepen/Drop) 판정. 라벨 부재 시 보수적 Confirm.
- **패턴**: (라우팅 로직) BR-P2/P3.

### 10. PendingQueue  (processing)
- **책임**: `PendingProcessingItem` 적재/drain. `EncryptedStore` ns=`processing.pending`.
- **패턴**: P3(오프라인 저하).

### 11. ProcessingService  (processing)  ← `ProcessingApi` 구현
- **책임**: `process(items)`. LlmGateway로 요약·분류, Router 판정, KnowledgeApi/InterviewApi 라우팅, PendingQueue 관리, ProcessReport 집계, UnmaskMap 스코프 폐기.
- **의존**: LlmGateway, Router, PendingQueue, `KnowledgeApi`+`InterviewApi`(U3, 개발 중 mock).

## 컴포넌트 관계 (텍스트)
```
[U1 Scheduler tick] --주기--> IngestionService.trigger + ProcessingService.resume_pending
                                     |                              |
ConnectorRegistry → Connector.sync   |                              | (online 시 drain)
        |  RawItem[]                  |                              v
        +----> IngestionCursorStore (멱등) --신규--> ProcessingService.process
                                                          |
                                            LlmGateway(mask→log→LlmClient, retry)
                                                          |
                                              Router(ProcessingDecision)
                                       ┌──────────────────┼───────────────────┐
                                  KnowledgeApi.upsert  InterviewApi.enqueue   drop
                                     (Store)          (Confirm/Deepen)      (filtered)
                              (LLM 불가 시 → PendingQueue)
```

## 컴포넌트 ↔ 계약(trait) 매핑
| 컴포넌트 | 구현/소비 계약 |
|---|---|
| IngestionService | **impl** `IngestionApi` |
| ProcessingService | **impl** `ProcessingApi` |
| Session/Notion/Gmail/File | **impl** `Connector` |
| LlmGateway | **소비** `Masker`, `LlmClient` (U1) |
| IngestionCursorStore/TransferLog/PendingQueue | **소비** `EncryptedStore` (U1) |
| Connectors(Notion/Gmail) | **소비** `CredentialStore` (U1) |
| ProcessingService | **소비** `KnowledgeApi`, `InterviewApi` (U3, mock) |

## 인프라성 요소
- 큐: PendingQueue(영속·EncryptedStore 기반, 외부 브로커 없음 — 로컬 앱).
- 잠금: in-memory RunLock(단일 인스턴스).
- 회로차단기/캐시: MVP 불필요(개인 PoC, 외부 호출은 재시도+pending으로 충분).
