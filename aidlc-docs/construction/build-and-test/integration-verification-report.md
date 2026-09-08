# 통합 검증 보고서 (전 유닛 병합 기준)

> 실행: 2026-09-08, macOS (Rust 1.97.1 / node 24.5.0)
> 대상: `construction/u4-interface-persona` = `origin/main` + U4 (fast-forward 병합 가능 = **병합 후 상태와 동일**)
> 계기: U4가 마지막 유닛이므로 "전부 합쳐진 뒤 문제 없는지" 확인

## 결론 요약

| 층위 | 상태 |
|---|---|
| 라이브러리 (4개 유닛 코드) | ✅ 전부 통과 — 빌드·테스트·lint·fmt |
| 유닛 간 실제 연동 (mock 아님) | ✅ 통과 — 신규 통합 테스트 9건으로 확인 |
| 보안 (암호화·마스킹·로컬 노출) | ✅ 통과 — 실물 스택에서 검증 |
| **실행되는 앱의 기능 노출** | ❌ **U1만 노출됨. U2·U3·U4가 앱에 배선되지 않음** |

**한 줄로**: 코드는 4개 유닛 모두 완성·검증되었고 서로 정상적으로 결합하지만, **데스크탑 앱을 실행하면 온보딩/잠금(U1) 외에는 아무것도 보이지 않는다.** 배선(wiring)이 빠져 있다.

---

## 1. 자동 검증 결과 (전부 로컬 실행)

| 명령 | 결과 |
|---|---|
| `cargo fmt --check` (src-tauri) | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo test` (lib) | **148 pass** |
| `cargo test --test integration_all_units` | **9 pass** (신규) |
| `cargo test --test u2_pbt` | **3 pass** |
| `cargo check` (desktop crate, Tauri 2) | 통과 |
| `npm test` | **41 pass** |
| `npx tsc --noEmit` | clean |
| `npm run build` | 성공 (149.32 kB / gzip 48.25 kB) |
| 시크릿 하드코딩 스캔 | 무검출 |
| 외부 인터페이스 바인딩 스캔(`0.0.0.0`) | 무검출 (loopback 전용) |

**Rust 총 160 테스트 + 프론트엔드 41 테스트 = 201 통과, 실패 0.**

---

## 2. 신규: 유닛 간 실제 연동 검증

`src-tauri/tests/integration_all_units.rs` — 각 유닛의 자체 테스트는 mock을 상대로 통과한다. 이 파일은 다른 질문에 답한다: **실물끼리 붙였을 때도 되는가.**

데스크탑 앱이 조립할 스택을 그대로 만든다:
```
U1 PasswordKeyManager(Argon2) + FileEncryptedStore(AES-256-GCM, 실제 디스크) + RegexMasker
  -> U3 KnowledgeService + InterviewService
    -> U4 QueryService + PersonaService + LocalApiServer
```
네트워크만 테스트 더블이다(클라우드 LLM은 실물을 쓸 수 없음).

| 테스트 | 확인 내용 |
|---|---|
| `read_views_serve_facts_written_through_the_real_encrypted_store` | 실제 암호화 저장소에 쓴 사실이 대시보드·미니홈피·그래프에 나온다. 링크가 엣지가 되고 연결도 높은 사실이 앞선다 (US-5.1~5.3) |
| `persona_grounds_answers_in_facts_confirmed_through_the_interview_queue` | Queue 후보 → 소유자 확인 → 확정 사실 → 페르소나 답변까지 실제 경로로 이어진다. Queue가 비면 대시보드 수치도 따라 변한다 (US-4.x → US-6.1) |
| `identifiers_are_masked_by_the_real_masker_before_leaving_the_device` | **U1의 실제 `RegexMasker`** 기준으로 이메일·전화번호가 게이트웨이에 도달하지 않고, 답변에서는 원문으로 복원된다 (NFR-2, R1) |
| `read_views_answer_with_no_llm_wired_in_at_all` | 조회 3종이 LLM 없이 동작한다. `QueryService`에 `LlmClient` 자체가 주입되지 않아 구조적으로 보장됨 (NFR-3) |
| `persona_says_it_cannot_answer_before_anything_is_confirmed` | 확정 사실이 0개면 클라우드 호출 없이 결정적 메시지 (BR-P4) |
| `local_api_drafts_over_the_real_stack_and_refuses_non_loopback_hosts` | `POST /draft`가 실제 스택 기반으로 초안을 만들고, 비-loopback `Host`는 403 (US-6.2, D7) |
| `locking_the_vault_blocks_reads_across_every_unit` | 잠금 상태에서 조회가 `AppError::Locked`로 거부된다 (FR-5.3) |
| `facts_are_encrypted_at_rest` | 데이터 디렉터리 전체를 훑어 **사실의 제목·본문 평문이 디스크에 존재하지 않음**을 확인 (FR-5.1, NFR-2) |
| `a_raw_item_travels_from_processing_all_the_way_to_a_persona_answer` | **4개 유닛 전 구간**: 원본 수집 항목 → U2 가공(마스킹→LLM→라우팅) → U3 저장/Queue → U4 조회·페르소나. 전송 투명성 로그에 기록되며 그 로그에도 원문 식별자가 남지 않는다 |

---

## 3. ❌ 발견: 앱에 배선되지 않은 유닛

라이브러리는 완성되었지만 **실행 파일이 그것을 노출하지 않는다.**

### 근거 (추측 아님)
빌드 산출물 `dist/assets/*.js`를 직접 조회한 결과:

| 화면 | 번들 포함 여부 |
|---|---|
| 온보딩/잠금 (U1) | ✅ 포함 |
| 대시보드 (US-5.1) | ❌ 없음 |
| 미니홈피 (US-5.2) | ❌ 없음 |
| 지식 그래프 (US-5.3) | ❌ 없음 |
| 페르소나 챗 (US-6.1) | ❌ 없음 |

Vite가 import되지 않은 모듈을 제거하므로, 번들에 없다 = **`App.tsx`가 렌더하지 않는다**는 뜻이다.

### 구체적으로 빠진 것

| # | 항목 | 현재 | 필요한 것 | 소유 |
|---|---|---|---|---|
| G1 | `src/App.tsx` | 온보딩 3개 뷰만 렌더 | 잠금 해제 후 대시보드·미니홈피·그래프·챗으로 갈 라우팅/탭 | U1 (앱 셸) |
| G2 | `desktop/src/main.rs` | `#[tauri::command]` 7개 (전부 U1) | U3 조회/Queue + U4 조회 3종·페르소나 2종 command 등록 | U1 |
| G3 | `core::app_state::AppState` | U1 컴포넌트만 보유 | `KnowledgeService`·`InterviewService`·`PersonaService`(+`IngestionService`) 보유 | U1 |
| G4 | `src/features/queue/` | **디렉터리 자체가 없음** | 인터뷰 Queue UI (US-4.x). unit-of-work 상 U3 산출물 | U3 |
| G5 | `LocalApiServer` | 어디서도 `start()` 호출되지 않음 | 앱 시작 시 기동, 핸들을 `AppState`에 보관, 실제 포트 UI 표시 (US-6.2 AC1) | U1 |
| G6 | `TransferLog` | `llm::TransferLog`(U1, 메모리)와 `processing::TransferLog`(U2, 암호화 저장) **2종 공존** | 어느 쪽이 단일 출처인지 정리. `list_transfers` command는 U1 쪽만 노출 | U1/U2 |

### 영향
- **심사 기준 ④(동작하는 코드 + 스크린샷)**: 지금 앱을 띄워 찍을 수 있는 화면은 온보딩·잠금뿐이다. MVP 핵심인 수집·Queue·미니홈피·페르소나가 화면에 없다.
- **US-4.x, US-5.x, US-6.1**: 백엔드는 되지만 사용자가 도달할 경로가 없다.
- **US-6.2 AC1**("앱이 실행 중이고 로컬 서버가 켜져 있을 때"): 서버가 기동되지 않으므로 전제 자체가 미충족. 라이브러리 수준에서는 통과(위 통합 테스트).

### 성격
이건 어느 한 유닛의 버그가 아니라 **AI-DLC의 마지막 단계인 Build and Test / 통합 배선이 아직 수행되지 않은 것**이다. 4명이 contract-first로 병렬 개발한 결과 각 유닛은 정상이고 서로 결합도 되지만, 조립을 담당할 사람이 아직 조립하지 않았다. G1·G2·G3·G5는 전부 U1 소유 파일(`App.tsx`, `desktop/src/main.rs`, `app_state.rs`)이라 U4가 단독으로 손대면 유닛 경계를 깬다.

---

## 4. 권고 순서

1. **G3 → G2 → G1** (U1): `AppState`에 서비스 보유 → command 등록 → `App.tsx`에서 잠금 해제 후 탭 노출. U4 뷰는 `KnowsMeApi` 포트만 의존하므로 `TauriApi` 주입 한 줄이면 붙는다 (`code/integration-handoff.md` 참조).
2. **G4** (U3): Queue UI. 이게 없으면 사실을 확정할 방법이 UI에 없어 미니홈피·페르소나가 빈 상태로만 보인다.
3. **G5** (U1): 로컬 API 기동. 핸들을 drop하면 서버도 종료되므로 반드시 `AppState`에 보관할 것.
4. **G6** (U1/U2): `TransferLog` 일원화.
5. 배선 후 스크린샷 촬영 (심사 기준 ④).
