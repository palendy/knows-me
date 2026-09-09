# AI-DLC State Tracking

## Project Information
- **Project Name**: knows-me
- **Project Type**: Greenfield
- **Start Date**: 2026-09-08T06:46:46Z
- **Current Stage**: CONSTRUCTION - **전 유닛(U1~U4) main 병합 완료 + 통합 배선(G1~G5) 해소 완료**. 순서: U4 PR #2 병합(`511d944`) → 앱 배선 G1~G6(`9b821a3`) → 코드리뷰 반영(`d60cdd5`/`53ad360`) → U2 세션 수집 실배선(`789741e`) → `.env.example`(`cf2fa02`). 실제 앱 창 기동 확인 완료(사용자, 2026-09-09). **남은 것**: G6 TransferLog 일원화 결정, stale 브랜치 정리(construction/integration·construction/u4-interface-persona·chore/cargo-fmt), Build and Test 최종 승인.

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

## ✅ 통합 배선 (Build and Test) — G1~G5 해소 완료
4개 유닛 라이브러리가 완성·검증된 뒤, 앱 배선이 `9b821a3`에서 수행되어 main에 병합되었다. 실행 앱이 잠금 해제 후 6탭(대시보드·대기열·미니홈피·그래프·페르소나·설정)을 노출한다.
- **G1** ✅ `src/App.tsx` — 잠금 해제 후 6탭 셸(`UnlockedShell`); 뷰는 포트(`KnowsMeApi`/`InterviewApi`)만 의존, Tauri면 어댑터·아니면 mock 주입
- **G2** ✅ `desktop/src/main.rs` — U3/U4 command 등록(get_dashboard/get_minihome/get_graph, persona_chat/persona_draft, queue_list/queue_answer, local_api_status/set_server_enabled 등)
- **G3** ✅ AppState는 U1 소유로 불변 유지; 별도 `Services` managed state가 unlock 시 서비스 조립·lock 시 해제 (`desktop/src/services.rs` 신규)
- **G4** ✅ `src/features/queue/` 신설 — 포트·Tauri 어댑터·mock·`QueueView`·테스트
- **G5** ✅ unlock 시 `server_enabled`면 LocalApiServer 기동, 핸들을 `ServiceSet`에 보관(drop 시 종료 방지), 토글·포트 조회 command
- **G6** ⏳ **미결정** — `TransferLog`가 `llm::`(메모리) / `processing::`(암호화 저장) 2종 공존. 배선 범위 외로 미룸(정보성). U4 경로는 U1 `TransferLog` 사용. 단일 출처 일원화 결정 필요.

상세: `aidlc-docs/construction/build-and-test/app-wiring-report.md`(해소 내역) · `integration-verification-report.md`(배선 전 발견 근거)

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
- [~] Build and Test — **앱 배선(G1~G5) 해소·main 병합 완료**. 재검증(로컬 2026-09-09): `cargo fmt --check`(src-tauri) clean · `cargo test`(core) 통합 9 + PBT 3 포함 실패 0 · `npm test` 51 pass(App.test teardown uncaught 2건, 비치명) · `tsc --noEmit` clean. 실제 앱 창 기동 확인 완료(사용자). **남은 것**: G6 TransferLog 일원화 결정, stale 브랜치 정리, Build and Test 최종 승인. 상세: `build-and-test/app-wiring-report.md`

**U4 — Interface & Persona (Dev D)** — PR #2 **머지 완료(`511d944`)**; 이후 통합 배선(`9b821a3~`)이 main에 반영됨
- [x] Functional Design (domain-entities, business-logic-model, business-rules, frontend-components)
- [x] NFR Requirements (nfr-requirements, tech-stack-decisions)
- [x] NFR Design (nfr-design-patterns, logical-components)
- [x] Code Generation (US-5.1~5.3, US-6.1~6.2)
- [x] 동료 코드리뷰(xhigh, coolfebreeze) 9건 반영 — 🔴2 · 🟡5 · 🟢2 전부 수정
- [x] main rebase 후 재검증
- [x] 전 유닛 통합 검증 수행 — `src-tauri/tests/integration_all_units.rs` 9건 + build-and-test 산출물 3종

### 🟡 OPERATIONS PHASE
- [ ] Operations (placeholder)
