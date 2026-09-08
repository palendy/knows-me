# U2 Code Generation Summary — Ingestion & Processing

> Branch: `construction/u2-ingestion-processing`. Rust crate `knows_me_core`.
> 빌드/테스트 검증: `cargo build` OK, `cargo test` **35 pass (32 unit + 3 PBT)**, `cargo clippy --all-targets -- -D warnings` **clean** (macOS 네이티브 cc).

## 생성 파일 (Created)
### Ingestion (`src-tauri/src/ingestion/`)
- `mod.rs` — 모듈 진입
- `cursor_store.rs` — `IngestionCursorStore`: 멱등 게이트(seen) + 증분 커서 (EncryptedStore 위)
- `registry.rs` — `ConnectorRegistry`: SourceKind→Connector 플러그 레지스트리
- `service.rs` — `IngestionService`(`impl IngestionApi`) + `RawItemSink`/`BufferSink`: trigger 오케스트레이션, 소스 오류 격리, single-flight run-lock
- `connectors/mod.rs`
- `connectors/session.rs` — **완전 구현**: 로컬 트랜스크립트 스캔, 자동탐지+SourceConfig override, mtime 증분
- `connectors/file.rs` — 텍스트/MD **완전 구현**, 이미지=비전 경로, PDF/DOCX=스킵+INTEGRATION-TODO, 미지원=스킵+사유
- `connectors/notion.rs` — **skeleton**(INTEGRATION-TODO US-1.3)
- `connectors/gmail.rs` — **skeleton**(INTEGRATION-TODO US-1.4)

### Processing (`src-tauri/src/processing/`)
- `mod.rs`
- `transfer_log.rs` — `TransferLog` append-only 전송 투명성 로그(마스킹 후 preview만)
- `llm_gateway.rs` — `LlmGateway`: **보안 경계**(MaskedText만 수용 → mask→log→호출 강제) + 재시도(3회, 백오프 1→2→4s)
- `router.rs` — `route()`/`ProcessingDecision`: classify 라벨→Store/Confirm/Deepen/Drop, 라벨부재=보수적 Confirm
- `pending.rs` — `PendingQueue`: 오프라인 저하 큐(push/drain, requeue=attempts 증가)
- `service.rs` — `ProcessingService`(`impl ProcessingApi`): 전체 파이프라인 오케스트레이션 + `resume_pending`

### Tests
- `src-tauri/tests/u2_pbt.rs` — PBT(proptest): 마스킹 왕복(PBT-02), 수집 멱등성/seen 단조성(PBT-03), 도메인 생성기(PBT-07), shrinking/seed 재현(PBT-08)

## 수정 파일 (Modified)
- `src-tauri/Cargo.toml` — `regex`, `tokio(time)` 추가; `proptest`(dev); 외부 커넥터/파서 crate는 주석(통합 시 활성)
- `src-tauri/src/lib.rs` — `pub mod ingestion; pub mod processing;` 등록

## 스토리 커버리지
| 스토리 | 구현 | 상태 |
|---|---|---|
| US-1.1 세션 자동수집(배치) | SessionConnector + IngestionService + CursorStore | ✅ 완전 |
| US-1.2 수동 트리거 | IngestionApi.trigger + run-lock | ✅ 완전(진행중 중복방지) |
| US-1.3 Notion | NotionConnector | 🟡 skeleton(TODO#1) |
| US-1.4 Gmail | GmailConnector | 🟡 skeleton(TODO#2) |
| US-1.5 파일 | FileConnector(텍스트/MD/이미지) | ✅ 텍스트/이미지 / 🟡 PDF·DOCX·watcher(TODO#3,4) |
| US-2.1 요약·분류 | LlmGateway + Router + ProcessingService | ✅ 완전 |
| US-2.2 마스킹 | LlmGateway(MaskedText 강제) + 로컬 unmask | ✅ (실 규칙은 U1 Masker, TODO#8) |
| US-2.3 전송 투명성 | TransferLog(append-only, masked preview) | ✅ 완전 |

## PBT 준수 (Partial 모드 — blocking)
| 규칙 | 구현 | 결과 |
|---|---|---|
| PBT-02 왕복 | `masker_roundtrip` | ✅ pass |
| PBT-03 불변식 | `ingestion_is_idempotent`, `seen_is_monotonic` | ✅ pass |
| PBT-07 생성기 | `identifier_text()`, `raw_items()` strategy | ✅ |
| PBT-08 shrinking/seed | proptest 기본 + `.proptest-regressions` | ✅ |
| PBT-09 프레임워크 | proptest(dev-dependency) | ✅ |
| PBT-10 보완성(advisory) | example 테스트 32개 병행 | ✅ |

## 통합 미완 항목
→ **`INTEGRATION-TODO.md`** 참조(Notion/Gmail 실 API, PDF/DOCX 파서, 폴더 watcher, configure 영속화, 스케줄러 배선, U1 실 Masker 계약 검증). 코드에도 `INTEGRATION-TODO(US-x.x)` 주석으로 표시.

## 경계 준수
- 스케줄러·Masker 실구현·저장 암호화는 U1 소유 → U2는 계약만 소비.
- U3 Knowledge/Interview는 개발 중 mock(`crate::mocks`) 사용, 통합 시 실구현으로 교체.
- 시크릿 하드코딩 0 (자격증명은 CredentialStore 경유; 현재 skeleton은 토큰 미사용).
