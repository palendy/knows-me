# U4 Business Logic Model (Interface & Persona)

> 기술 중립 흐름 정의. 구현 위치는 Code Generation 단계에서 확정.

## 파이프라인 개요
```text
  [P1] 조회 파이프라인   (US-5.1/5.2/5.3)  : Knowledge --> DTO --> ViewModel --> SVG/DOM
  [P2] 페르소나 챗       (US-6.1)          : Knowledge --> Context --> Mask --> LLM --> Unmask
  [P3] 초안 작성         (US-6.2)          : Knowledge --> Context(+kind) --> Mask --> LLM --> Unmask
  [P4] 로컬 API          (US-6.2)          : HTTP(127.0.0.1) --> P2/P3 위임 --> JSON
```

---

## P1. 조회 파이프라인 (US-5.1, US-5.2, US-5.3)

### P1-a. 대시보드 (US-5.1)
1. 뷰 진입 또는 새로고침 → `get_dashboard()` 호출
2. `KnowledgeApi.dashboard()` → `DashboardDto { collected_count, pending_queue, recent_facts }`
3. 그대로 렌더. **집계 로직은 U3 소유** — U4는 재계산하지 않는다(중복 방지).
4. 오류 시: 수치 대신 오류 상태 표시, 재시도 버튼 제공

> AC2("상태 변화가 반영") 충족 근거: 매 조회가 최신 스냅샷을 pull 하므로 별도 구독 없이 최신값이 보인다.

### P1-b. 미니홈피 (US-5.2)
1. `KnowledgeApi.search("", FactFilter::default())` → 사실 요약 목록
2. `KnowledgeApi.graph(GraphFilter::default())` → 엣지 전체에서 연결도 집계
   (사실마다 `get()`을 부르면 사실 수만큼 왕복이 발생해 U4-NFR-P2를 깬다 — 호출은 총 2회로 고정)
3. **하이라이트 선정**(BR-V2): 확정 사실만 → `(연결도 desc, 제목 asc, id asc)` 정렬 → 상위 `limit`(기본 9)
3. `MiniHomeDto { highlights }` 구성 → 3x3 그리드 카드로 렌더(프로필 영역 + 하이라이트 + 범위 배지)

### P1-c. 지식 그래프 (US-5.3)
1. `KnowledgeApi.graph(GraphFilter{ scope })` → `GraphDto { nodes, edges }`
2. **정규화**: 양 끝이 모두 존재하는 엣지만 유지(C9), 중복 엣지 제거, 무방향 취급 시 `(min,max)` 정규화
3. **레이아웃**(순수 함수, 결정적): 연결도 내림차순 정렬 → 동심 링 배치
   - 링 0(중심): 연결도 최상위 1개
   - 링 k: 반지름 `r_k = r_step * k`, 각 링에 최대 `6k`개, 각도 `2*pi*i/n_k`
   - 좌표를 뷰박스 안으로 클램프(C8)
4. SVG 렌더: 엣지=`<line>`, 노드=`<circle>`+`<text>`
5. **탐색**: 노드 클릭 → 선택 노드와 그 이웃만 강조(focus). 재클릭 시 해제. 선택 상태는 뷰 로컬 state.

---

## P2. 페르소나 챗 파이프라인 (US-6.1)

```text
prompt
  |
  v
(1) build_context(prompt, ContextSelection)
      - KnowledgeApi.search(prompt, filter) -> 타깃 후보
      - confirmed == true 인 것만 남김            [BR-P1]  <-- 개수 판단보다 먼저
      - KnowledgeApi.search("", filter) -> 확정 모집단
          * total_confirmed = 모집단 크기          [E1]
          * 타깃 후보 < max_facts 이면 모집단으로 넓힘 (recall 폴백)
      - 제목 연관도 내림차순으로 pre-rank 후 fetch_cap(64) 절단
          (id 순 절단 금지 — 연관 사실 유실)
      - 남은 후보만 get() 으로 본문 확보
      - 제목+본문 연관도 내림차순, 동점은 confirmed_at desc -> FactId asc
      - 상위 max_facts 개 절단                    [C2]
  |
  +-- entries.is_empty()? --yes--> "확정된 맥락이 없습니다" 반환, LLM 호출 없음  [BR-P4]
  |
  no
  v
(2) render PersonaPrompt
      system        = 페르소나 지시문 (+초안 형식 지시). 정적, 소유자 데이터 없음
      user_document = 맥락 본문(제목/본문/범위) + 소유자 질문
  |
  v
(3) Masker.mask(user_document) -> (MaskedText, UnmaskMap)   [BR-P2, 호출 1회]
  |
  v
(4) LlmClient.chat(masked_system, masked_user)              [외부 전송 지점]
  |
  +-- Err(External) --> 그대로 전파 (오프라인 저하)          [BR-P6]
  |
  v
(5) Masker.unmask(reply, UnmaskMap) -> 원문 복원 (로컬)     [BR-P3]
  |
  v
(6) PersonaReply { text }  ; UnmaskMap 폐기(메모리 밖으로 나가지 않음)
```

**호출 순서 불변식**: (3)은 항상 (4)보다 먼저 실행된다. `LlmClient`가 받는 가변 인자는 반드시 `MaskedText`다 — 타입 시스템이 이를 강제한다(`LlmClient::chat(&str, &MaskedText)`). `&str`로 가는 `system`은 정적 템플릿이므로 마스킹할 소유자 데이터가 없다(BR-P2a).

---

## P3. 초안 작성 파이프라인 (US-6.2)

P2와 동일하되 두 지점이 다르다.

1. **맥락 선택**: `DraftKind`에 따라 기본 scope 힌트를 적용
   - `Email`/`Message` → 제한 없음(None)
   - `Post` → 제한 없음(None)
   - (scope를 강제하지 않는 이유: MVP 사실 대부분이 `Unknown`이며, 잘못된 필터가 "근거 없음"을 과다 유발)
2. **지시문**: 종류별 출력 형식 지시를 system에 추가
   - `Email` → 제목 줄 + 본문, 격식 있는 어조
   - `Message` → 짧은 메신저 톤, 인사 최소
   - `Post` → 공개 게시글 톤, 1인칭
3. 나머지(마스킹 → LLM → 복원 → 반환)는 P2와 동일. 반환 타입은 `Draft { text }`.

---

## P4. 로컬 API 파이프라인 (US-6.2 AC1/AC2)

```text
클라이언트(로컬 프로세스)
  |  HTTP POST 127.0.0.1:<port>/chat | /draft
  v
(1) 바인딩 검사: 리스너는 127.0.0.1 에만 바인딩됨          [BR-A1]
(2) Host 헤더 검사: loopback 이 아니면 403                  [BR-A2]
(3) JSON 역직렬화 실패 -> 400 InvalidInput
(4) PersonaService.chat / .draft 로 위임 (P2/P3 재사용)     [BR-A3]
(5) 결과 매핑
      Ok            -> 200 { "text": ... }
      Locked        -> 423
      InvalidInput  -> 400
      NotFound      -> 404
      External      -> 502   (LLM/네트워크 실패)
      그 외          -> 500
```

- **수명**: `start(port)` → 리스너 오픈, `stop()` → graceful shutdown. 앱이 실행 중일 때만 살아 있다(FR-7.1).
- **포트**: 기본 8765. 사용 중이면 8765..8785 범위에서 첫 가용 포트. 실제 바인딩 포트를 반환해 UI가 표시.
- **인증 없음**(D7, MVP). 원격 노출·API 키는 FR-7.3으로 범위 외.

---

## 프론트엔드 데이터 접근 (어댑터)

U1의 Tauri command 계층이 아직 없으므로, U4는 **포트/어댑터**로 분리한다.

```text
  React 뷰  --> KnowsMeApi (interface)  --+--> TauriApi   (통합 후 U1 command 호출)
                                          +--> MockApi    (개발/테스트용, 결정적 픽스처)
```

- 뷰는 `KnowsMeApi`만 알고 구현체를 모른다 → U1 통합 시 어댑터 1개 교체로 끝난다.
- `MockApi`는 U1 `mocks.rs`의 의미(확정 사실만 노출, 이력 보존)를 그대로 따르는 결정적 픽스처를 쓴다.

## Testable Properties (PBT-01)
- P1-b 하이라이트 선정: 입력 순열 불변(전순서), 부분집합성, 길이 상한 → **PBT-03**
- P1-c 레이아웃: 노드 보존·경계 내 좌표·결정성·dangling edge 제거 → **PBT-03**
- P2/P3 맥락 조합: 확정만·상한·중복 없음 → **PBT-03**
- P2/P3 마스킹 경유: 생성된 임의 사실/질문에 대해 LLM 인자에 원문 식별자 미포함 → **PBT-03**
- P4 JSON DTO: 왕복 무손실 → **PBT-02**
