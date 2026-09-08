# U3 — Domain Entities (Knowledge & Interview)

> Functional Design, technology-agnostic. Entities reuse the **shared types** fixed in Milestone 0 (`src-tauri/src/core/types.rs`). This document defines U3's *persisted records*, their storage layout, and relationships. Design decisions per approved questions: JSON serialization (Q4=A), encrypted-store-as-canonical (Q3=A), explicit links only (Q5=A), full-body history snapshots (Q6=A), in-memory index (Q7=A).

## 1. Entity Catalog

| Entity | Shared type | Role in U3 | Mutability |
|---|---|---|---|
| **Fact** | `Fact` | A confirmed unit of context = one wiki document. Current value. | Upsert (with history) |
| **FactMetadata** | `FactMetadata` | Provenance + confirmed + scope + confirmed_at. | Set on write |
| **Provenance** | `Provenance` | source + collected_at. | Immutable once set |
| **FactChange** | `FactChange` | One history entry (before/after body snapshot). | Append-only |
| **FactHistory** | `Vec<FactChange>` | Chronological (ascending) change log for one `FactId`. | Append-only |
| **SearchIndex** | *(new, in-memory)* | Inverted index term → set of `FactId`, plus per-fact scope. Rebuilt on unlock; not persisted. | Rebuilt / incremental |
| **QueueItem** | `QueueItem` | A pending interview item (Confirm or Deepen) + priority + created/expires. | Insert / remove |
| **QueueItemKind** | `QueueItemKind` | `Confirm { candidate }` or `Deepen { question, hypothesis }`. | — |
| **FactCandidate** | `FactCandidate` | Unconfirmed candidate carried inside a `Confirm` item. | — |
| **AnswerInput / AnswerResult** | `AnswerInput` / `AnswerResult` | Owner's answer; result = optional confirmed fact + follow-ups. | — |
| **GraphNode / GraphEdge / GraphDto** | shared | Derived projection of facts (nodes) and `Fact.links` (edges). | Derived (read) |

**No new persisted shared-type changes required.** `SearchIndex` is the only new (in-memory-only) structure U3 introduces.

## 2. Storage Layout (via U1 `EncryptedStore`, JSON bytes)

`EncryptedStore` is the canonical store (Q3=A). All values are `serde_json`-serialized then encrypted by U1's implementation. Namespaces:

| Namespace (`ns`) | Key | Value (JSON) | Notes |
|---|---|---|---|
| `facts` | `FactId` (uuid string) | `Fact` | Canonical current value of each fact. |
| `fact_history` | `FactId` | `Vec<FactChange>` | Append-only; ascending by `changed_at`. |
| `queue` | `QueueItemId` (uuid string) | `QueueItem` | Pending interview items; removed on answer/expire. |

- **Current value is identifiable** (US-3.2/AC1): the record in `facts` is always the latest; prior bodies live only in `fact_history`.
- **Human-readable wiki** (US-3.1): provided by an in-app viewer and an explicit *export* action that renders a `Fact` to plaintext Markdown on demand — the at-rest form stays encrypted (satisfies US-7.1). No plaintext file is written implicitly.
- **SearchIndex is never persisted**: built by listing `ns=facts` on unlock and updated incrementally on each `upsert`; dropped on lock.

## 3. Relationships

```
Fact (1) ──< links >──> Fact (0..*)        # explicit wiki edges (Q5=A); graph() derives from these
Fact (1) ──< history >── FactChange (0..*)  # append-only snapshots
QueueItem.Confirm ──contains──> FactCandidate ──answer(affirm)──> Fact   # single upsert path
QueueItem.Deepen  ──answer(text)──> Fact  (+)──derive──> QueueItem.Deepen (follow-ups, bounded)
SearchIndex ──indexes──> Fact              # in-memory term -> {FactId}
```

**Text alternative**: A `Fact` may link to zero-or-more other facts (edges of the knowledge graph, derived only from stored `links`). Each `Fact` has zero-or-more `FactChange` history entries. A `Confirm` queue item contains a `FactCandidate` that, when affirmed, becomes a `Fact`. A `Deepen` queue item, when answered with text, produces a `Fact` and may spawn bounded follow-up `Deepen` items. The `SearchIndex` maps terms to the set of facts containing them.

## 4. Invariants at the entity level
- A `Fact` persisted in `ns=facts` MUST have `metadata.confirmed == true` (US-3.1/AC3).
- `fact_history[id]` is ordered ascending by `changed_at`; entries are never mutated or deleted.
- A `GraphEdge (from,to)` is emitted only when both `from` and `to` are present in the (filtered) node set — dangling links are skipped, not errors.
- A `QueueItem` in `ns=queue` is either pending or absent; there is no "answered-but-kept" state (answered items are removed; `Skip` leaves the item pending).
