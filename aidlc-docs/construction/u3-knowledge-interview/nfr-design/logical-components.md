# U3 — Logical Components (Knowledge & Interview)

> Logical view of U3's components, their integration, and dependencies. Layered + service-orchestration, no cyclic dependencies (AD5). All local, in-process — no infrastructure services.

## Component inventory

| Component | Type | Responsibility | Persists via / calls |
|---|---|---|---|
| **KnowledgeService** | Service (orchestrator) | upsert(+history+index), get/links/history, search, graph, dashboard, export | FactStore, HistoryTracker, SearchIndex |
| **InterviewService** | Service (orchestrator) | enqueue(+dedup+priority+TTL), list, answer, expire | QueueManager, AnswerIntake, KnowledgeService |
| **FactStore** | Component | read/write current fact documents | `EncryptedStore` ns=`facts` |
| **HistoryTracker** | Component | append-only change log; lazy read | `EncryptedStore` ns=`fact_history` |
| **SearchIndex** | Component (in-memory, derived) | inverted index; build-on-unlock, incremental, drop-on-lock | in-RAM (rebuildable from FactStore) |
| **QueueManager** | Component | pending queue CRUD, sort, expiry sweep, dedup | `EncryptedStore` ns=`queue` |
| **AnswerIntake** | Component | answer → confirmed fact + bounded follow-ups | KnowledgeService, `Masker`+`LlmClient` (U1) |
| **ExportRenderer** | Support | render facts → Markdown+JSON bundle (on-demand) | FactStore (read) |

## Dependencies on U1 (contracts, already available)
- `EncryptedStore` — all persistence (facts / fact_history / queue).
- `Masker` + `LlmClient` — used **only** by AnswerIntake for follow-up generation (mask-first, P-7).

## Integration diagram
```
                 Tauri commands (U1 CommandRouter, unlock-gated)
                        │                         │
             KnowledgeService            InterviewService
             ├─ FactStore ────────┐      ├─ QueueManager ───────┐
             ├─ HistoryTracker ───┤      ├─ AnswerIntake         │
             └─ SearchIndex (RAM) │      │     ├─ (calls) KnowledgeService.upsert
                                  │      │     └─ (calls) Masker → LlmClient  (U1)
                                  ▼      ▼
                          EncryptedStore (U1)  [ns: facts | fact_history | queue]
```
**Text alternative**: Two U3 services sit behind U1's Tauri command router (which gates on unlock). `KnowledgeService` orchestrates `FactStore`, `HistoryTracker`, and the in-memory `SearchIndex`. `InterviewService` orchestrates `QueueManager` and `AnswerIntake`; `AnswerIntake` calls back into `KnowledgeService.upsert` to persist confirmed facts and calls U1's `Masker`+`LlmClient` to derive follow-ups. All durable state goes through U1's `EncryptedStore` in three namespaces; `SearchIndex` is the only in-memory (derived, rebuildable) component.

## Data-flow notes
- **Confirmed fact = single write path**: whether from a Confirm answer, a Deepen answer, or (later) U2 processing, facts reach storage only through `KnowledgeService.upsert` (services.md principle).
- **No cyclic deps**: U3 → U1 only. `InterviewService` → `KnowledgeService` (one direction). `KnowledgeService` does not depend on `InterviewService` (dashboard reads the queue count via a read-only accessor / passed-in count, not a reverse dependency).
- **Derived vs durable**: durable = 3 encrypted namespaces; derived = `SearchIndex` (RAM). Recovery = rebuild index from `facts`.

## Absence of infrastructure components (intentional)
No queues/brokers, external caches, circuit breakers, load balancers, or databases beyond the local encrypted file store — the app is a single local process for one user (NFR-3, D8/D9).
