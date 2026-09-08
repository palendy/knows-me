# U2 NFR Design Patterns — Ingestion & Processing

> NFR을 U2 설계에 못박는 패턴 목록. 각 패턴은 NFR ID와 스토리에 추적된다.

## P1. 마스킹 게이트웨이 (Security Gateway) — 최우선
- **문제**: 원문이 실수로 마스킹 없이 `LlmClient`로 전송될 수 있음(R1 유출).
- **패턴**: U2 내부에 `LlmGateway` 래퍼 컴포넌트를 두고, **텍스트 경로는 반드시 `mask → TransferLog.append → LlmClient` 순서**를 통과. `ProcessingService`는 `LlmClient`를 직접 호출하지 않고 항상 `LlmGateway` 경유. (Q1=A)
- **강제**: `LlmGateway`가 `MaskedText`만 받도록 시그니처 설계 → 원문(String) 직접 호출 경로가 타입상 존재하지 않음.
- **비전 경로**: `vision_extract`는 이미지 입력이며 반환이 `MaskedText` 계약이므로 게이트웨이가 로그만 담당.
- **추적**: U2-NFR-SEC1/SEC2, BR-K1, US-2.2.

## P2. 재시도 + 타임아웃 (Resilience — Retry with Backoff)
- **패턴**: LLM 호출은 타임아웃 30s, 최대 3회, 지수 백오프(1s→2s→4s). 소진 시 예외를 삼키지 않고 **pending 이월**로 전환. (Q2=A)
- **적용 범위**: `LlmGateway`의 summarize/classify/vision_extract 호출만. 네트워크성 오류에 한정(4xx 인증오류는 재시도 대신 재인증 신호).
- **추적**: U2-NFR-REL2, BR-O.

## P3. 오프라인 저하 큐 (Graceful Degradation — Pending Queue)
- **패턴**: `llm_online()==false`거나 재시도 소진 시 `PendingProcessingItem`을 `EncryptedStore(ns=processing.pending)`에 적재. 수집·원본 대기는 계속.
- **재개**: U1 Scheduler의 주기 tick에서 online이면 `resume_pending()`이 drain하여 정상 가공 경로 재투입(수집 배치와 동일 주기 편승). (Q3=A)
- **계측**: 오프라인은 `errors`가 아니라 pending으로 카운트.
- **추적**: U2-NFR-OFF1/2/3, BR-O1~O3, NFR-3.

## P4. 멱등 게이트 (Idempotency Gate)
- **패턴**: 수집 시 `IngestionCursorStore.is_seen((source, external_id))`로 신규만 통과. 처리 성공 후에만 `mark_seen`. 커서는 sync 성공 시에만 전진.
- **불변식**: 재실행 시 collected=0(PBT-03).
- **추적**: U2-NFR-IDEM1/2, BR-I1~I5, NFR-5.

## P5. 소스 오류 격리 (Bulkhead per Source)
- **패턴**: `trigger` 루프가 소스별 try/catch로 격리. 한 소스 실패가 다른 소스/앱을 중단시키지 않음. errors++ 기록 후 계속.
- **추적**: U2-NFR-REL1, BR-I4, US-1.1 AC3.

## P6. 단일 실행 잠금 (Single-Flight Lock)
- **패턴**: 소스별 in-memory run-lock. 진행 중 재트리거 시 중복 실행 없이 "진행 중" 반환. (Q4=A, 단일 인스턴스 가정)
- **추적**: BR-M2, US-1.2 AC2.

## P7. 감사 가능 투명성 로그 (Append-Only Audit Log)
- **패턴**: 모든 외부 호출마다 `TransferLogEntry` append-only(`ns=transfer.log`). 편집/삭제로 흔적 은폐 금지. `masked_preview`는 마스킹 후 텍스트에서만 파생.
- **추적**: U2-NFR-SEC2, BR-T1~T3, US-2.3, NFR-2.

## P8. 플러그형 커넥터 레지스트리 (Plugin Registry)
- **패턴**: `ConnectorRegistry`가 `SourceKind → Box<dyn Connector>` 매핑 보유. 신규 소스는 등록만으로 추가(다른 계층 변경 0).
- **추적**: U2-NFR-EXT1, NFR-7.

## P9. 결정적 마스킹 규칙 (Deterministic, Offline-First Masking)
- **패턴**: `regex` 규칙 집합(이메일·전화·API키/토큰·URL 자격증명) + 사용자 사전(이름 등). 네트워크·비용 없이 항상 동작. 미검출은 패턴/위치만 로깅(원문 금지).
- **주의**: 실제 `Masker` 구현은 U1 소유. U2는 규칙 요구사항을 U1에 제공하고 계약을 소비. (통합 시 U1 Masker가 이 규칙을 만족해야 함 → 계약 테스트로 검증)
- **추적**: U2-NFR-SEC5, BR-K4, NFR-2/R1.

## 패턴 ↔ NFR 매트릭스
| 패턴 | 주요 NFR | 스토리 |
|---|---|---|
| P1 마스킹 게이트웨이 | SEC1/2 | US-2.2 |
| P2 재시도/타임아웃 | REL2 | US-2.1 |
| P3 pending 큐 | OFF1/2/3 | US-2.1 |
| P4 멱등 게이트 | IDEM1/2 | US-1.1 |
| P5 오류 격리 | REL1 | US-1.1 |
| P6 실행 잠금 | REL(동시성) | US-1.2 |
| P7 투명성 로그 | SEC2 | US-2.3 |
| P8 커넥터 레지스트리 | EXT1 | US-1.3~1.5 |
| P9 결정적 마스킹 | SEC5 | US-2.2 |
