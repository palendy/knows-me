# U2 Business Logic Model — Ingestion & Processing

> 기술 무관 로직 흐름. `Connector`/`IngestionApi`/`ProcessingApi` trait(U1 `core/traits.rs`)를 구현하는 서비스의 알고리즘 설계.

## A. 수집 파이프라인 (Ingestion)

### A.1 IngestionService.trigger(source: Option<SourceKind>) → IngestReport (US-1.1, US-1.2)
```
report = IngestReport{0,0,0}
targets = source.map(|s| [s]).unwrap_or(all_configured_sources)
for src in targets:
    if run_lock.held(src): continue        # US-1.2 AC2: 중복 실행 방지(진행 중 알림)
    run_lock.acquire(src)
    try:
        connector = registry.get(src)
        cursor = cursor_store.load_cursor(src)          # 없으면 None(최초)
        (items, next_cursor) = connector.sync(cursor)   # 소스 접근 실패 시 catch
        for it in items:
            if cursor_store.is_seen(src, it.external_id):   # Q1=A 멱등 판정
                report.skipped += 1
                continue
            enqueue_for_processing(it)     # 가공 대기(원본 저장/큐)
            cursor_store.mark_seen(src, it.external_id, now)
            report.collected += 1
        cursor_store.save_cursor(src, next_cursor)
    catch SourceError:                     # US-1.1 AC3: 오류 기록 후 계속
        report.errors += 1
        log_source_error(src)
    finally:
        run_lock.release(src)
return report
```
- **멱등성 불변식(PBT)**: 동일 입력으로 `trigger`를 2회 실행하면 2회차 `collected == 0`(전부 skipped). → PBT-03.
- **오류 격리**: 한 소스 실패가 다른 소스 수집을 막지 않는다.

### A.2 Connector 구현별 sync 로직
| Connector | external_id | 증분 방식 | "내 것" 필터(Q3=A) |
|---|---|---|---|
| **Session** | 트랜스크립트 파일별 안정 ID(경로 해시+세션id) | 커서=마지막 처리 시각/오프셋; 신규 파일·추가분 | N/A(로컬 내 세션) |
| **Notion** | Notion page id | 커서=last_edited_time; API 증분 | 내가 소유/편집자인 페이지만 |
| **Gmail** | message id | 커서=historyId/internalDate | 내 주소 기준 보낸·받은 메일 |
| **File** | 경로+콘텐츠 식별 | 폴더 감시 이벤트 + 수동 업로드 | N/A |

- **Session 경로(Q2=A)**: 알려진 기본 경로 자동 탐지, `SourceConfig`로 override.
- **인증 만료(US-1.3 AC2)**: `CredentialStore.load`가 없거나 만료면 `AppError`로 재인증 유도. 자격증명은 U1 Vault에 암호화 저장(직접 파일 기록 금지).

### A.3 파일 수집 (US-1.5)
```
on_file(path):
    ext = extension(path)
    match classify_format(ext):        # Q7=A
        Text|Office -> raw = RawItem{File, id(path), now, text=parse_body(path), None}
        Image       -> raw = RawItem{File, id(path), now, None, image_png=read(path)}
                       # 비전 추출은 가공 단계에서 LlmClient.vision_extract
        Unsupported  -> record FileSkipRecord{path, "unsupported: {ext}", now}; return
    if not cursor_store.is_seen(File, raw.external_id): enqueue_for_processing(raw)
```
- 감시(자동)와 수동 업로드는 **동일 파이프라인**으로 수렴(US-1.5 AC2).

## B. 가공 파이프라인 (Processing)

### B.1 ProcessingService.process(items: Vec<RawItem>) → ProcessReport (US-2.1, US-2.2, US-2.3)
```
report = ProcessReport{0,0,0}
for raw in items:
    if not llm_online():                          # Q8=A 오프라인 저하
        pending_store.push(PendingProcessingItem{raw, now, 0})
        continue                                  # 수집·대기는 유지, 가공만 보류
    # 1) 텍스트 확보 (이미지는 비전으로)
    masked_text, unmask_map =
        if raw.image_png: 
            mt = llm.vision_extract(raw.image_png)         # 반환은 MaskedText 계약
            log_transfer(raw.source, VisionExtract, mt.preview)
            (mt, UnmaskMap::empty)                         # 비전 결과는 이미 masked 계약
        else:
            (mt, map) = masker.mask(raw.text ?? "")        # Q5=A: map은 이 스코프 메모리에만
            (mt, map)
    # 2) 요약·분류 (마스킹된 입력만 전송) — US-2.2 AC1
    summary = llm.summarize(masked_text);  log_transfer(raw.source, Summarize, masked_text.preview)
    labels  = llm.classify(masked_text);   log_transfer(raw.source, Classify,  masked_text.preview)
    # 3) 국소 복원(필요 시): 저장용 title/body는 로컬에서 unmask 적용 가능
    body = masker.unmask(summary_as_masked, unmask_map)    # 로컬 범위 복원(US-2.2 AC2)
    # 4) 라우팅 판정 (Q4=A)
    decision = route(labels, summary, raw)
    match decision:
        Store(cand)          -> knowledge.upsert(to_fact(cand, confirmed=true)); report.facts_created += 1
        Confirm(cand)        -> interview.enqueue(confirm_item(cand));  report.queue_items_created += 1
        Deepen{q,h}          -> interview.enqueue(deepen_item(q,h));    report.queue_items_created += 1
        Drop{reason}         -> report.filtered += 1
    # unmask_map 폐기(스코프 종료) — Q5=A
return report
```

### B.2 route() — 저장 vs 필터 vs Queue (Q4=A, US-2.1)
```
route(labels, summary, raw):
    if is_noise(labels):              return Drop{"one-off/noise"}        # AC2
    if is_uncertain(labels):          return Confirm(candidate(summary))   # AC3 → 확인형
    if needs_more_context(labels):    return Deepen{question, hypothesis}  # AC3 → 심화형
    else:                             return Store(candidate(summary))     # AC1 확실
```
- `is_noise / is_uncertain / needs_more_context`는 `LlmClient.classify` 라벨 + U2 임계 규칙(business-rules.md 참조).

### B.3 오프라인 재개
```
resume_pending():                    # 스케줄러/온라인 복귀 시
    if not llm_online(): return
    for p in pending_store.drain():
        process([p.raw])             # 정상 파이프라인 재투입
```

## C. 전송 투명성 (US-2.3)
- 모든 외부 LLM 호출(`summarize`/`classify`/`vision_extract`) 직전/직후 `TransferLogEntry` append.
- `masked_preview`는 **마스킹 후 텍스트에서만** 파생 → 원문 유출 없음(불변식).
- 조회 API는 U4 대시보드가 소비(향후) — U2는 기록만 책임.

## D. 시퀀스 (수집→가공→라우팅) 텍스트 다이어그램
```
Scheduler/Manual → IngestionService.trigger
   → Connector.sync(cursor) → RawItem[] (신규분, 멱등)
   → ProcessingService.process
        → Masker.mask → LlmClient.summarize/classify (+TransferLog)
        → route → { KnowledgeApi.upsert | InterviewApi.enqueue | drop }
```

## E. PBT-01 속성 식별 (테스트는 Code Generation에서)
| 속성 | 유형 | 대상 | 규칙 |
|---|---|---|---|
| 증분 수집 멱등성 | Invariant | `trigger` 2회 → 2회차 collected=0 | PBT-03 |
| 마스킹 왕복 | Round-trip | `unmask(mask(x)) == x` | PBT-02 |
| 마스킹 불변식 | Invariant | 마스킹 출력·TransferLog에 원본 식별정보 부재 | PBT-03 |
| seen 단조성 | Invariant | mark_seen 후 is_seen=true 유지 | PBT-03 |
