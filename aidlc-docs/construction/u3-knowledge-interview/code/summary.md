# U3 — Code Generation Summary (Knowledge & Interview)

> Markdown summary of generated code (the code itself lives under `src-tauri/src/`). Generated per the approved plan `plans/u3-knowledge-interview-code-generation-plan.md`.

## Files created (Rust, in `knows_me_core` crate)
| File | Contents |
|---|---|
| `src-tauri/src/knowledge/mod.rs` | module docs + re-exports |
| `src-tauri/src/knowledge/fact_store.rs` | `FactStore` — encrypted JSON facts (`ns=facts`), load_all for index rebuild |
| `src-tauri/src/knowledge/history.rs` | `HistoryTracker` — append-only change log (`ns=fact_history`), lazy read |
| `src-tauri/src/knowledge/search_index.rs` | `SearchIndex` — in-memory inverted index + metadata cache + `tokenize` (Latin words + CJK bigrams) |
| `src-tauri/src/knowledge/service.rs` | `KnowledgeService` impl `KnowledgeApi` + tests (examples + proptest P1/P4/P7) |
| `src-tauri/src/interview/mod.rs` | module docs + re-exports |
| `src-tauri/src/interview/queue_manager.rs` | `QueueManager` (`ns=queue`) + pure helpers: `score_priority`, `ttl_days`, `dedup_key`, `sort_queue`, `is_expired` |
| `src-tauri/src/interview/answer_intake.rs` | `is_affirmative`, `fact_from_candidate`, `fact_from_deepen`, `derive_follow_ups` (mask→LLM, graceful) |
| `src-tauri/src/interview/service.rs` | `InterviewService` impl `InterviewApi` + tests (examples + proptest P1/P5/P6 + range) |

## Files modified
| File | Change |
|---|---|
| `src-tauri/src/lib.rs` | added `pub mod knowledge; pub mod interview;` (additive) |
| `src-tauri/Cargo.toml` | `proptest` dev-dependency (added in NFR pre-reflection) |

## Story coverage
- **US-3.1** fact + metadata → `FactStore`, `KnowledgeService::upsert` (confirmed-only, KR-1/2)
- **US-3.2** history → `HistoryTracker`, upsert appends on body change (KR-3/4)
- **US-3.3** search → `SearchIndex` (keyword + scope, Korean via bigram; NFR-4 in-memory)
- **US-4.1** queue items + follow-ups → `InterviewService::enqueue`, `AnswerIntake::derive_follow_ups`
- **US-4.2** no-context-switch answer semantics → `answer` dispatch; Skip leaves pending; UI design in `functional-design/frontend-components.md` (impl deferred to U1 shell)
- **US-4.3** priority/expiry → `score_priority`, TTL, `expire`, dedup

## Tests (26 total: 21 new + 5 pre-existing)
- **Example-based**: confirmed-only rejection, empty-title rejection, upsert/get/dashboard, history growth + no-op-no-history, Korean+scope search, graph edges within nodes; confirm affirm→fact, confirm reject→no fact, deepen→fact+follow-ups, skip leaves pending, **offline LLM degradation (fact saved, no follow-ups)**, dedup, expire.
- **Property-based (proptest)**: P1 Fact/QueueItem JSON round-trip (PBT-02); P4 search results satisfy filter+query; P5 priority & newest sort monotonic; **P6 `expire()` leaves no expired item in the real queue** (invokes the service, not a local filter); P7 graph edges within nodes; priority range invariant (PBT-03/07/08).

## How to run
```
cd src-tauri
cargo test        # 32 pass
cargo clippy --all-targets -- -D warnings   # clean
cargo fmt --check
```

## Code-review fixes applied (xhigh review)
All 🔴 (4) + 🟡 (6) findings from the code review were fixed in this iteration:
- **Lazy index rebuild (P-1)**: `KnowledgeService` now rebuilds the in-memory index from the store on first use (`ensure_index`), plus a public `build_index()`. Fixes "knowledge base appears empty after restart" (search/graph/dashboard were returning empty until an upsert).
- **Serialized writes (P-6)**: an async `write_lock` (tokio `sync`) is held across `upsert`'s read-modify-write, so concurrent upserts can't lose a history entry.
- **Metadata gate (KR-2)**: `upsert` now also rejects `confirmed_at == None`.
- **Empty-answer guard**: blank `Choice`/`Text` answers are rejected (`InvalidInput`) and the item stays pending — no accidental knowledge / discard. `is_affirmative("")` is now `false`.
- **Follow-ups via `enqueue`**: derived deepen items go through `enqueue` (dedup/priority/TTL apply), no longer `put` directly.
- **`dedup_key` per-kind**: `confirm:`/`deepen:` prefix so a Confirm title and a Deepen question with the same text no longer collide.
- **Search token semantics**: a non-empty query that tokenizes to nothing (e.g. `"???"`) returns empty, not the whole DB; only a truly empty query returns all.
- **`expire()`**: also runs dedup suppression; `list()` filters out expired items so the UI never shows them pre-sweep.
- **`enqueue` dedup**: decides against the highest-priority duplicate before mutating, so it can't drop both the existing and the incoming item.
- **P6 property test**: rewritten to actually call `InterviewService::expire()`; `prop_score_priority_in_range` now asserts a real band.

## Known / deferred (tracked, not fixed here)
- **`SourceKind::Interview` (U1 coordination)**: interview-answer facts still use `SourceKind::Session` placeholder pending a shared-contract addition (Dev A).
- **Single CJK-character search**: the bigram index can't match a 1-char Korean query (needs ≥2 chars); known limitation, low value — optional CJK-unigram indexing later.
- **Export renderer (P-8 / NFR-6)**: encrypted-store → human-readable Markdown/JSON export not yet implemented (was out of the code-gen plan scope); tracked as a follow-up.

## Notes / deviations
- Priority scoring omits a time-decay term (recency handled by `NewestFirst` sort + TTL expiry) — simpler and equivalent at creation time.
- Added `tokio` (`default-features = false, features = ["sync"]`) to `[dependencies]` for the async write lock — sync primitives only, executor still from the U1 app shell.
- Frontend `src/features/queue/` implementation deferred to U1 app-shell integration (FD Q1=A).
