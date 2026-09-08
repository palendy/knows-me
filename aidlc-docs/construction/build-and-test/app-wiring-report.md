# 앱 배선 완료 보고서 (Build and Test — 통합 배선)

> 실행: 2026-09-08, macOS (Rust / node)
> 브랜치: `construction/integration` (base = `origin/construction/u4-interface-persona`)
> 계기: `integration-verification-report.md`가 남긴 G1~G6(앱 배선 미수행) 해소.
> 성격: 라이브러리는 이미 완성·검증됨. 이 작업은 실행 앱이 U3/U4 기능을 노출하도록 조립(wiring)한 것.

## 결론 요약

| 층위 | 배선 전 | 배선 후 |
|---|---|---|
| 라이브러리 (4개 유닛) | ✅ 통과 | ✅ 유지 |
| 유닛 간 실제 연동 | ✅ 통과 | ✅ 유지 |
| **실행 앱의 기능 노출** | ❌ U1만 | ✅ **6개 화면 노출** (대시보드·대기열·미니홈피·그래프·페르소나·설정) |

**한 줄로**: 잠금 해제 후 탭 셸에서 U3 인터뷰 대기열과 U4 조회 3종·페르소나가 모두 노출된다. 프론트 번들에 5개 뷰 텍스트가 실제로 포함됨을 확인(배선 전에는 전부 미포함이었다).

## G1~G6 해소 내역

| # | 항목 | 조치 | 파일 |
|---|---|---|---|
| G1 | `App.tsx`가 온보딩만 렌더 | 잠금 해제 후 6탭 셸(`UnlockedShell`). 뷰는 포트(`KnowsMeApi`/`InterviewApi`)만 의존, Tauri면 어댑터·아니면 mock 주입 | `src/App.tsx` |
| G2 | U3/U4 command 미등록 | `get_dashboard`/`get_minihome`/`get_graph`/`persona_chat`/`persona_draft` + `queue_list`/`queue_answer` + `local_api_status`/`set_server_enabled` 등록 | `desktop/src/main.rs` |
| G3 | AppState가 U1만 보유 | AppState는 U1 소유로 **불변**. 별도 `Services` managed state가 unlock 시 서비스 조립, lock 시 해제 | `desktop/src/services.rs` (신규) |
| G4 | Queue UI 부재 | `src/features/queue/` 신설 — 포트·Tauri 어댑터·mock·`QueueView`·테스트 | `src/features/queue/*` |
| G5 | LocalApiServer 미기동 | unlock 시 `server_enabled`면 기동, 핸들을 `ServiceSet`에 보관(drop 시 종료 방지). 토글·포트 조회 command | `desktop/src/services.rs`, `main.rs` |
| G6 | TransferLog 이원화 | 이번 범위 외(정보성). U4 경로는 U1 `TransferLog`를 그대로 사용 | — |

## LLM 구성 (신규)

- `llm::build_client(transfer_log)` 팩토리: feature `llm-http` 없으면 항상 `CannedLlm`(키 없이 앱 기동). 있으면 `LLM_PROVIDER`(anthropic|openai)로 분기, 키 없으면 CannedLlm 폴백.
- **OpenAI 호환 클라이언트 `OpenAiLlm` 신규**: `POST /v1/chat/completions`, `Authorization: Bearer`, `choices[].message.content` 파싱. `OPENAI_API_KEY`/`OPENAI_MODEL`/`OPENAI_BASE_URL`. `OPENAI_BASE_URL`로 OpenAI 호환 게이트웨이(Azure/OpenRouter/로컬)도 지원.

## 검증 결과 (전부 로컬 실행)

| 명령 | 결과 |
|---|---|
| `cargo test` (core, default) | **160 pass** (148 lib + 9 통합 + 3 PBT) |
| `cargo test --features llm-http` (core) | **162 pass** (신규 OpenAI 파싱 2건 포함) |
| `cargo clippy --all-targets --features llm-http -- -D warnings` (core) | clean |
| `cargo clippy --all-targets -- -D warnings` (desktop, default+llm-http) | clean |
| `cargo fmt --check` (core + desktop) | clean |
| `cargo check` (desktop, default + llm-http) | 통과 |
| `npm test` | **51 pass** (기존 41 + Queue 5 + App 5) |
| `npx tsc --noEmit` | clean |
| `npm run build` (vite) | 성공 (165.49 kB / gzip 54.53 kB — 배선 전 149 kB 대비 뷰 포함) |
| 번들 뷰 포함 스캔 | 대시보드·인터뷰 대기열·미니홈피·지식 그래프·페르소나 **전부 포함** |

## 남은 확인 (사용자 몫)

**실제 창 기동**은 데스크톱 GUI/webview 세션이 필요해 이 환경(헤드리스)에서는 창을 띄워 스크린샷을 찍지 못했다. 위 검증(번들 포함 + desktop 컴파일 + 프론트 테스트)으로 배선이 정상임은 확인되었으나, 실물 창은 다음으로 확인:

```bash
npm install
npx tauri dev          # 온보딩 → 비밀번호 설정 → 잠금 해제 → 6개 탭
# 실제 클라우드 LLM을 쓰려면:
#   cargo 로 llm-http feature 켜고(예: 앱 빌드시 --features llm-http)
#   ANTHROPIC_API_KEY=...  또는  LLM_PROVIDER=openai OPENAI_API_KEY=...
```

키를 주지 않으면 페르소나/인터뷰 답변은 `CannedLlm`으로 결정적 응답을 낸다(앱은 정상 기동, 조회 3종은 애초에 LLM 불필요).
