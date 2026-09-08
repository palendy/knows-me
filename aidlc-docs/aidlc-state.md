# AI-DLC State Tracking

## Project Information
- **Project Name**: knows-me
- **Project Type**: Greenfield
- **Start Date**: 2026-09-08T06:46:46Z
- **Current Stage**: CONSTRUCTION - U1/U2/U3 머지 완료(main). U4 (Interface & Persona) 완료 + 동료 코드리뷰(xhigh) 반영 — branch `construction/u4-interface-persona`, PR #2. 남은 것: Build and Test(전 유닛 통합).

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

## ⚠️ 통합 미완 항목 (Build and Test)
4개 유닛 라이브러리는 전부 완성·검증되었고 실물끼리 정상 결합한다. 그러나 **실행되는 앱은 U1(온보딩/잠금)만 노출한다** — 빌드 산출물 `dist/assets/*.js`에 대시보드·미니홈피·그래프·페르소나 챗 화면이 존재하지 않음을 확인했다.
- **G1** `src/App.tsx` — U4 뷰 4종 라우팅 없음 (U1 소유)
- **G2** `desktop/src/main.rs` — U1 command 7개만 등록, U3/U4 command 없음 (U1 소유)
- **G3** `core::app_state::AppState` — U1 컴포넌트만 보유 (U1 소유)
- **G4** `src/features/queue/` — 인터뷰 Queue UI 미구현 (U3 소유)
- **G5** `LocalApiServer::start()` 호출 지점 없음 → US-6.2 AC1 전제 미충족 (U1 소유)
- **G6** `TransferLog`가 `llm::`/`processing::` 2종 공존 (U1/U2)

상세·근거·권고 순서: `aidlc-docs/construction/build-and-test/integration-verification-report.md`

## U4 Construction Notes (Dev D)
- **유닛 경계 준수**: U1(`core/**`, `mocks.rs`, Tauri 셸, `vite.config.ts`), U2(`ingestion/`, `processing/`), U3(`knowledge/`, `interview/`, `src/features/queue/`) 파일을 일절 수정하지 않음. 공용 파일은 `Cargo.toml`(dependency 추가)과 `lib.rs`(`pub mod persona;` 1줄)만 추가 변경.
- **테스트 툴체인 신규 추가**: `package.json`/`tsconfig.json`/`vitest.config.ts`/`src/test-setup.ts`. 번들러(`vite.config.ts`)와 앱 엔트리는 U1 몫으로 남겨 둠.
- **Tech stack 추가**: axum 0.7 + tokio(로컬 API), proptest 1(Rust PBT), fast-check 3 + Vitest 2 + RTL(TS PBT/예제 테스트). 그래프는 의존성 없는 자체 SVG.
- **통합 지침**: `aidlc-docs/construction/u4-interface-persona/code/integration-handoff.md` — U1이 등록할 command 5종, 뷰 마운트 방법, U3 `search` 질의 의미론에 대한 요청 사항 포함.
- **승인 게이트**: 사용자가 "다 완료하고 푸쉬"로 U4 전 단계 승인을 일괄 위임함(audit.md 기록).

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
- [x] U1 Code Generation (full: security + LLM gateway + platform + onboarding UI) — DONE, merged to main
- [x] Functional Design — **U1 DONE, U2 DONE, U3 DONE, U4 DONE** (4 artifacts each)
- [x] NFR Requirements — **U2 DONE, U3 DONE, U4 DONE** (2 artifacts each); U1은 본구현과 함께 처리
- [x] NFR Design — **U2 DONE, U3 DONE, U4 DONE** (2 artifacts each); U1은 본구현과 함께 처리
- [ ] Infrastructure Design — SKIP (local desktop app, no cloud infra)
- [x] Code Generation — **U1 DONE (merged), U2 DONE (merged, 35 tests), U3 DONE (merged, 26 tests), U4 DONE (PR #2)**
- [~] Build and Test — 부분 완료. 자동 검증 전부 통과(Rust 160 + FE 41, 실패 0), 유닛 간 실물 연동 통합 테스트 9건 신규 추가·통과. **미완: 앱 배선(G1~G6)** — `aidlc-docs/construction/build-and-test/integration-verification-report.md` §3

**U4 — Interface & Persona (Dev D)** — branch `construction/u4-interface-persona`, PR #2
- [x] Functional Design (domain-entities, business-logic-model, business-rules, frontend-components)
- [x] NFR Requirements (nfr-requirements, tech-stack-decisions)
- [x] NFR Design (nfr-design-patterns, logical-components)
- [x] Code Generation (US-5.1~5.3, US-6.1~6.2)
- [x] 동료 코드리뷰(xhigh, coolfebreeze) 9건 반영 — 🔴2 · 🟡5 · 🟢2 전부 수정
- [x] main rebase 후 재검증
- [x] 전 유닛 통합 검증 수행 — `src-tauri/tests/integration_all_units.rs` 9건 + build-and-test 산출물 3종

### 🟡 OPERATIONS PHASE
- [ ] Operations (placeholder)
