# Application Design Plan (knows-me)

**목적**: 요구사항·스토리를 바탕으로 **고수준 컴포넌트/서비스/의존성**을 정의(상세 비즈니스 로직은 이후 Functional Design).

---

## 실행 체크리스트 (승인 후 생성)
- [ ] `components.md` — 컴포넌트 정의·책임·인터페이스
- [ ] `component-methods.md` — 메서드 시그니처(고수준 목적·입출력)
- [ ] `services.md` — 서비스 정의·오케스트레이션
- [ ] `component-dependency.md` — 의존성 매트릭스·통신 패턴·데이터 흐름
- [ ] `application-design.md` — 위 문서 통합본
- [ ] 설계 완전성·일관성 검증

---

## 예비 컴포넌트 지도 (초안 — 질문 답변 후 확정)
스토리 Epic과 정렬한 후보 컴포넌트(모두 로컬):
- **Ingestion**: SessionConnector, NotionConnector, GmailConnector, FileWatcher/Uploader (플러그형)
- **Processing**: Summarizer/Classifier, **Masker**(전송 전 마스킹), LlmClient(클라우드)
- **Knowledge**: FactStore(파일 위키), HistoryTracker, SearchIndex
- **Interview**: QueueManager(확인형/심화형 생성), AnswerIntake(전환 없는 UX 지원)
- **Interface(Frontend)**: Dashboard, MiniHomeView, GraphView, PersonaChat
- **PersonaService + LocalApiServer**: 페르소나 응답, "나 대신 네트워킹" 로컬 REST
- **Security**: KeyManager(비밀번호→키유도), Vault(암호화 저장/자격증명)

---

## 확인 질문 (Step 4)
각 `[Answer]:` 뒤에 보기 letter를 적어주세요. 맞는 게 없으면 마지막(Other)에 직접 설명. 모두 답하면 "완료".

## Question 1
프런트엔드(웹 UI) 프레임워크는? (Tauri의 웹뷰에서 동작)

A) React + TypeScript (생태계·그래프/차트 라이브러리 풍부 — 추천)

B) Svelte / SvelteKit (경량·빠름)

C) Vue 3 + TypeScript

D) Solid.js

E) Other (please describe after [Answer]: tag below)

[Answer]: A) 

## Question 2
백엔드(코어 로직) 구성은? (Tauri 코어는 Rust)

A) Rust 코어 단일 — 수집/가공/저장/암호화/서버 모두 Rust로 구현, 프런트는 Tauri command로 호출 (단순·이식성·성능 — 추천)

B) Rust 코어 + 사이드카(Node/Python) — 커넥터·LLM SDK 편의를 위해 별도 프로세스 병행 (SDK 풍부하나 복잡도↑)

C) Other (please describe after [Answer]: tag below)

[Answer]:  A)

## Question 3
소스 커넥터 구조는? (세션→Notion·Gmail→파일 순차 확장 요구)

A) 공통 Connector 인터페이스(trait) + 소스별 구현 (플러그형, 확장 쉬움 — 추천)

B) 소스별 개별 모듈(공통 추상화 없이 직접 구현)

C) Other (please describe after [Answer]: tag below)

[Answer]: A)

## Question 4
페르소나 로컬 API 서버의 실행 형태는? (요구사항 D8=앱 내장 로컬)

A) Tauri 코어(Rust) 내장 HTTP 서버 — 앱 실행 중 localhost 포트로 제공 (추천)

B) 별도 사이드카 프로세스로 서버 구동

C) Other (please describe after [Answer]: tag below)

[Answer]: A)

## Question 5
컴포넌트 간 통신/결합 방식 선호는?

A) 계층형 + 서비스 오케스트레이션 — 각 도메인 서비스가 컴포넌트를 조율, 프런트는 명령/조회만 (추천)

B) 이벤트 기반 — 수집·가공·Queue를 이벤트/메시지로 느슨히 연결 (비동기 유리, 복잡도↑)

C) 혼합 — 수집·가공 파이프라인은 이벤트, 나머지는 계층형

D) Other (please describe after [Answer]: tag below)

[Answer]: A)
