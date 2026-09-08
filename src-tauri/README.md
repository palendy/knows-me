# knows-me core (U1) — Milestone 0

This crate is **Unit U1 (Core Platform & Security)**. Milestone 0 delivers the
**shared contract layer** so the other units can be developed in parallel:

- `src/core/types.rs` — shared domain types (`Fact`, `QueueItem`, DTOs, `SourceKind`, …)
- `src/core/traits.rs` — service interfaces every unit codes against
- `src/core/error.rs` — unified `AppError` / `Result`
- `src/mocks.rs` — in-memory mock implementations (so U2/U3/U4 build & test today)

The frontend mirror of these types lives in `../src/shared/contracts.ts`.

## Build & test

Requires the Rust toolchain (stable). From `src-tauri/`:

```bash
cargo build          # compile the contract crate
cargo test           # run the contract/mock smoke tests
cargo run            # prints the Milestone 0 banner
```

> The Tauri application (windows, `#[tauri::command]` handlers, real service
> wiring) and the real implementations (crypto Vault, file-based FactStore,
> connectors, LLM client) are added in **U1 full implementation** and the
> per-unit CONSTRUCTION stages.

## Contract ownership (who implements what)

| Trait(s) | Owner unit |
|---|---|
| `EncryptedStore`, `CredentialStore`, `KeyManager`, `Masker`, `LlmClient` | U1 |
| `Connector`, `IngestionApi`, `ProcessingApi` | U2 |
| `KnowledgeApi`, `InterviewApi` | U3 |
| `PersonaApi` | U4 |

**Parallel rule**: develop against the traits + mocks now; integrate in order
**U1 → U3 → U2 → U4** (see `../aidlc-docs/inception/application-design/unit-of-work-dependency.md`).

## Security note (judging criterion: maintainability)

Secrets are never hardcoded. External credentials go through `CredentialStore`
(encrypted), and the encryption key is derived from the user's password at
runtime — see `../aidlc-docs/inception/requirements/requirements.md` §3.5.
