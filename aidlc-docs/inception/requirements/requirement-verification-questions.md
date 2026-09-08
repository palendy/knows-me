# knows-me 요구사항 확인 질문

`requirements/REQUIREMENTS.md`를 검토했습니다. 대부분 명확하지만, 설계로 넘어가기 전에 확정해야 할 항목들이 있습니다.
각 질문의 `[Answer]:` 태그 뒤에 보기 letter(A/B/C…)를 적어주세요. 맞는 보기가 없으면 마지막 보기(Other)를 고르고 직접 설명해 주세요.
모두 답하시면 "완료" 라고 알려주세요.

---

## A. 아키텍처 · 기술

## Question 1
LLM(요약·분류·인터뷰·비전 인식·페르소나 대화)을 **어디서 실행**하나요? "로컬 전용·암호화" 보안 방침과 직접 충돌할 수 있어 가장 먼저 확정이 필요합니다.

A) 로컬 LLM 전용 — 모든 추론을 로컬 모델로. 개인 데이터가 기기를 절대 벗어나지 않음 (비전/품질은 로컬 모델 성능에 종속)

B) 클라우드 LLM API 전용 — Claude 등 외부 API 사용. 품질·비전 우수하나 가공 대상 데이터가 외부로 전송됨

C) 하이브리드 — 민감도 낮은 작업/비전은 클라우드, 민감 데이터 가공은 로컬. 사용자가 소스별로 전송 허용 여부 설정

D) Other (please describe after [Answer]: tag below)

[Answer]: B

## Question 2
Knowledge DB의 **저장 엔진**을 무엇으로 시작할까요? (요구사항: "지식 그래프를 목표로 하되 스키마는 구현하며 고도화")

A) 임베디드 관계형(SQLite) + 그래프 뷰는 앱에서 계산해 시각화 — 단순·이식성 좋음, 로컬 앱에 적합

B) 임베디드 그래프 DB — 처음부터 노드/엣지 네이티브 저장

C) 파일 기반(사실 하나 = 마크다운/JSON 파일 하나) + 인덱스 — 사람이 직접 열람/편집 쉬움

D) Other (please describe after [Answer]: tag below)

[Answer]: LLM위키 방식이 좋은데 굳이 유사한 걸 고르자면 C

## Question 3
데스크탑 앱의 **대상 OS 플랫폼**은? (Tauri 빌드 타깃과 폴더 감시·파일 경로 구현에 영향)

A) Windows 우선 (개발/테스트 주 환경)

B) macOS 우선

C) Windows + macOS 동시 지원

D) Windows + macOS + Linux 전부

X) Other (please describe after [Answer]: tag below)

[Answer]: D (만약 과하면 하지말고)

---

## B. 소스 수집

## Question 4
에이전트 세션(§3.1.1) 수집을 **언제** 트리거할까요? (MVP 1순위 소스)

A) 주기적 배치 — 앱이 일정 간격으로 세션 저장 위치를 스캔해 신규분 증분 수집

B) 파일 감시 — 세션 트랜스크립트 파일 변경을 실시간 감지해 수집

C) 수동 트리거 — 사용자가 앱에서 "지금 수집" 버튼을 누를 때만

D) Other (please describe after [Answer]: tag below)

[Answer]: A > B > C 우선순위 순서로

## Question 5
외부 시스템(§3.1.2) 중 **MVP에서 첫 번째로 연동**할 시스템은? (나머지는 순차 확장)

A) Jira (담당 이슈)

B) Confluence (팀 문서)

C) Notion

D) Gmail (내가 주고받은 메일)

X) Other (please describe after [Answer]: tag below)

[Answer]: C, D

---

## C. 인터페이스 · 보안

## Question 6
페르소나를 노출하는 **REST API / 챗 인터페이스(타인·제3자 소비)** 의 접근 제어는? ("나 대신 네트워킹/회의" 기능이 최우선이므로 인증 방식 확정 필요)

A) API 키 기반 — 발급한 키로 제3자/외부 에이전트 인증

B) 토큰 기반(OAuth 유사) + 범위(scope)별 권한 — 무엇까지 대신하게 할지 제한

C) 로컬 전용 — 같은 기기/내부 네트워크에서만 접근, 외부 노출 없음(MVP는 최소화)

D) Other (please describe after [Answer]: tag below)

[Answer]: D

## Question 7
REST API와 페르소나 챗을 제공하려면 **서버 컴포넌트**가 필요합니다. 어디서 실행하나요?

A) 데스크탑 앱 내장 로컬 서버 — 앱 실행 중에만 로컬 포트로 제공(외부 노출은 사용자가 켤 때만)

B) 별도 배포 가능한 서버 — 앱과 분리해 상시 구동/원격 접근 지원

C) MVP는 A(로컬 내장)로, 원격 배포는 향후 확장

D) Other (please describe after [Answer]: tag below)

[Answer]: C

## Question 8
"권한 부여 다중 사용자"(요구사항 §5)는 MVP 범위인가요, 설계만 대비인가요? (§4는 "3장 기능 전부"라 했고 §1.3/§5는 "추후"라 해서 확인)

A) MVP 제외 — 지금은 나 1인용으로 구현, 데이터 모델만 다중 사용자 확장 가능하게 설계

B) MVP 포함 — 처음부터 권한 기반 접근 제어까지 구현

C) Other (please describe after [Answer]: tag below)

[Answer]: A

---

## D. 익스텐션 적용 여부

## Question: 보안(Security) 익스텐션
이 프로젝트에 보안 익스텐션 규칙을 강제할까요?

A) 예 — 모든 SECURITY 규칙을 차단(blocking) 제약으로 강제 (프로덕션급 애플리케이션 권장)

B) 아니오 — SECURITY 규칙 전체 건너뜀 (PoC·프로토타입·실험 프로젝트에 적합)

X) Other (please describe after [Answer]: tag below)

[Answer]: B

## Question: 복원력(Resiliency) 익스텐션
복원력 베이스라인을 이 프로젝트에 적용할까요?

**이 익스텐션은** AWS Well-Architected(신뢰성 기둥) 기반의 **설계 시점 방향성 모범사례**를 요구사항·설계·코드에 반영합니다(내결함성·고가용성·관측성·복구성 등). **프로덕션 준비 완료를 보장하지 않으며**, 정식 리뷰의 대체물이 아닌 **좋은 출발점**입니다.

A) 예 — 복원력 베이스라인을 설계 지침으로 적용 (업무상 중요한 워크로드에 권장)

B) 아니오 — 복원력 베이스라인 건너뜀 (빠른 반복이 우선인 PoC·프로토타입에 적합)

X) Other (please describe after [Answer]: tag below)

[Answer]: B

## Question: 속성 기반 테스트(Property-Based Testing) 익스텐션
속성 기반 테스트(PBT) 규칙을 이 프로젝트에 강제할까요?

A) 예 — 모든 PBT 규칙을 차단 제약으로 강제 (비즈니스 로직·데이터 변환·직렬화·상태 저장 컴포넌트가 있는 프로젝트 권장)

B) 부분 — 순수 함수와 직렬화 왕복(round-trip)에만 PBT 적용 (알고리즘 복잡도가 제한적인 프로젝트에 적합)

C) 아니오 — PBT 규칙 전체 건너뜀 (단순 CRUD·UI 전용·얇은 통합 계층에 적합)

X) Other (please describe after [Answer]: tag below)

[Answer]: B
