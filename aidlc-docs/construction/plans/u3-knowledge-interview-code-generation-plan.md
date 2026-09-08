# U3 (Knowledge & Interview) — Code Generation Plan

> **Stage**: CONSTRUCTION → Code Generation (per-unit, U3) — **Part 1: Planning** (single source of truth for generation)
> **Prereqs approved**: Functional Design, NFR Requirements, NFR Design.
> **This plan is executed step-by-step in Part 2 after approval. NO code is written until this plan is approved.**

## Unit context
- **Stories**: US-3.1 (fact doc + metadata), US-3.2 (history), US-3.3 (search), US-4.1 (queue items confirm/deepen), US-4.2 (no-context-switch answer), US-4.3 (priority/expiry).
- **Implements contracts** (fixed, Milestone 0): `KnowledgeApi`, `InterviewApi`.
- **Depends on U1** (via traits + mocks for parallel dev): `EncryptedStore`, `Masker`, `LlmClient`, shared types, `AppError`.
- **Provides to U2/U4**: real `KnowledgeApi` (write/read/search/graph) + `InterviewApi` (enqueue).

## Code location (greenfield modular monolith)
- **Application code** → `src-tauri/src/knowledge/` and `src-tauri/src/interview/` (Rust), registered in `src-tauri/src/lib.rs`.
- **Docs/summary** → `aidlc-docs/construction/u3-knowledge-interview/code/` (markdown only).
- **Frontend** (`src/features/queue/`): **deferred** to U1 app-shell integration per FD Q1=A — the design contract lives in `functional-design/frontend-components.md`. *(Override: say so if you want the `.tsx` authored now as ready-to-wire source.)*

## Target module layout
```
src-tauri/src/
  knowledge/
    mod.rs            # module docs + re-exports
    fact_store.rs     # FactStore: EncryptedStore ns="facts", JSON
    history.rs        # HistoryTracker: EncryptedStore ns="fact_history", append-only, lazy read
    search_index.rs   # SearchIndex (in-memory inverted index) + tokenizer (word split + CJK bigram)
    service.rs        # KnowledgeService: impl KnowledgeApi
  interview/
    mod.rs
    queue_manager.rs  # QueueManager: EncryptedStore ns="queue", sort, expire, dedup, priority scoring
    answer_intake.rs  # AnswerIntake: answer dispatch + follow-up generation (Masker+LlmClient)
    service.rs        # InterviewService: impl InterviewApi
```

---

## Generation Steps (each marked [x] as completed in Part 2)

- [ ] **Step 1 — Module scaffolding**: create `knowledge/mod.rs`, `interview/mod.rs`; add `pub mod knowledge; pub mod interview;` to `src-tauri/src/lib.rs` (additive; U1 gate-keeps shared file). *(US-3.x, US-4.x)*

- [ ] **Step 2 — Repository layer** (`fact_store.rs`, `history.rs`, `queue_manager.rs`):
  - `FactStore` over `EncryptedStore` ns=`facts`: `put/get/list/delete` with `serde_json` (Fact ↔ bytes). *(US-3.1)*
  - `HistoryTracker` over ns=`fact_history`: `append(id, change)`, `history(id)` (lazy read, ascending). *(US-3.2)*
  - `QueueManager` over ns=`queue`: `put/get/list/remove`, `expire(now)`, dedup suppression, priority scoring (`base + source + recency + need`, clamp 0–255). *(US-4.3)*

- [ ] **Step 3 — SearchIndex + tokenizer** (`search_index.rs`): in-memory `HashMap<Term, HashSet<FactId>>` + `HashMap<FactId, Scope>` + `HashMap<FactId, HashSet<Term>>` (for incremental removal); `build(facts)`, `upsert(fact)`, `remove(id)`, `search(query, filter)`. Tokenizer: lowercase word split for Latin/numeric + **CJK bigrams**; symmetric on index/query. *(US-3.3, NFR-4)*

- [ ] **Step 4 — KnowledgeService** (`service.rs`, `impl KnowledgeApi`): `upsert` (validate confirmed-only KR-1/KR-2 → append history if body changed KR-3 → put → index update, write-order per P-5, no lock across `.await`), `get`, `links`, `history`, `search`, `graph` (edges within node set KR-5), `dashboard` (fact count + `EncryptedStore.list("queue").len()` for pending — no service cycle + recent-5), `build_index()` for unlock. *(US-3.1, 3.2, 3.3)*

- [ ] **Step 5 — InterviewService + AnswerIntake** (`service.rs`, `answer_intake.rs`, `impl InterviewApi`): `enqueue` (validate + dedup + priority + TTL: Confirm 14d/Deepen 30d), `list` (PriorityDesc/NewestFirst), `answer` (dispatch per FD Q9: Confirm+affirm→upsert fact / Confirm+reject→discard / Deepen+text→fact + `derive_follow_ups` / any+Skip→leave pending), `expire`. `derive_follow_ups`: `Masker.mask` → `LlmClient.classify/summarize` → up to 3 Deepen items; **graceful degradation** (LLM error → skip follow-ups, keep fact, log — P-4). *(US-4.1, 4.2, 4.3)*

- [ ] **Step 6 — Example-based unit tests** (in-module `#[cfg(test)]`): confirmed-only rejection; upsert+history growth; no-op upsert (no history); search keyword+scope filter incl. Korean bigram; graph edges; queue priority sort + newest sort; TTL expire; answer dispatch (confirm affirm/reject, deepen→fact+follow-ups, skip leaves pending); **offline degradation** (failing-LLM double → fact saved, follow_ups empty). Uses U1 mocks (`InMemoryStore`, `NoopMasker`, `CannedLlm`) + a local `FailingLlm`. *(all US, PBT-10 complementary)*

- [ ] **Step 7 — Property-based tests (proptest)** + generators: strategies for `Fact`/`FactChange`/`QueueItem` (valid metadata, non-empty title, bounded priority/TTL — PBT-07). Properties: P1 JSON round-trip (PBT-02); P2 history +1 on body change, P3 no-op upsert no history, P4 search results satisfy filter+query, P5 list ordering monotonic, P6 expire removes all expired, P7 graph edges within node set (PBT-03). Shrinking on; seed reproducibility (PBT-08). *(NFR-8)*

- [ ] **Step 8 — Code summary docs**: `aidlc-docs/construction/u3-knowledge-interview/code/summary.md` (files created, story coverage, how to run tests) + module-level `//!` docs.

- [ ] **Step 9 — Verify** (`src-tauri/`): `cargo fmt`, `cargo build`, `cargo test` (existing 5 + new), `cargo clippy --all-targets -- -D warnings`. All must be green before completion. *(full Build & Test is the later ALWAYS stage; this is per-unit verification.)*

---

## Story traceability
| Story | Steps |
|---|---|
| US-3.1 사실 저장+메타 | 2 (FactStore), 4 (upsert), 6/7 (tests) |
| US-3.2 이력 | 2 (HistoryTracker), 4, 6/7 |
| US-3.3 검색 | 3 (SearchIndex), 4 (search), 6/7 |
| US-4.1 큐 생성(확인/심화+후속) | 5 (enqueue, derive_follow_ups), 6/7 |
| US-4.2 전환 없는 답변 | 5 (answer dispatch), 6 (skip leaves pending); UI = frontend-components.md (deferred) |
| US-4.3 우선순위·만료 | 5 (priority/TTL/dedup), 6/7 |

## PBT plan (enforcement per property-based-testing.md)
- **PBT-01**: properties identified (FD business-rules P1–P7) → carried into Step 7. ✅
- **PBT-02/03**: Step 7 round-trip + invariants. **PBT-07**: domain generators. **PBT-08**: shrinking + seed. **PBT-09**: proptest (added). **PBT-10**: Step 6 example tests complement PBT.
- **N/A / advisory**: PBT-04 (idempotence — P3 covered as invariant), PBT-05 (oracle — mocks serve as reference), PBT-06 (stateful — advisory, not in Partial set).

## Dependencies / assumptions
- Async trait impls (`#[async_trait]`); std `Mutex`/`RwLock` guards never held across `.await`.
- Real crypto/masking/LLM come from U1; U3 is developed & tested against U1 mocks (integration order U1→U3).
- No new crates beyond `proptest` (already added).
