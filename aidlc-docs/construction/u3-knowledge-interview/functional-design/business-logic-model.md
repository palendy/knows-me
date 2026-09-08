# U3 — Business Logic Model (Knowledge & Interview)

> Technology-agnostic orchestration + algorithms for `KnowledgeService` and `InterviewService`, implementing the fixed `KnowledgeApi` / `InterviewApi` contracts. Decisions per approved answers (all A).

## Layering
```
Frontend (InterviewQueueView, viewer)
   │  Tauri commands (U1 CommandRouter — locked-state gate)
KnowledgeService / InterviewService   (U3 orchestration)
   │
FactStore · HistoryTracker · SearchIndex · QueueManager · AnswerIntake   (U3 components)
   │
EncryptedStore (U1)   +   LlmClient / Masker (U1, used by AnswerIntake)
```
**Text alternative**: The frontend calls Tauri commands (gated for unlock by U1). Commands delegate to the two U3 services, which orchestrate U3's components. Persistence goes through U1's `EncryptedStore`; follow-up question generation goes through U1's `Masker` + `LlmClient`.

---

## A. KnowledgeService

### `upsert(fact) -> FactId`
1. **Validate** (see business-rules KR-1/KR-2): reject if `fact.metadata.confirmed == false` → `InvalidInput`; require non-empty `title` and complete metadata.
2. Read existing `facts[fact.id]`.
3. If it exists **and** the body changed → `HistoryTracker.append(id, FactChange{ changed_at: now, before: Some(old.body), after: fact.body, note })`. If nothing changed, append nothing (keeps history clean; supports idempotent re-writes).
4. `FactStore.put(ns="facts", id, json(fact))`.
5. `SearchIndex.upsert(fact)` — remove old postings for `id`, tokenize `title + body`, add new postings; record `id → scope`.
6. Return `fact.id`.

### `get(id)` / `links(id)` / `history(id)`
- `get`: read `facts[id]`, deserialize, else `NotFound`.
- `links`: return `get(id).links` (explicit only — Q5=A).
- `history`: read `fact_history[id]`, default empty; already ascending.

### `search(query, filter) -> Vec<FactSummary>`  (US-3.3, NFR-4)
1. Tokenize `query` (lowercase, split on non-alphanumeric, drop empties).
2. If tokens empty → candidate set = all indexed ids; else intersect posting lists for each token (AND semantics).
3. Apply `filter.scope` (if `Some`, keep matching scope).
4. Map surviving ids → `FactSummary`. In-memory index keeps this responsive at 10⁴ facts (Q7=A).

### `graph(filter) -> GraphDto`
1. Node set = facts passing `filter.scope`.
2. Edges = for each node `f`, for each `t in f.links` where `t` is also in the node set → `GraphEdge{from:f.id, to:t}` (dangling links skipped, KR-5).

### `dashboard() -> DashboardDto`
- `collected_count` = number of facts.
- `pending_queue` = current queue length (read from `InterviewService`/`QueueManager`).
- `recent_facts` = up to 5 facts with the most recent `confirmed_at`.

### SearchIndex lifecycle
- **build_on_unlock()**: `EncryptedStore.list("facts")` → load each → tokenize → build inverted index. (Invoked by U1 after unlock, or lazily on first query.)
- **incremental**: every `upsert` updates the index (step A.5).
- **drop_on_lock()**: index discarded; nothing plaintext survives a lock.

---

## B. InterviewService

### `enqueue(item) -> QueueItemId`  (US-4.1)
1. Validate kind-specific fields (Confirm has candidate; Deepen has non-empty question).
2. **Dedup/suppress** (IR-4): if a pending item with the same normalized key (Confirm→candidate.title; Deepen→question text) exists, keep the higher-priority one and drop the other; return the surviving id.
3. Assign `priority` via **scoring** (below) if caller left it at default; set `expires_at = created_at + TTL` (Confirm 14d, Deepen 30d) when absent.
4. `QueueManager.put(ns="queue", id, json(item))`; return id.

### `list(sort) -> Vec<QueueItem>`  (US-4.3/AC1)
- Load all queue items; sort `PriorityDesc` (priority desc, tie-break newest) or `NewestFirst` (created_at desc).

### `answer(id, answer) -> AnswerResult`  (US-4.2, US-4.1/AC3 — Q9=A)
1. Load `queue[id]` else `NotFound`.
2. Dispatch on `(kind, answer)`:
   - **Confirm + Choice(affirm)/Text**: build `Fact` from candidate (`confirmed=true`, `confirmed_at=now`), `KnowledgeService.upsert` → `confirmed_fact = Some`. Remove item.
   - **Confirm + Choice(reject)**: discard candidate; no fact. Remove item.
   - **Deepen + Text(answer)**: build `Fact` (title from question, body = answer), upsert → `confirmed_fact = Some`; `derive_follow_ups(answer)` → enqueue each → `follow_ups`. Remove item.
   - **any + Skip**: leave item pending (US-4.2/AC2, no force); `confirmed_fact = None`, `follow_ups = []`.
3. Return `AnswerResult{ confirmed_fact, follow_ups }`.

### `expire() -> usize`  (US-4.3/AC2)
- `now`; remove every item with `expires_at <= now`; also run dedup suppression; return count removed.

---

## C. AnswerIntake — follow-up generation (Q8=A, US-4.1/AC3)

`derive_follow_ups(answer_text) -> Vec<QueueItem>` (bounded, max 3):
1. `Masker.mask(answer_text)` → `(masked, _map)` — **never send raw text** (KR-7, US-2.2).
2. `LlmClient.classify(masked)` and/or `LlmClient.summarize(masked)` → salient topics/keywords.
3. For each new salient topic (up to the cap), create `QueueItem { kind: Deepen{ question, hypothesis }, priority: scored, created_at: now, expires_at: now+30d }`.
4. Return the list (caller enqueues them, which also applies dedup).

**Chain safety**: the per-answer cap (3) plus dedup suppression bounds the follow-up chain so the queue cannot explode.

## D. Priority scoring (Q10=A)
`priority: u8 = clamp_0_255( base + w_source + w_recency + w_need )`, e.g.:
- **base** 100.
- **w_source**: Session +40, Notion/Gmail +25, File +10 (agent-session context weighted highest).
- **w_recency**: +30 if `created_at` within 24h, decaying to 0 over 14d.
- **w_need**: Confirm +20 (needs verification), Deepen +10.

Concrete constants are fixed here so Code Generation is deterministic; they can be tuned later without contract change.

## E. Cross-service consistency
- Confirmed facts reach storage through the **single** `KnowledgeService.upsert` path (services.md principle) — from Confirm affirm, Deepen answer, or U2 processing.
- Every external LLM call in U3 (only in `derive_follow_ups`) passes through `Masker` first (invariant KR-7).
