# U3 — NFR Design Patterns (Knowledge & Interview)

> How U3's NFR requirements are realized as concrete design patterns. Decisions per approved answers (all A). No external infrastructure (local desktop, Infrastructure Design skipped).

## Patterns applied

### P-1 In-memory inverted index, rebuild-on-unlock  (Performance — NFR-4, Q1=A)
- **What**: `SearchIndex` = `HashMap<Term, HashSet<FactId>>` + `HashMap<FactId, Scope>`, held in RAM.
- **Lifecycle**: built by scanning `EncryptedStore ns=facts` on unlock (<2s @50k, may be async/lazy); updated incrementally on every `upsert`; dropped on lock (no plaintext survives lock).
- **No disk cache** for the index — simplicity + guaranteed consistency over startup speed.
- **Where**: `SearchIndex.build_on_unlock / upsert / drop_on_lock`; `KnowledgeService.search`.

### P-2 Lazy history loading  (Performance/Memory — NFR-4, Q3=A)
- **What**: only the fact index is resident; `fact_history[id]` is read from the store on demand in `history(id)`.
- **Rationale**: history reads are rare; keeps RAM proportional to facts, not to change volume.
- **Where**: `HistoryTracker.history`.

### P-3 CJK bigram tokenization  (Performance/Usability — NFR-4, tech-stack Q2)
- **What**: Latin/numeric → word split + lowercase; CJK runs → 2-gram tokens, enabling Korean substring search without a morphological analyzer.
- **Where**: shared tokenizer helper used by `SearchIndex` (index + query paths, symmetric).

### P-4 Graceful LLM degradation — fail-open on non-critical path  (Resilience — NFR-3, Q2=A)
- **What**: Deepen follow-up generation is best-effort. On `LlmClient` error/timeout → **skip follow-ups, still persist the fact**, log the reason. No retry, no failure surfaced to the user.
- **Invariant**: the network-dependent path never blocks or loses the offline-safe path (fact persistence).
- **Where**: `AnswerIntake.derive_follow_ups` (guarded), `InterviewService.answer`.

### P-5 Self-healing derived state  (Resilience/Consistency — NFR-4/5, Q5)
- **What**: the index is a derived projection of `ns=facts`; if it ever drifts it is fully reconstructed on the next unlock. Write order in `upsert`: append history (if body changed) → write fact → update index (index update last, since it is rebuildable).
- **Where**: `KnowledgeService.upsert`, `SearchIndex.build_on_unlock`.

### P-6 Serialized single-writer  (Consistency — Q5=A)
- **What**: store + index guarded by a lock; commands processed serially. Single-user desktop → negligible contention, no journaling/transactions needed.
- **Where**: `KnowledgeService` / `InterviewService` internal synchronization.

### P-7 Mask gate before egress  (Privacy/Security — NFR-2, KR-7/KR-8)
- **What**: every U3 → cloud LLM call passes `Masker.mask` first; U3 never writes plaintext facts outside `EncryptedStore`. Plaintext appears only via explicit export.
- **Where**: `AnswerIntake.derive_follow_ups`; all persistence via `EncryptedStore`.

### P-8 On-demand export renderer  (Portability — NFR-6, Q4)
- **What**: encrypted canonical store + an export operation that renders facts to a human-readable Markdown+JSON bundle while unlocked (backup / manual edit). Import deferred.
- **Where**: `KnowledgeService` export path (support operation).

### P-9 Idempotent writes / dedup  (NFR-5)
- **What**: no-op `upsert` (unchanged fact) appends no history; `enqueue` suppresses duplicate pending items.
- **Where**: `KnowledgeService.upsert`, `InterviewService.enqueue`.

## Pattern → NFR traceability
| Pattern | NFR |
|---|---|
| P-1, P-2, P-3 | NFR-4 (scale/latency) |
| P-4, P-5 | NFR-3 (offline), reliability |
| P-6, P-5, P-9 | NFR-5, consistency |
| P-7 | NFR-2 (privacy) |
| P-8 | NFR-6 (portability) |

## Explicitly NOT used (and why)
- **No external cache / message broker / circuit breaker**: single local process; the in-memory index is the only "cache".
- **No horizontal scaling patterns**: single-user (D9).
- **No retry/back-off queue for LLM**: MVP graceful degradation chosen (Q2=A).
- **No search engine (tantivy)**: in-memory index meets NFR-4 at target scale (Q1).
