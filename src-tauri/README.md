# knows-me-core — U1 (Core Platform & Security)

This crate is **Unit U1**. It is a pure Rust library (no Tauri dependency) so it
builds and runs its full test suite on any toolchain. It provides:

**Shared contract layer** (Milestone 0 — unblocks U2/U3/U4 parallel work):

- `src/core/types.rs` — shared domain types (`Fact`, `QueueItem`, DTOs, `SourceKind`, …)
- `src/core/traits.rs` — service interfaces every unit codes against
- `src/core/error.rs` — unified `AppError` / `Result`
- `src/mocks.rs` — in-memory mocks (so U2/U3/U4 build & test today)

**U1 implementation:**

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
| `EncryptedStore`, `CredentialStore`, `KeyManager`, `Masker`, `LlmClient` | **U1 — implemented** |
| `Connector`, `IngestionApi`, `ProcessingApi` | U2 |
| `KnowledgeApi`, `InterviewApi` | U3 |
| `PersonaApi` | U4 |

**Parallel rule**: develop against the traits + mocks now; integrate in order
**U1 → U3 → U2 → U4** (see `../aidlc-docs/inception/application-design/unit-of-work-dependency.md`).

## Security note (judging criterion: maintainability)

Secrets are never hardcoded. External credentials go through `CredentialStore`
(encrypted); the encryption key is derived from the user's password at runtime
via Argon2id and held only in memory (zeroized on drop). See
`../aidlc-docs/inception/requirements/requirements.md` §FR-5.
