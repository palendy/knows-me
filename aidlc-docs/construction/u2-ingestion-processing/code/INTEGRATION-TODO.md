# U2 INTEGRATION-TODO (다음 개발자 필독)

> U2는 계약(U1 `traits.rs`)을 만족하는 상태로 **오프라인 빌드·테스트 가능**하게 구현되었다.
> 아래 항목은 외부 인증/네트워크/무거운 파서가 필요해 **통합 단계로 의도적으로 미룬** 것이다.
> 각 항목은 코드에도 `INTEGRATION-TODO(US-x.x)` 주석으로 표시되어 있다. 통합 순서(U1→U3→U2→U4)에서 U2 합류 시 이 문서를 기준으로 채운다.

## 1. Notion 커넥터 — `src-tauri/src/ingestion/connectors/notion.rs`  (US-1.3)
현재: 계약 만족 skeleton(항상 빈 결과, 커서 보존). 안전한 no-op이라 오케스트레이션·멱등 로직은 이미 검증됨.
할 일:
1. `Cargo.toml`의 `reqwest`, `oauth2` 주석 해제.
2. `CredentialStore::load(SourceKind::Notion)`로 OAuth 토큰 로드. 없거나 만료 → `AppError::External("reauth required: notion")` 반환(재인증 유도, BR-C3). **토큰 하드코딩/평문 저장 금지**(BR-C2, 심사기준⑥).
3. 증분: `cursor`를 마지막 `last_edited_time`으로 사용, 이후 편집분만 조회(BR-I/NFR-5).
4. "내 것" 필터(Q3=A/BR-C1): 소유/편집자 페이지만.
5. 각 페이지 → `RawItem { source: Notion, external_id: page_id, text: Some(plain) }`.
6. 최대 `last_edited_time`을 다음 `Cursor`로 반환.

## 2. Gmail 커넥터 — `src-tauri/src/ingestion/connectors/gmail.rs`  (US-1.4)
현재: 계약 만족 skeleton.
할 일: Notion과 동일 패턴 +
- 증분: `historyId`/`internalDate`를 `cursor`로.
- "내 것": 내 주소 기준 보낸/받은 메일.
- 본문은 민감 가능 → 가공 단계에서 **반드시 마스킹 통과**(US-1.4 AC2 / BR-C4 / BR-K1). 이미 `ProcessingService`가 mask→gateway 경로를 강제하므로 커넥터는 원문 text만 채우면 됨.

## 3. 파일 커넥터 오피스 파싱 — `src-tauri/src/ingestion/connectors/file.rs`  (US-1.5)
현재: 텍스트/MD 완전 구현, 이미지=비전 경로로 `image_png` 세팅 완료. PDF/DOCX는 `FileFormat::Office` 분기에서 **스킵 + INTEGRATION-TODO 사유 기록**.
할 일:
1. `Cargo.toml`의 `pdf-extract`, `docx-rs` 주석 해제.
2. `to_raw_item`의 `FileFormat::Office` 분기에서 PDF=`pdf-extract`, DOCX=`docx-rs`로 본문 추출 → `RawItem.text`.
3. 파싱 실패는 기존과 동일하게 `FileSkipRecord`에 사유 기록(BR-F2).

## 4. 폴더 감시(watcher) — `src-tauri/src/ingestion/connectors/file.rs`  (US-1.5 AC1)
현재: `ingest_paths()`로 수동/배치 경로 주입(동일 파이프라인). 자동 감시 미구현.
할 일:
1. `Cargo.toml`의 `notify` 주석 해제.
2. 감시 폴더의 FS 이벤트를 받아 `FileConnector::ingest_paths`로 밀어넣기(BR-F3: 감시·수동 동일 파이프라인).
3. 이벤트 유실 대비 주기 배치 폴백 유지.

## 5. 이미지 비전 실호출 — `LlmClient::vision_extract` (U1 소유)
현재: `ProcessingService`가 이미지 `RawItem`을 `LlmGateway::vision_extract`로 라우팅하는 배선 완료. 실제 비전 품질은 U1 `LlmClient` 실구현에 의존.
할 일: U1 `LlmClient` 실구현이 vision을 실제 처리하는지 계약 테스트로 확인.

## 6. `IngestionApi::configure` 영속화 — `src-tauri/src/ingestion/service.rs`
현재: no-op placeholder. 커넥터 생성(SourceConfig 포함)은 wiring 시점에서 처리.
할 일: per-source `SourceConfig`를 `EncryptedStore`에 저장하고 커넥터 재빌드(경로 override 등 런타임 반영).

## 7. 스케줄러 배선 (U1 소유, U2 계약 노출)
현재: `IngestionService::trigger` / `ProcessingService::resume_pending`가 준비됨.
할 일: U1 Scheduler가 주기 tick에서 (a) `trigger(None)` (b) online 시 `resume_pending()` 호출(Q3=A/Q7=A).

## 8. 마스킹 규칙 실구현 (U1 `Masker` 소유)
현재: U2는 `NoopMasker`(mock)로 왕복·라우팅 검증. 실제 결정적 규칙(이메일·전화·토큰·URL 자격증명 정규식 + 사용자 사전)은 U1 `Masker` 실구현이 담당(Q8=A).
할 일: U1 Masker가 PBT-02(왕복)·PBT-03(잔존 부재) 속성을 만족하는지 **공용 계약 테스트**로 검증. U2의 `tests/u2_pbt.rs` masker_roundtrip을 U1 실구현으로도 돌릴 수 있게 공유.

---
### 통합 시 체크
- [ ] `Cargo.toml` 외부 crate 주석 해제 후 `cargo build` green
- [ ] Notion/Gmail 실제 토큰으로 증분·멱등 동작 확인
- [ ] PDF/DOCX 샘플 파싱 + 미지원 스킵 사유 확인
- [ ] 감시 폴더 이벤트 → 수집 반영
- [ ] U1 실 Masker로 PBT-02/03 통과(마스킹 잔존 부재 실검증)
- [ ] TransferLog에 원문 미노출 재확인(BR-K5)
