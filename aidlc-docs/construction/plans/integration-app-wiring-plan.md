# 통합 배선 계획 (App Wiring) — Code Generation Part 1

> 브랜치: `construction/integration` (base = `origin/construction/u4-interface-persona` = 전 유닛 병합 상태)
> 계기: 라이브러리는 4개 유닛 완성·검증되었으나, 실행 앱이 U1(온보딩/잠금)만 노출. U2·U3·U4가 앱에 배선되지 않음(`build-and-test/integration-verification-report.md` G1~G6).
> 성격: 어느 유닛의 버그가 아니라 AI-DLC 마지막 단계인 **Build and Test / 통합 배선** 미수행. U1 소유 파일(`AppState`·`main.rs`·`App.tsx`)을 손대야 하므로 통합 브랜치에서 수행.

## 범위 (승인된 결정)

- ✅ 조회 3종(대시보드 US-5.1 · 미니홈피 US-5.2 · 그래프 US-5.3)
- ✅ 페르소나 챗(US-6.1) + 로컬 API(US-6.2)
- ✅ **인터뷰 Queue UI 포함**(US-4.x, U3 소유 산출물)
- ✅ LLM: **둘 다 feature 전환** — 기본 `CannedLlm`(키 없이 앱 기동), `llm-http` feature 시 실제 클라이언트
- ✅ **OpenAI 형식 클라이언트 신규 추가**(`OpenAiLlm`) — `LLM_PROVIDER` 환경변수로 anthropic/openai 선택
- ✅ 서비스 보유: **별도 `Services` managed state** (AppState는 U1 소유로 그대로 유지)

## 설계 결정 (근거)

1. **`Services`를 별도 managed state로.** `AppState`(U1 소유, `#[derive(Clone)]`)를 확장하지 않는다. `Services { RwLock<Option<ServiceSet>> }`를 `app.manage`로 별도 등록. unlock 성공 후 서비스를 조립해 채우고, lock 시 비운다. U1 경계 손상 최소화.
2. **서비스는 unlock 후에만 유효.** store가 키를 요구하므로, 잠금 상태에서 U3/U4 커맨드는 `AppError::Locked`(→ IPC String "locked: unlock required") 반환. UI는 이를 안내로 표시.
3. **LLM 구성은 core에 팩토리 함수로.** `llm::build_client(provider, transfer_log) -> Arc<dyn LlmClient>`. feature `llm-http` 없으면 항상 `CannedLlm`. 있으면 `LLM_PROVIDER`(anthropic|openai, 기본 anthropic)로 분기, 키 없으면 CannedLlm으로 폴백(앱은 뜨되 실제 호출은 안 됨).
4. **탭은 App.tsx unlocked 분기에.** HomeView(보안 표면)를 "설정" 탭 중 하나로 배치. `KnowsMeApi` 인스턴스는 App.tsx unlocked 시점에 한 번 생성해 각 뷰에 prop 주입. Queue는 별도 포트(`InterviewApi` TS)로 주입.
5. **타입 coercion.** `state.store()`(`Arc<FileEncryptedStore>`)·`masker()`(`Arc<RegexMasker>`)를 `Arc<dyn EncryptedStore>`/`Arc<dyn Masker>`로 coerce해 U3/U4 생성자에 전달.

## 작업 항목 (체크리스트)

### A. LLM 팩토리 + OpenAI 클라이언트 (Rust, core)
- [x] A1. `src-tauri/src/llm/client.rs` — `#[cfg(feature="llm-http")]`로 `OpenAiLlm` 추가: `POST {base_url}/v1/chat/completions`, `Authorization: Bearer`, `choices[].message.content` 파싱. `OpenAiConfig::from_env()`(`OPENAI_API_KEY`/`OPENAI_MODEL`/`OPENAI_BASE_URL`). `LlmClient`의 4메서드(summarize/classify/vision_extract/chat) 구현.
- [x] A2. `src-tauri/src/llm/mod.rs` (또는 client.rs) — `pub fn build_client(transfer_log: Arc<TransferLog>) -> Arc<dyn LlmClient>` 팩토리. feature off → CannedLlm. feature on → `LLM_PROVIDER` 분기, 키 로드 실패 시 CannedLlm 폴백 + 경고 로그.
- [x] A3. 단위 테스트: OpenAI 응답 파싱 왕복(모의 payload), 팩토리 분기(env 조합별). Anthropic 기존 테스트 불변 확인.

### B. Rust 배선 — Services managed state + 커맨드 (desktop)
- [x] B1. `desktop/src/services.rs` (신규) — `struct ServiceSet { knowledge: Arc<dyn KnowledgeApi>, interview: Arc<dyn InterviewApi>, query: QueryService, persona: Arc<dyn PersonaApi>, local_api: Option<LocalApiHandle> }` + `struct Services(RwLock<Option<ServiceSet>>)`. `build(state) -> ServiceSet` 조립 함수(KnowledgeService→InterviewService/QueryService/PersonaService 순, LLM은 A2 팩토리).
- [x] B2. `desktop/src/main.rs` — unlock 커맨드 래퍼에서 `commands::unlock` 성공 후 `services.set(build(...))` + `KnowledgeService::build_index()`. lock 커맨드에서 `services.clear()`(+ local_api stop). setup 후에도 초기화 상태 유지.
- [x] B3. U4 커맨드 5개 등록: `get_dashboard`/`get_minihome`/`get_graph`/`persona_chat`/`persona_draft` → Services에서 꺼내 위임. 잠금 시 `AppError::Locked` 반환.
- [x] B4. U3 Queue 커맨드 2개 등록: `queue_list(sort)` → `InterviewApi::list`, `queue_answer(id, input)` → `InterviewApi::answer`. (enqueue는 processing 몫이라 UI 미노출)
- [x] B5. 로컬 API 수명: unlock 시 `AppConfig.server_enabled`면 `LocalApiServer::start(persona, DEFAULT_PORT)` 호출, 핸들을 ServiceSet에 보관(drop되면 서버 죽음). `set_server_enabled` 커맨드 등록 + 토글 반영. 실제 포트를 조회하는 `local_api_status` 커맨드 추가(UI 표시용).
- [x] B6. `invoke_handler`에 신규 커맨드 전부 등록, `app.manage(Services::default())` 추가.

### C. 프론트 — Interview 포트 + Queue 뷰 (TypeScript)
- [x] C1. `src/features/queue/api.ts` (신규) — `InterviewApi` 포트: `list(sort): Promise<QueueItem[]>`, `answer(id, input): Promise<AnswerResult>`. `contracts.ts`의 `QueueItem`/`AnswerInput`/`AnswerResult`/`QueueSort` 사용.
- [x] C2. `src/features/queue/tauri-interview-api.ts` + `mock-interview-api.ts` — Tauri 어댑터(`queue_list`/`queue_answer`) + mock(개발/테스트용).
- [x] C3. `src/features/queue/QueueView.tsx` (신규) — 대기 항목 목록, 각 항목에 확정(Choice yes)/거부(Choice no)/스킵(Skip)/정정(Text) 액션. `u4-shared/StateShell`로 loading/empty/error 처리. 답변 후 목록·대시보드 새로고침 콜백.
- [x] C4. `QueueView.test.tsx` — mock 기반: 목록 렌더, 확정 시 answer 호출, 빈 큐 empty 상태, 에러 상태. (US-4.x AC)

### D. 프론트 — 앱 셸 배선 (TypeScript, U1 소유 App.tsx)
- [x] D1. `src/App.tsx` unlocked 분기에 탭 컨테이너: 대시보드 / 미니홈피 / 그래프 / 페르소나 / Queue / 설정(HomeView). `new TauriApi(call)` + `new TauriInterviewApi(call)`를 unlocked 시 1회 생성해 주입. `ipc.ts`의 `call`(또는 동일 invoke) 재사용.
- [x] D2. 최소 탭 UI(접근성: 버튼 role, 키보드 이동) + 인라인 스타일 재사용(`u4-shared/styles.ts`). 뷰 간 새로고침 시 대시보드 pending_queue가 Queue 변화 반영하도록 공용 refresh 신호.
- [x] D3. `App.test.tsx`(있으면 확장, 없으면 신규) — 잠금 해제 후 탭이 노출되고 각 뷰가 mock으로 렌더되는지.

### E. 검증 (Build and Test)
- [x] E1. `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` (기본 + `--features llm-http`) clean.
- [x] E2. `cargo test`(lib+통합+PBT) 전부 pass — 기존 160 유지 + 신규(A3) 통과.
- [x] E3. `npm test`(기존 41 + 신규 C4/D3) pass, `tsc --noEmit` clean, `npm run build`(vite) 성공.
- [x] E4. `cargo check`(desktop crate, 기본 feature) + `cargo check -p ...`가능 시 `--features llm-http` 통과.
- [x] E5. **앱 기동 확인**: `npx tauri dev`로 실행 → 온보딩 → 잠금 해제 → 6개 탭 노출 확인. (환경 제약으로 헤드리스 불가 시, 번들에 뷰가 포함되는지 `npm run build` 산출물로 대체 확인 + 사용자에게 수동 실행 안내.)
- [x] E6. 통합 검증 리포트 갱신(G1~G6 해소 반영) + 커밋.

## 유닛 경계 영향 (투명성)

이 작업은 U1 소유 파일을 수정한다 — 통합 단계이므로 의도된 것:
- `desktop/src/main.rs`, `desktop/src/services.rs`(신규), `src/App.tsx` — U1(앱 셸)
- `src/features/queue/*`(신규) — U3(인터뷰) 산출물
- `src/llm/client.rs`, `src/llm/mod.rs` — U1/U2(LLM 게이트웨이)
- U4 파일(`persona/*`, `u4-shared/*`, 뷰 4종)은 **수정하지 않음** — 포트 주입만으로 붙음

## 리스크 / 대응

| 리스크 | 대응 |
|---|---|
| `llm-http` feature에서 reqwest/OpenAI 파싱 회귀 | A3 단위 테스트로 응답 파싱 고정. 기본 feature 빌드는 CannedLlm이라 무영향 |
| 로컬 API 핸들 drop → 서버 즉사 | ServiceSet에 핸들 보관(B5). `dropping_the_handle...` 테스트가 동작 고정 |
| lock 후 서비스 stale 접근 | Services.clear() + 커맨드에서 None → Locked 게이트(B2/B3) |
| App.tsx 탭 추가로 기존 온보딩 회귀 | D3 테스트로 status 3분기 불변 확인 |
| 헤드리스 환경에서 tauri dev 실행 불가 | E5 대체 검증 + 사용자 수동 실행 안내 |

## 승인 요청

위 계획으로 진행할까요?

1. **Request Changes** — 범위·순서·설계 결정 수정
2. **Continue** — 이 계획대로 Code Generation Part 2(구현) 진행
