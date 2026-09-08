# U3 — NFR Requirements (Knowledge & Interview)

> Non-functional requirements for U3, with measurable targets and traceability. Decisions per approved answers (all A). Infrastructure Design is SKIPPED (local desktop app).

## 1. Performance & Scale — NFR-4 (U3-critical)

| Requirement | Target | Approach | Trace |
|---|---|---|---|
| Fact corpus size | up to **50,000** fact documents (single user) | in-memory inverted index | NFR-4, Q1 |
| `search` latency | **p95 < 100 ms** at 50k facts | AND-intersect posting lists on in-memory index | US-3.3/AC1, NFR-4 |
| `get(id)` latency | < 10 ms | direct key read + JSON deserialize | US-3.1 |
| `list_queue` latency | < 50 ms for ≤ 1,000 pending items | in-memory sort | US-4.3/AC1 |
| Index build on unlock | < 2 s for 50k facts (may run async/lazy) | list `ns=facts` → tokenize | NFR-4 |
| Index update on `upsert` | O(tokens in fact); incremental | remove old postings, add new | NFR-4 |

**Memory budget**: index is `term → set<FactId>` in RAM; estimated well within desktop limits at 50k facts. Acceptable trade-off (no external search engine — Q1=A).

## 2. Privacy — NFR-2
- U3 transmits to the cloud LLM **only** during follow-up generation (`AnswerIntake.derive_follow_ups`). It MUST call `Masker.mask` first (business-rule **KR-7**); raw text never leaves the device.
- U3 MUST NOT write plaintext facts anywhere except through `EncryptedStore` (**KR-8**). Plaintext is produced only by explicit user export (§6).
- **Transparency**: U3's single LLM path uses the shared `LlmClient`, whose calls are recorded centrally (TransferLog, owned by U1/U2). U3 depends on that shared gateway rather than re-implementing logging.

## 3. Local-first / Offline — NFR-3
- **Fully offline** (no network): `upsert`, `get`, `links`, `history`, `search`, `graph`, `dashboard`; `enqueue`, `list`, `expire`; and `answer` **fact persistence**.
- **Network-dependent**: only Deepen follow-up question generation (LLM). **Graceful degradation (Q3=A)**: on `LlmClient` error/offline, the answer's fact is still saved and `follow_ups = []`, with a logged note; nothing fails or is lost. No retry queue in MVP.

## 4. Idempotency & Consistency — NFR-5, reliability
- Re-`upsert` of an unchanged fact appends **no** history entry (FD KR-3 / property P3).
- Duplicate pending queue items (same candidate title / question) are suppressed to the highest-priority one (IR-4).
- **Write model (Q5=A)**: writes are serialized per command; `upsert` order = append history (if body changed) → write fact → update index. The index is **rebuildable** from `ns=facts`, so if it ever drifts it is reconstructed on next unlock (self-healing). Store/LLM failures surface as `AppError` variants.

## 5. Testability — NFR-8 (PBT Partial: PBT-02/03/07/08/09 enforced)
- Enforced properties for U3 (from FD business-rules P1–P7): Fact/FactChange/QueueItem **JSON round-trip** (PBT-02); history-growth, search-filter-correctness, queue-ordering, expiry, graph-edge invariants (PBT-03).
- Domain generators for `Fact`/`QueueItem`/`FactChange` (PBT-07); shrinking + seed logging in CI (PBT-08); framework = **proptest** (PBT-09, §tech-stack).
- Example-based smoke tests retained alongside PBT (PBT-10, advisory).

## 6. Portability / Backup — NFR-6
- At-rest form is encrypted (canonical). **Export command (Q4=A)** renders facts to a human-readable **Markdown + JSON** bundle while unlocked, for backup / manual editing / portability. Import is deferred (out of MVP scope).
- Markdown export shape: title as heading, metadata as frontmatter, body as content, links listed — matches the "LLM 위키" intent (FR-3.1, NFR-6).

## 7. Usability — FR-4.3 / US-4.2
- Queue answering is **no-context-switch** (inline), never forces an answer (Skip keeps item pending), and surfaces derived follow-ups immediately (FD frontend-components).
- Loading/empty/busy states defined; stable `data-testid` for automation.

## 8. Availability / DR
- Single-user local desktop: availability = the app process running (no server SLA). Disaster recovery = user's own encrypted backups + export bundle (§6). No HA/failover in scope.

## 9. Out of scope for U3 NFR
- Multi-user access control, remote exposure, authentication (D7/D9 — future).
- Cloud infrastructure / deployment NFRs (Infrastructure Design skipped).

## 10. Traceability summary
| NFR | U3 coverage |
|---|---|
| NFR-2 | §2 (mask-before-LLM, no plaintext, transparency) |
| NFR-3 | §3 (offline ops + graceful LLM degradation) |
| NFR-4 | §1 (scale + latency targets, in-memory index) |
| NFR-5 | §4 (idempotent upsert, dedup) |
| NFR-6 | §6 (encrypted canonical + export) |
| NFR-8 | §5 (PBT Partial properties + proptest) |
