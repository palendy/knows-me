# AI-DLC State Tracking

## Project Information
- **Project Name**: knows-me
- **Project Type**: Greenfield
- **Start Date**: 2026-09-08T06:46:46Z
- **Current Stage**: CONSTRUCTION - U1 (Core Platform & Security) FULLY implemented on branch `feat/u1-core-platform-security` (autonomous, per user directive); U2/U3/U4 pending

## Construction Notes
- **U1 full implementation done** (branch `feat/u1-core-platform-security`, user directive "내가 U1을 맡았어 … 자율주행"): real security + LLM gateway + platform + onboarding UI on top of the Milestone 0 contracts.
  - Security: `PasswordKeyManager` (Argon2id → 256-bit key, salt + encrypted verifier, plaintext key never on disk, zeroized in memory), `vault` (AES-256-GCM), `FileEncryptedStore` (encrypted-at-rest KV + `CredentialStore`). `KeyHandle` upgraded to real zeroizing key material.
  - LLM gateway: `RegexMasker` (fixed-point masking, US-2.2/R1), `prompts`, `TransferLog` (NFR-2 egress transparency), real `AnthropicLlm` behind default-off `llm-http` feature.
  - Platform: `AppState`, `Scheduler`, `commands` (onboarding/security/settings/transparency); added `AppConfig`/`TransferRecord`/`AppStatus`.
  - Frontend: React onboarding (Setup/Unlock/Home) + typed IPC bridge w/ browser mock. Desktop: standalone Tauri 2 crate `desktop/`.
  - Verified in sandbox (zig-cc): `cargo test` 39/39, `cargo clippy -D warnings` clean, `cargo run` demo, `cargo fmt`. Unverifiable-in-sandbox but built on normal toolchains: `llm-http` (ring C build), `desktop/` Tauri + frontend (no webview/node).
  - Stories US-7.1 (setup+encrypt), US-7.2 (unlock/key re-derive, wrong-pw rejected), US-7.3 (encrypted credentials) implemented + tested. PBT-02/03/07/08/09 satisfied (proptest).
- **U1 Milestone 0 done early** (user directive "오늘 모인 김에 공통 작업 먼저"): shared contract layer written as code so 4 devs can start in parallel.
  - `src-tauri/` Rust crate `knows_me_core`: core/{error,types,traits}.rs + mocks.rs + smoke tests
  - `src/shared/contracts.ts` frontend type mirror
  - Build VERIFIED locally: installed Rust (rustup) + zig-as-cc linker (no system cc available). `cargo build`, `cargo test` (5 pass), `cargo run`, and `cargo clippy --all-targets -- -D warnings` (clean) all green. Cargo.lock committed.
  - Note: devs with a normal C toolchain just run `cargo test` (the zig linker was only needed in this sandbox).
- Deviation note: this is a slice of U1 Code Generation produced ahead of the formal per-unit Functional Design/NFR gates, at user request. Remaining U1 impl and per-unit stages (Functional Design → NFR Requirements → NFR Design → Code Generation) still to run per execution plan.

## Units (4, modular monolith, 1 owner each, contract-first parallel)
- U1 Core Platform & Security (Dev A) — US-7.x + shared contracts/crypto/LLM gateway
- U2 Ingestion & Processing (Dev B) — US-1.x, US-2.x
- U3 Knowledge & Interview (Dev C) — US-3.x, US-4.x
- U4 Interface & Persona (Dev D) — US-5.x, US-6.x
- Integration order: U1 → U3 → U2 → U4 (development parallel via mocks)

## Application Design Decisions
- AD1 Frontend: React + TypeScript (Tauri webview)
- AD2 Backend: Rust core single (Tauri commands)
- AD3 Connectors: common Connector trait + per-source impl
- AD4 Persona API: Rust embedded HTTP server (localhost, no external exposure in MVP)
- AD5 Communication: layered + service orchestration (no cyclic deps)

## Execution Plan Summary
- **Stages to Execute**: Application Design, Units Generation, Functional Design, NFR Requirements, NFR Design, Code Generation, Build and Test
- **Stages to Skip**: Reverse Engineering (greenfield), Infrastructure Design (local desktop app, no cloud infra)
- **Risk Level**: Medium

## Workspace State
- **Existing Code**: No
- **Programming Languages**: None yet (target: Tauri = Rust + web frontend, per requirements)
- **Build System**: None yet
- **Project Structure**: Empty (docs only)
- **Reverse Engineering Needed**: No
- **Workspace Root**: /home/user/repos/knows-me

## Code Location Rules
- **Application Code**: Workspace root (NEVER in aidlc-docs/)
- **Documentation**: aidlc-docs/ only
- **Structure patterns**: See code-generation.md Critical Rules

## Extension Configuration
| Extension | Enabled | Decided At |
|---|---|---|
| Security Baseline | No | Requirements Analysis |
| Resiliency Baseline | No | Requirements Analysis |
| Property-Based Testing | Yes (Partial) | Requirements Analysis |

**Property-Based Testing — Partial mode**: Only rules PBT-02, PBT-03, PBT-07, PBT-08, PBT-09 are enforced (blocking). All other PBT rules are advisory (non-blocking).

## Stage Progress
### 🔵 INCEPTION PHASE
- [x] Workspace Detection
- [ ] Reverse Engineering (N/A - greenfield)
- [x] Requirements Analysis
- [x] User Stories
- [x] Workflow Planning
- [x] Application Design — EXECUTE
- [x] Units Generation — EXECUTE (unit-of-work + dependency + story-map generated - awaiting approval)

### 🟢 CONSTRUCTION PHASE
- [x] U1 Milestone 0 (shared contracts + mocks) — DONE (early, per user directive)
- [x] U1 Code Generation (full: security + LLM gateway + platform + onboarding UI) — DONE on branch `feat/u1-core-platform-security` (autonomous)
- [~] Functional Design — EXECUTE (per-unit) — **U1 DONE (autonomous), U2 DONE (approved)**; U3/U4 pending
- [~] NFR Requirements — EXECUTE (per-unit) — **U2 DONE (approved)**; others pending
- [~] NFR Design — EXECUTE (per-unit) — **U2 DONE (approved)**; others pending
- [ ] Infrastructure Design — SKIP (local desktop app, no cloud infra)
- [~] Code Generation — EXECUTE (per-unit; U1 full impl done) — **U2 DONE (build/test/clippy green: 35 tests pass; awaiting approval)**; U3/U4 pending
- [ ] Build and Test — EXECUTE

### 🟡 OPERATIONS PHASE
- [ ] Operations (placeholder)
