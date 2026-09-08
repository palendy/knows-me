# U3 — Business Rules & Testable Properties (Knowledge & Interview)

> Decision rules, validation, constraints, and invariants for U3. Traceability to stories/AC in brackets. Rule IDs: **KR-** (Knowledge), **IR-** (Interview).

## 1. Knowledge Rules

| ID | Rule | Trace |
|---|---|---|
| KR-1 | Only **confirmed** facts persist. `upsert` rejects a fact with `metadata.confirmed == false` (`InvalidInput`). | US-3.1/AC3 |
| KR-2 | Every stored fact carries complete metadata: `provenance{source, collected_at}`, `confirmed`, `scope`, and `confirmed_at` (set when confirmed). `title` must be non-empty; `body` may be empty. | US-3.1/AC2 |
| KR-3 | History is **never overwritten**. When an existing fact's `body` changes, a `FactChange{before, after}` snapshot is appended before the new value is written. | US-3.2/AC1 |
| KR-4 | `history(id)` returns changes in **ascending chronological** order; the current value is always the record in `ns=facts`. | US-3.2/AC1,AC2 |
| KR-5 | Links are **explicit** (`Fact.links`). `graph()` emits an edge only when both endpoints are in the (filtered) node set; dangling links are skipped, not errors. | US-5.3/AC1, Q5 |
| KR-6 | `search` filters are exact on `scope`; empty query returns all (filtered) facts. Search uses the in-memory inverted index for responsiveness at 10⁴ facts. | US-3.3/AC1,AC2, NFR-4 |
| KR-7 | Any external LLM call originating in U3 (follow-up generation) MUST pass through `Masker` first — raw text never leaves the device. | US-2.2, services.md |
| KR-8 | All persistence flows through `EncryptedStore`; the at-rest form is encrypted. Plaintext Markdown is produced only by an explicit user *export* action. | US-7.1, US-3.1 |

## 2. Interview Rules

| ID | Rule | Trace |
|---|---|---|
| IR-1 | **Confirm + affirm** (Choice affirmative / Text) → candidate becomes a confirmed fact (single `upsert` path). **Confirm + reject** → candidate discarded, no fact. | US-4.2/AC3, Q9 |
| IR-2 | **Deepen + text** → a confirmed fact is created from the answer, then up to **3** follow-up `Deepen` items are derived via LLM. | US-4.1/AC3, Q8 |
| IR-3 | **Any + Skip** leaves the item pending — the owner is never forced to answer. | US-4.2/AC2 |
| IR-4 | Queue is ordered by `priority` (0–255, weighted by source/recency/need) for display; duplicate pending items (same candidate title / question) are suppressed to the highest-priority one. | US-4.3/AC1 |
| IR-5 | Items expire by TTL from `created_at` — Confirm 14 days, Deepen 30 days; `expire()` removes all items past due. | US-4.3/AC2 |
| IR-6 | Follow-up chains are bounded (per-answer cap of 3 + dedup) so the queue cannot grow unboundedly. | US-4.1/AC3 |

## 3. Preconditions / cross-cutting
- **Unlocked required**: all Knowledge/Interview operations assume the vault is unlocked (U1 `CommandRouter`/`AppState` gate rejects otherwise). U3 does not re-implement the gate; it operates on the `EncryptedStore` handed to it.
- **Idempotent re-write**: re-`upsert`ing an unchanged fact appends no history entry.

---

## 4. Testable Properties (PBT-01)

PBT extension is **Partial** — enforced (blocking): PBT-02, PBT-03, PBT-07, PBT-08, PBT-09; others advisory. Framework = **proptest** (Q2=A, added to `src-tauri/Cargo.toml` dev-dependencies).

| # | Property | Category | Rule(s) | Enforced |
|---|---|---|---|---|
| P1 | `deserialize(serialize(x)) == x` for `Fact`, `FactChange`, `QueueItem`, `AnswerResult` (JSON) | Round-trip | PBT-02 | ✅ blocking |
| P2 | After `upsert` of an existing fact with a changed body: `history.len` grows by exactly 1 and `history.last.before == old.body` | Invariant | KR-3 / PBT-03 | ✅ blocking |
| P3 | Re-`upsert` of an unchanged fact appends no history entry | Invariant/Idempotence | KR-3 | ✅ blocking (invariant) |
| P4 | Every `search(q, filter)` result satisfies `filter.scope`, and (q non-empty ⇒ each result's title/body contains a query token) | Invariant | KR-6 / PBT-03 | ✅ blocking |
| P5 | `list(PriorityDesc)` is non-increasing in `priority`; `list(NewestFirst)` non-increasing in `created_at` | Invariant | IR-4 / PBT-03 | ✅ blocking |
| P6 | After `expire()`, no remaining item has `expires_at <= now` | Invariant | IR-5 / PBT-03 | ✅ blocking |
| P7 | `graph()` emits no edge whose endpoint is outside the node set | Invariant | KR-5 / PBT-03 | ✅ blocking |
| P8 | `expire(expire(state)) == expire(state)` (no item removed twice) | Idempotence | IR-5 / PBT-04 | advisory |
| P9 | Stateful sequence testing of QueueManager/FactStore vs. a simple model | Stateful | PBT-06 | advisory |

**Generators (PBT-07)**: domain generators for `Fact` (valid metadata, non-empty title, realistic scope/source), `QueueItem` (both kinds, bounded priority/TTL), and `FactChange`. Centralized as reusable test utilities. No raw-primitive-only generators for domain-typed params.

**Shrinking & reproducibility (PBT-08)**: proptest shrinking left enabled; seed logged on failure; PBT included in CI (detailed in Build & Test).

**Framework (PBT-09)**: proptest selected & added as dependency; supports custom strategies, shrinking, seed reproducibility, integrates with `cargo test`.

**Complementary (PBT-10, advisory)**: the existing example-based smoke tests in `mocks.rs` (history-preserving upsert, confirm→fact) are retained; PBT complements them.

### PBT Compliance Summary (this stage — Functional Design / PBT-01)
| Rule | Status | Notes |
|---|---|---|
| PBT-01 | **Compliant** | Properties identified per component above with categories. |
| PBT-02/03/07/08/09 | Deferred to their stages | Carried forward as code-gen/test requirements; no violation at design stage. |
| PBT-04/05/06/10 | Advisory | P8/P9 noted; oracle (PBT-05) → mocks can serve as reference in tests. |

No blocking PBT findings at Functional Design.
