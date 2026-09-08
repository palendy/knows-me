# U3 — Tech Stack Decisions (Knowledge & Interview)

> Confirmed technology choices for U3 with rationale. U3 is a set of Rust modules inside the existing `knows_me_core` crate (`src-tauri/`) plus a React/TS Queue feature (deferred to U1 app shell). Decisions per approved NFR answers (all A).

## 1. Decisions

| Area | Decision | Rationale | Trace |
|---|---|---|---|
| **Language / core** | Rust (edition 2021), inside `knows_me_core` crate `src-tauri/` | Matches AD2 (Rust core) + Milestone 0 crate; modular monolith module boundary = unit boundary | AD2, unit-of-work |
| **Async** | `async_trait` for the contract impls; runtime = **tokio** provided by the Tauri app shell at runtime (dev-dependency for tests). U3 adds **no** new runtime dependency | Contracts (`KnowledgeApi`/`InterviewApi`) are already `async`; runtime ownership belongs to U1 shell | traits.rs |
| **Serialization** | `serde` + `serde_json` (already deps); **JSON** as canonical fact/history/queue form | FD Q4=A; stable round-trip for PBT-02; machine-friendly | FD Q4, NFR-8 |
| **Persistence** | U1 `EncryptedStore` only; namespaces `facts` / `fact_history` / `queue`. **No new persistence crate** | Encryption-at-rest gate owned by U1 (Q3=A); keeps U3 storage-agnostic | FD Q3, NFR-2 |
| **Search index** | In-memory inverted index using std collections (`HashMap<Term, HashSet<FactId>>` + `HashMap<FactId, Scope>`); rebuilt on unlock, incremental on upsert. **No external search engine** | NFR-4 target (50k, p95<100ms) met without tantivy's complexity | NFR-4, Q1 |
| **Tokenizer (KO/EN)** | Std char-class tokenization: split on non-alphanumeric + lowercase for Latin/numeric; **CJK runs indexed as bigrams (2-grams)** for Korean substring search. Implemented as a small reusable helper; **no new dependency** for MVP (`unicode-segmentation` noted as an easy future upgrade) | Q2=A — practical KO+EN search with minimal deps | NFR-4, Q2 |
| **Offline degradation** | LLM calls guarded; on error → save fact, skip follow-ups, log (Q3=A). No retry queue in MVP | NFR-3 graceful degradation | NFR-3, Q3 |
| **Concurrency** | Serialized writes; store+index guarded by a lock; index self-heals from store on unlock | Q5=A — single-user, minimal contention | Q5 |
| **PBT framework** | **proptest 1.11** (`src-tauri/Cargo.toml` `[dev-dependencies]`) — **PBT-09 record** | Rust-idiomatic, macro strategies, shrinking, seed reproducibility, integrates with `cargo test` | NFR-8, PBT-09 |
| **IDs / time** | `uuid` (v4) + `chrono` (Utc) — already deps | Shared types use them | types.rs |
| **Errors** | Shared `AppError` / `Result<T>` | Unified across units | error.rs |
| **Export format** | Markdown (frontmatter + body) + JSON bundle, on-demand | NFR-6 portability | NFR-6, Q4 |
| **Frontend (deferred)** | React + TypeScript + Vite (AD1) via U1 app shell; UI test tooling (Vitest / fast-check) decided at frontend integration | Q6=A defers FE test tooling; Q1(FD)=A defers scaffolding to U1 | AD1, FD Q1, Q6 |

## 2. Dependency delta introduced by U3
- **Already present** (no change): `serde`, `serde_json`, `async-trait`, `uuid`, `chrono`, `thiserror`; `tokio` (dev).
- **Added in this stage**: `proptest` (dev) — done during NFR pre-reflection (verified: 5/5 lib tests still pass).
- **Not added** (deliberately, per rationale above): no `tantivy`, no `unicode-segmentation` (MVP), no new async runtime.

## 3. PBT-09 Compliance (framework selection)
- Framework selected & documented: **proptest 1.11**. ✅
- Included as project dependency (`src-tauri/Cargo.toml` dev-dependency). ✅
- Supports custom strategies (domain generators), automatic shrinking, seed-based reproducibility, `cargo test` integration. ✅
- Single primary language (Rust) for U3 backend PBT; frontend PBT (if any) deferred with the FE stack. ✅

**No blocking PBT findings at NFR Requirements.**
