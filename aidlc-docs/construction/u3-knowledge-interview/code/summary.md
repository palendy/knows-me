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
- **Property-based (proptest)**: P1 Fact/QueueItem JSON round-trip (PBT-02); P4 search results satisfy filter+query; P5 priority & newest sort monotonic; P6 no expired after retain; P7 graph edges within nodes; priority range invariant (PBT-03/07/08).

## How to run
```
cd src-tauri
cargo test        # 26 pass
cargo clippy --all-targets -- -D warnings   # clean
cargo fmt --check
```

## Known coordination item (for U1 / Dev A)
- Interview-answer-derived facts have no natural `SourceKind`. `fact_from_deepen` uses `SourceKind::Session` as a **documented placeholder** (`// TODO(U1): SourceKind::Interview`). Recommend adding a `SourceKind::Interview` variant to the shared contract during integration.

## Notes / deviations
- Priority scoring omits a time-decay term (recency handled by `NewestFirst` sort + TTL expiry instead) — simpler and equivalent for ranking at creation time.
- Follow-up items are `put` directly (bypassing enqueue dedup) since freshly generated items rarely collide.
- Frontend `src/features/queue/` implementation deferred to U1 app-shell integration (FD Q1=A).
