# knows-me-core — Rust core (all units)

This crate is the `knows-me-core` library. It is a pure Rust library (no Tauri
dependency) so it builds and runs its full test suite on any toolchain. It began
as **Unit U1** and now hosts every unit's logic behind the shared contracts; the
Tauri desktop shell in `../desktop/` only bridges it to the frontend.

Module map:

- `src/core/` — U1: shared domain types (`Fact`, `QueueItem`, DTOs, `SourceKind`, …),
  service traits, unified `AppError` / `Result`, `AppState`, scheduler, commands
- `src/security/`, `src/llm/` — U1: crypto + LLM gateway (below)
- `src/ingestion/`, `src/processing/` — U2: source connectors + summarize/classify
- `src/knowledge/`, `src/interview/` — U3: fact store/history/search + interview queue
- `src/persona/` — U4: read-view queries, persona chat, local REST API
- `src/mocks.rs` — in-memory mocks (from Milestone 0, still used by unit tests)

**U1 implementation (crypto + LLM gateway):**

- `src/security/` — `PasswordKeyManager` (Argon2id KDF, lock/unlock, in-memory
  key), `vault` (AES-256-GCM), `FileEncryptedStore` (encrypted-at-rest KV +
  credential store). Plaintext keys never touch disk (US-7.1–7.3).
- `src/llm/` — `RegexMasker` (identifier masking + local restore, US-2.2),
  prompt builders, `TransferLog` (egress transparency, NFR-2), and the real
  Anthropic `LlmClient` behind the `llm-http` feature.
- `src/core/app_state.rs`, `scheduler.rs`, `commands.rs` — platform wiring, the
  periodic batch runner, and the front-end-facing command layer.

The frontend mirror of the shared types lives in `../src/shared/contracts.ts`.
The Tauri desktop shell lives in `../desktop/`.

## Build & test

Requires the Rust toolchain (stable). From `src-tauri/`:

```bash
cargo build                          # compile the library + demo binary
cargo test                           # unit + property tests (proptest)
cargo run                            # headless U1 demo (onboarding→unlock→mask)
cargo clippy --all-targets -- -D warnings
cargo build --features llm-http      # + real Anthropic cloud client (needs a C toolchain)
```

The real cloud client (`llm-http`) reads `ANTHROPIC_API_KEY` (and optional
`ANTHROPIC_MODEL`, default `claude-opus-5`) from the environment — no secrets in
code. It is off by default so the core builds/tests fully offline.

## Property-based tests (NFR-8, Partial: PBT-02/03/07/08/09)

Framework: **proptest** (PBT-09). Covered:

- **Round-trip (PBT-02)** — `Masker` mask↔unmask, `Vault` encrypt↔decrypt.
- **Invariant (PBT-03)** — masking completeness: re-scanning masked output finds
  no identifier (the R1 privacy property).
- **Generators (PBT-07)** — domain generators produce realistic prose + real
  identifiers (emails/tokens/phones).
- **Shrinking + seeds (PBT-08)** — proptest shrinks failures and prints the seed;
  any regression seed is saved under `proptest-regressions/` (commit it).

## Contract ownership (who implements what)

| Trait(s) | Owner unit |
|---|---|
| `EncryptedStore`, `CredentialStore`, `KeyManager`, `Masker`, `LlmClient` | U1 |
| `Connector`, `IngestionApi`, `ProcessingApi` | U2 |
| `KnowledgeApi`, `InterviewApi` | U3 |
| `PersonaApi` | U4 |

All four units are implemented and integrated (development ran in parallel
against the traits + mocks, integrated in order **U1 → U3 → U2 → U4**; see
`../aidlc-docs/inception/application-design/unit-of-work-dependency.md`).

## Security note (judging criterion: maintainability)

Secrets are never hardcoded. External credentials go through `CredentialStore`
(encrypted); the encryption key is derived from the user's password at runtime
via Argon2id and held only in memory (zeroized on drop). See
`../aidlc-docs/inception/requirements/requirements.md` §FR-5.
