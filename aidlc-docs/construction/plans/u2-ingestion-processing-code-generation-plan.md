# U2 Code Generation Plan (Ingestion & Processing)

> **이 문서가 Code Generation의 단일 출처(single source of truth)다.**
> Owner: Dev B · Branch: `construction/u2-ingestion-processing`
> Project: Greenfield, modular monolith → 코드 위치 `src-tauri/src/{ingestion,processing}/` (Rust crate `knows_me_core`).

## Unit 컨텍스트
- **구현 스토리**: US-1.1~1.5(수집), US-2.1~2.3(가공/마스킹/투명성)
- **구현 계약**: `Connector`, `IngestionApi`, `ProcessingApi` (U1 `core/traits.rs`)
- **소비 계약(개발 중 mock)**: U1 `Masker`/`LlmClient`/`CredentialStore`/`EncryptedStore`, U3 `KnowledgeApi`/`InterviewApi`
- **소유 데이터(EncryptedStore ns)**: `ingestion.cursor`, `ingestion.seen`, `transfer.log`, `processing.pending`, `ingestion.file_skip`
- **경계**: 스케줄러·Masker 실구현·저장 암호화는 U1 소유. U2는 계약만 소비.

## MVP 구현 범위 결정 (심사·시간 고려)
- **완전 구현(실로직+테스트)**: 멱등/증분 골격, Session 커넥터(로컬 파일 기반), File 커넥터(텍스트/MD 파싱 + 미지원 스킵), LlmGateway(마스킹 게이트+재시도+로그), Router, PendingQueue, IngestionService, ProcessingService, CursorStore.
- **골격+명시적 TODO(계약·시그니처만, 통합 시 채움)**: Notion/Gmail 커넥터의 실제 API 호출(OAuth 토큰 필요), PDF/DOCX 파싱(외부 crate), 이미지 비전(LlmClient.vision_extract 경로만 배선). — 각각 `todo!()` 대신 명확한 stub + 로그 + 사유. 
- 근거: 실행계획 Success Criteria = MVP; 외부 인증/무거운 파서는 통합 단계에서 실연결. 골격은 계약을 만족해 컴파일·테스트 가능해야 함.

## PBT 계획 (Partial 모드 — blocking: PBT-02/03/07/08)
| 속성 | 규칙 | 대상 |
|---|---|---|
| 마스킹 왕복 `unmask(mask(x))==x` | PBT-02 | (계약 기반) `Masker` — U2는 `NoopMasker`로 왕복 성질 + 규칙 마스커 도입 시 확장 |
| 증분 수집 멱등성(재실행 collected=0) | PBT-03 | IngestionService + mock 커넥터 |
| seen 단조성(mark 후 is_seen 유지) | PBT-03 | IngestionCursorStore |
| 마스킹 잔존 부재(출력에 원본 식별정보 없음) | PBT-03 | LlmGateway 경유 시 TransferLog preview |
| 생성기 품질(식별정보 포함 텍스트, RawItem 시퀀스) | PBT-07 | proptest strategy |
| shrinking + seed 재현 | PBT-08 | proptest 기본 + regressions 파일 |
- example 기반 테스트도 함께 작성(PBT-10, advisory).

---

## 생성 단계 (순차, 각 완료 시 [x])

### Step 1: 프로젝트 구조 & 의존성 (Greenfield)
- [ ] `src-tauri/Cargo.toml`에 U2 deps 추가: `regex`, (stub 경계) reqwest/oauth2/notify/pdf-extract/docx-rs는 **주석 처리 + 통합 시 활성** (빌드 무게·오프라인 빌드 고려), `[dev-dependencies] proptest`
- [ ] `src-tauri/src/lib.rs`에 `pub mod ingestion; pub mod processing;` 등록
- [ ] `ingestion/mod.rs`, `processing/mod.rs` 생성
- 스토리: (구조) 전체

### Step 2: Ingestion — CursorStore & 멱등 게이트 (Business Logic)
- [ ] `ingestion/cursor_store.rs`: `IngestionCursorStore`(EncryptedStore 위) — load/save_cursor, is_seen/mark_seen (ns 직렬화)
- 스토리: US-1.1(AC2 멱등), NFR-5

### Step 3: Ingestion — Connector 레지스트리 & 구현
- [ ] `ingestion/registry.rs`: `ConnectorRegistry`
- [ ] `ingestion/connectors/session.rs`: Session 커넥터(로컬 트랜스크립트 경로 스캔, external_id=경로+id, 자동탐지+SourceConfig override) — **완전 구현**
- [ ] `ingestion/connectors/file.rs`: File 커넥터(텍스트/MD 파싱 완전 구현; PDF/DOCX·이미지는 형식 분기+stub; 미지원=FileSkipRecord)
- [ ] `ingestion/connectors/notion.rs`, `gmail.rs`: 계약 구현 골격(“내 것” 필터·증분 로직 주석, 실제 API 호출은 통합 TODO, 인증 없으면 재인증 신호)
- 스토리: US-1.1, US-1.3, US-1.4, US-1.5

### Step 4: Ingestion — Service + 실행잠금 (API Layer)
- [ ] `ingestion/service.rs`: `IngestionService`(`impl IngestionApi`) — trigger 루프(멱등·오류격리·리포트), RunLock(소스별 in-memory), configure
- 스토리: US-1.1, US-1.2

### Step 5: Processing — LlmGateway (보안 경계) + TransferLog
- [ ] `processing/transfer_log.rs`: `TransferLog`(append-only, EncryptedStore ns=transfer.log)
- [ ] `processing/llm_gateway.rs`: `LlmGateway` — `MaskedText`만 수용, mask는 호출측 강제; summarize/classify/vision 경유 시 재시도(30s·3회·백오프) + TransferLog append
- 스토리: US-2.2(마스킹 강제), US-2.3(투명성)

### Step 6: Processing — Router + PendingQueue
- [ ] `processing/router.rs`: classify 라벨→`ProcessingDecision`(Store/Confirm/Deepen/Drop), 라벨부재=보수적 Confirm
- [ ] `processing/pending.rs`: `PendingQueue`(EncryptedStore ns=processing.pending) push/drain
- 스토리: US-2.1

### Step 7: Processing — Service (오케스트레이션)
- [ ] `processing/service.rs`: `ProcessingService`(`impl ProcessingApi`) — process: (online?) mask→gateway→router→KnowledgeApi/InterviewApi 라우팅, offline=pending, UnmaskMap 스코프 폐기, resume_pending, ProcessReport
- 스토리: US-2.1, US-2.2, US-2.3, NFR-3

### Step 8: 단위 테스트 (example 기반)
- [ ] `ingestion` 테스트: 멱등 재실행 collected=0, 소스오류 격리, run-lock, file 미지원 스킵
- [ ] `processing` 테스트: 마스킹 게이트 경유 확인, router 분기(Store/Confirm/Drop), offline→pending→resume, TransferLog append
- 계약 소비는 U1/U3 mock(`crate::mocks`) 사용
- 스토리: 전체 AC

### Step 9: PBT 테스트 (PBT-02/03/07/08 blocking)
- [ ] `processing`: 마스킹 왕복(PBT-02), 마스킹 잔존 부재 불변식(PBT-03) — proptest strategy(식별정보 포함 텍스트)
- [ ] `ingestion`: 증분 멱등성(PBT-03), seen 단조성(PBT-03) — RawItem 시퀀스 strategy(PBT-07)
- [ ] shrinking/seed 재현 확인 주석(PBT-08)

### Step 10: 문서 요약 (markdown, aidlc-docs)
- [ ] `aidlc-docs/construction/u2-ingestion-processing/code/code-summary.md`: 생성 파일 목록·스토리 커버리지·통합 TODO(Notion/Gmail/PDF/DOCX/비전)·테스트 목록

### Step 11: 빌드·테스트 검증 (샌드박스)
- [ ] `cargo build`, `cargo test`(U2 포함), `cargo clippy --all-targets -- -D warnings` green 확인 → code-summary에 결과 기록
- (배포 아티팩트: Tauri 패키징은 Build and Test 단계에서 다룸 — 이 단계 N/A)

---

## 스토리 추적성
| 스토리 | 단계 |
|---|---|
| US-1.1 자동수집(배치) | S2,S3(session),S4 |
| US-1.2 수동 트리거 | S4(run-lock) |
| US-1.3 Notion | S3(골격) |
| US-1.4 Gmail | S3(골격) |
| US-1.5 파일 | S3(file),  |
| US-2.1 요약·분류 | S6,S7 |
| US-2.2 마스킹 | S5,S7,S9 |
| US-2.3 투명성 | S5,S7 |

## 규모 요약
- 총 11단계. 신규 Rust 파일 ~13개(ingestion 7 + processing 5 + 요약). 완전 구현 6~7개, 골격+TODO 3~4개.
- 프론트엔드: U2는 command 노출만 — 이 단계에서 UI 없음(N/A).
