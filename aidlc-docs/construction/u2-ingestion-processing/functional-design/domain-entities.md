# U2 Domain Entities — Ingestion & Processing

> 기술 무관(technology-agnostic) 도메인 모델. Rust 구현은 Code Generation에서.
> 공유 타입(`RawItem`, `Cursor`, `MaskedText`, `UnmaskMap`, `Fact`, `FactCandidate`, `QueueItem` 등)은 U1 `core/types.rs`가 단일 출처이며, 여기서는 **U2 내부 전용 엔티티**와 공유 타입과의 관계만 정의한다.

## 1. 공유 타입 재확인 (U1 제공, U2 소비)
| 타입 | 역할 | U2에서의 사용 |
|---|---|---|
| `SourceKind` {Session, Notion, Gmail, File} | 소스 식별 | 커넥터·커서·로그 키 |
| `RawItem` {source, external_id, collected_at, text?, image_png?} | 수집된 원본 | 수집 산출 / 가공 입력 |
| `Cursor(String)` | 증분 커서(불투명) | 커넥터별 진행 위치 |
| `SourceConfig(Value)` | 소스별 설정 | 경로/필터 등 |
| `IngestReport` {collected, skipped, errors} | 수집 결과 요약 | trigger 반환 |
| `MaskedText`, `UnmaskMap` | 마스킹 결과·복원맵 | 가공 파이프라인 |
| `FactCandidate` {title, body, provenance, suggested_scope} | 사실 후보 | 확실→Knowledge, 확인형→Queue |
| `ProcessReport` {facts_created, queue_items_created, filtered} | 가공 결과 | process 반환 |

## 2. U2 내부 엔티티

### 2.1 CursorRecord (증분·멱등의 근간)
소스별 마지막 진행 위치를 `EncryptedStore(ns="ingestion.cursor")`에 저장.
```
CursorRecord
  source: SourceKind          # 키
  cursor: Cursor              # 소스별 불투명 커서(다음 sync 시작점)
  updated_at: DateTime<Utc>
```
- **키**: `source` 단일 (소스당 커서 1개).

### 2.2 SeenKey (중복 판정 — Q1=A)
이미 수집한 항목을 식별하는 값. 재수집 멱등성의 핵심.
```
SeenKey = (SourceKind, external_id)     # Q1=A: 조합만으로 판정
```
- `EncryptedStore(ns="ingestion.seen")`에 `key = "{source}:{external_id}"`, value = 최초 수집 시각.
- 판정: `seen.contains(SeenKey)` → 스킵(skipped++), else 신규(collected++)로 처리.

### 2.3 IngestionCursorStore (개념적 저장소)
`CursorRecord` + `SeenKey` 집합을 관리하는 논리 컴포넌트. `EncryptedStore` 위에 얹힌 얇은 계층.
```
IngestionCursorStore
  load_cursor(source) -> Cursor?
  save_cursor(source, cursor)
  is_seen(source, external_id) -> bool
  mark_seen(source, external_id, at)
```

### 2.4 ProcessingDecision (가공 라우팅 — Q4=A)
가공된 항목이 어디로 갈지에 대한 판정 결과.
```
ProcessingDecision (enum)
  Store(FactCandidate)         # 확실 → KnowledgeApi.upsert
  Confirm(FactCandidate)       # 확인 필요 → InterviewApi.enqueue(Confirm)
  Deepen{question, hypothesis} # 심화 필요 → InterviewApi.enqueue(Deepen)
  Drop{reason}                 # 잡음/일회성 → 저장 안 함(filtered++)
```

### 2.5 TransferLogEntry (전송 투명성 — US-2.3, Q6=A)
외부 LLM 호출마다 남기는 append-only 기록. `EncryptedStore(ns="transfer.log")`.
```
TransferLogEntry
  id: Uuid
  at: DateTime<Utc>
  source: SourceKind           # 원본 출처
  operation: TransferOp        # Summarize | Classify | VisionExtract
  masked_preview: String       # 실제 전송된(마스킹 후) 텍스트 요약/발췌
  target: String               # 대상 LLM 식별(예: "cloud-llm")
```
- **불변식**: `masked_preview`는 반드시 마스킹 후 텍스트에서만 파생 (원문 절대 기록 금지).

### 2.6 PendingProcessingItem (오프라인 저하 — Q8=A)
LLM 미연결 시 가공 대기 큐. `EncryptedStore(ns="processing.pending")`.
```
PendingProcessingItem
  raw: RawItem
  queued_at: DateTime<Utc>
  attempts: u32
```

### 2.7 FileSkipRecord (US-1.5 AC3)
미지원 형식 스킵 사유 기록(투명성·디버깅).
```
FileSkipRecord
  path: String
  reason: String               # "unsupported extension: .xyz" 등
  at: DateTime<Utc>
```

## 3. 엔티티 관계 (텍스트 다이어그램)
```
[Connector(Session|Notion|Gmail|File)]
      | sync(cursor)  → (Vec<RawItem>, next Cursor)
      v
[IngestionService] --uses--> [IngestionCursorStore] --on--> EncryptedStore(cursor, seen)
      | RawItem (신규분만, 멱등)
      v
[ProcessingService]
      | 1) Masker.mask(text) → (MaskedText, UnmaskMap)   [Q5=A: map은 메모리 국소]
      | 2) LlmClient.summarize/classify/vision_extract    → TransferLogEntry 기록
      | 3) 판정 → ProcessingDecision
      v
  Store  → KnowledgeApi.upsert(Fact)
  Confirm/Deepen → InterviewApi.enqueue(QueueItem)
  Drop   → filtered++
```

## 4. 저장 네임스페이스 요약 (EncryptedStore)
| ns | 내용 | 성격 |
|---|---|---|
| `ingestion.cursor` | 소스별 `CursorRecord` | mutable(덮어씀) |
| `ingestion.seen` | `SeenKey` → 최초 수집 시각 | append/lookup |
| `transfer.log` | `TransferLogEntry` | append-only |
| `processing.pending` | `PendingProcessingItem` | 큐(추가/제거) |
| `ingestion.file_skip` | `FileSkipRecord` | append-only |

> 주: `UnmaskMap`은 **저장하지 않는다**(Q5=A) — 가공 처리 스코프의 메모리에만 존재하고 처리 종료 시 폐기. 어떤 ns에도 기록되지 않음.
