# AI-DLC State Tracking

## Project Information
- **Project Name**: knows-me
- **Project Type**: Greenfield
- **Start Date**: 2026-09-08T06:46:46Z
- **Current Stage**: CONSTRUCTION - U1 Milestone 0 (shared contracts + mocks) delivered early per user directive; per-unit design stages pending

## Construction Notes
- **U1 Milestone 0 done early** (user directive "오늘 모인 김에 공통 작업 먼저"): shared contract layer written as code so 4 devs can start in parallel.
  - `src-tauri/` Rust crate `knows_me_core`: core/{error,types,traits}.rs + mocks.rs + smoke tests
  - `src/shared/contracts.ts` frontend type mirror
  - Build NOT verified in this environment (no Rust/Node toolchain) — run `cargo test` locally to confirm.
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
- [~] U1 Milestone 0 (shared contracts + mocks) — DONE (early, per user directive)
- [ ] Functional Design — EXECUTE (per-unit)
- [ ] NFR Requirements — EXECUTE (per-unit)
- [ ] NFR Design — EXECUTE (per-unit)
- [ ] Infrastructure Design — SKIP (local desktop app, no cloud infra)
- [ ] Code Generation — EXECUTE (per-unit; U1 contracts slice done)
- [ ] Build and Test — EXECUTE

### 🟡 OPERATIONS PHASE
- [ ] Operations (placeholder)
