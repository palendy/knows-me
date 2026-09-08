# U4 Functional Design Plan (Interface & Persona)

> Owner: Dev D · Branch: `construction/u4-interface-persona`
> 대상 스토리: US-5.1(대시보드), US-5.2(미니홈피), US-5.3(지식 그래프), US-6.1(페르소나 챗), US-6.2(로컬 API 초안)
> 의존(mock 준비됨): U1 `Masker`/`LlmClient`(공유 LLM 게이트웨이), U3 `KnowledgeApi`(읽기) — 개발 중에는 `knows_me_core::mocks::InMemoryKnowledge` 사용
> 제공: `PersonaApi` 구현체, 조회 command 3종, LocalApiServer(`POST /chat`, `POST /draft`)
> 활성 확장: Property-Based Testing (Partial — PBT-02/03/07/08/09 blocking)

## 유닛 경계 (중복 방지 — 사용자 지시)
| 영역 | 오너 | U4 조치 |
|---|---|---|
| `src-tauri/src/core/**`, `mocks.rs` | U1 | 읽기 전용, 수정 금지 |
| Tauri 셸(`tauri.conf.json`, `index.html`, `src/main.tsx`, `App.tsx`, 라우팅), 온보딩/잠금 UI, `vite.config.ts` | U1 | 생성 금지 (통합 시 U1이 마운트) |
| `ingestion/`, `processing/` | U2 | 접근 금지 |
| `knowledge/`, `interview/`, `src/features/queue/` | U3 | 접근 금지 — mock으로만 소비 |
| `src-tauri/src/persona/**` | **U4** | 신규 생성 |
| `src/features/{dashboard,minihome,graph,persona-chat,u4-shared}/**` | **U4** | 신규 생성 |
| `Cargo.toml`, `lib.rs` | 공용 | 추가만 (dependency, `pub mod persona;`) |
| `package.json`, `tsconfig.json`, `vitest.config.ts` | 미할당 | 테스트 툴체인만 추가 |

## 설계 산출물 체크리스트
- [x] `functional-design/domain-entities.md` — U4 내부 엔티티(PersonaContext, ContextSelection, GraphLayout, ViewModel)와 U1 공유 타입 관계
- [x] `functional-design/business-logic-model.md` — 조회 파이프라인 / 페르소나 챗 파이프라인 / 초안 파이프라인 / 로컬 API 요청 흐름
- [x] `functional-design/business-rules.md` — 맥락 선택 규칙, 마스킹 불변식, 근거 없음 처리, 그래프 필터, 로컬 전용 바인딩, 오프라인 저하
- [x] `functional-design/frontend-components.md` — 컴포넌트 계층, props/state, 상호작용 흐름, 어댑터 연동 지점
- [x] PBT-01: Testable Properties 식별 문서화 (각 산출물에 "Testable Properties" 절)

---

## 명확화 질문 & 결정

### Q1. 프론트엔드 스캐폴드 소유권
- **[Answer]**: 다른 유닛과 겹치지 않게 실행 → **테스트 툴체인만 U4가 추가**(`package.json`/`tsconfig.json`/`vitest.config.ts`). 앱 엔트리(`index.html`, `main.tsx`, `App.tsx`)와 번들러 설정(`vite.config.ts`)은 U1 앱 셸 소유로 남김. U4 컴포넌트는 마운트 방식에 무관하게 순수 export.

### Q2. 지식 그래프 렌더링 (US-5.3)
- **[Answer]**: **의존성 없는 자체 SVG**. 결정적(deterministic) radial/ring 레이아웃을 순수 함수로 구현 → 스냅샷·속성 테스트 가능, 번들 가벼움.

### Q3. 로컬 API 서버 HTTP 구현 (US-6.2)
- **[Answer]**: **axum + tokio**. Tauri가 이미 tokio 기반이며 라우팅/JSON 추출이 간결.

### Q4. 페르소나 맥락(PersonaContext) 구성 범위 — 결정 근거: FR-6.4, NFR-4
- **[Answer]**: **확정된(confirmed) Fact만** 사용. 질의 연관도 상위 N개(기본 12개)로 상한을 두어 토큰·지연을 통제. 미확정 후보는 절대 페르소나 근거로 쓰지 않음(US-3.x 확정 경로 존중).

### Q5. 마스킹 적용 지점 (US-6.1 AC2, D2, NFR-2)
- **[Answer]**: **PersonaService가 LLM 호출 직전에 Masker 적용**. 마스킹 대상 = 시스템 프롬프트(맥락 본문 포함) + 사용자 입력 **둘 다**. LLM 응답은 `UnmaskMap`으로 로컬 복원 후 반환. `UnmaskMap`은 요청 처리 중 메모리에만 존재(U2 Q5와 동일 정책).

### Q6. 근거가 없을 때의 응답 (US-6.1 AC1)
- **[Answer]**: **맥락이 비어 있으면 LLM을 호출하지 않고** "확정된 맥락이 없어 답할 수 없다"는 결정적 응답 반환. 환각 방지 + 불필요한 외부 전송 차단(NFR-2).

### Q7. 로컬 API 바인딩·노출 정책 (US-6.2 AC2, D7)
- **[Answer]**: **`127.0.0.1`에만 바인딩**(`0.0.0.0` 금지). MVP는 인증 없음(D7). 요청 `Host` 헤더가 loopback이 아니면 거부. 기본 포트 8765, 사용 중이면 상위 포트 탐색.

### Q8. 오프라인/LLM 실패 시 저하 동작 (NFR-3)
- **[Answer]**: 조회 3종(대시보드/미니홈피/그래프)은 **LLM 무관 — 항상 동작**. 챗/초안은 `AppError::External`을 그대로 전파하고 UI가 "오프라인/외부 오류" 상태로 표시(부분 저하). 로컬 API는 502로 매핑.

### Q9. 미니홈피 하이라이트 선정 규칙 (US-5.2)
- **[Answer]**: 확정 Fact 중 **링크 수(연결도) 내림차순 → 최근 확정일 내림차순 → 제목 사전순**으로 정렬해 상위 N개(기본 9개, 3x3 그리드). 전순서(total order)라 결과가 결정적.

### Q10. 대시보드 갱신 방식 (US-5.1 AC2)
- **[Answer]**: **명시적 새로고침 + 뷰 진입 시 조회(pull)**. MVP에서 push/구독 없음. `KnowledgeApi.dashboard()`가 항상 최신 스냅샷을 반환하므로 AC2 충족.
