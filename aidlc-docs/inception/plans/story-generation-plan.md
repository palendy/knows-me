# Story Generation Plan (knows-me)

**Role**: Product Owner
**목적**: 요구사항(`aidlc-docs/inception/requirements/requirements.md`)을 사용자 중심 스토리 + 수용 기준으로 변환.

---

## 실행 체크리스트 (승인 후 Part 2에서 수행)
- [x] `personas.md` 생성 — 사용자 아키타입과 특성
- [x] `stories.md` 생성 — INVEST 기준(독립·협상가능·가치·추정가능·작음·테스트가능) 준수
- [x] 각 스토리에 수용 기준(Acceptance Criteria) 포함 (Given-When-Then)
- [x] 페르소나 ↔ 스토리 매핑
- [x] MVP 범위(요구사항 §5.1) ↔ 스토리 추적성 표기
- [x] 승인된 breakdown 방식/포맷/세분도 반영 (Epic+Feature / GWT / MVP-only / 중간)

**확정 답변**: Q1=A, Q2=B(→충돌해소 A: 제3자 소비자는 페르소나만, 스토리는 MVP), Q3=A, Q4=A, Q5=B.

---

## 스토리 분해(Breakdown) 방식 옵션 (Step 5)
아래에서 하나(또는 하이브리드)를 선택합니다. 트레이드오프 참고:

- **User Journey-Based**: 사용자 흐름(수집→가공→인터뷰→조회→대화)을 따라 스토리 구성. 장점: 실제 사용 흐름 정렬. 단점: 기능 경계가 흐려질 수 있음.
- **Feature-Based**: 시스템 기능(수집/가공/DB/Queue/인터페이스/보안)별 구성. 장점: 요구사항 FR과 1:1, 개발 단위 명확. 단점: 사용자 흐름 가시성 낮음.
- **Persona-Based**: 사용자 유형별 그룹. 장점: 대상별 니즈 명확. 단점: 이 프로젝트는 1차 사용자가 대부분 소유자라 효과 제한적.
- **Domain-Based**: 업무 도메인별. 단점: 개인 도구라 도메인 분리 이점 적음.
- **Epic-Based**: 상위 Epic → 하위 스토리 계층. 장점: 큰 그림 + 추적성. FR/모듈 구조와 잘 맞음.

**PO 추천**: **Epic-Based + Feature 정렬** (Epic = FR 그룹, 하위에 사용자 스토리). Journey는 수용 기준/시나리오에 녹임.

---

## 확인 질문 (Step 3)
각 `[Answer]:` 뒤에 보기 letter를 적고, 맞는 보기가 없으면 마지막(Other)에 직접 설명해 주세요. 모두 답하면 "완료"라고 알려주세요.

## Question 1
스토리 분해 방식을 무엇으로 할까요?

A) Epic-Based + Feature 정렬 (PO 추천 — Epic=FR 그룹, 하위 사용자 스토리, Journey는 수용기준에 반영)

B) Feature-Based (FR과 1:1 평면 스토리)

C) User Journey-Based (사용 흐름 중심)

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 2
스토리에 등장시킬 **페르소나 범위**는?

A) 소유자(나) 1인만 — MVP 실사용자에 집중

B) 소유자(주) + 제3자 소비자(부, 향후) — 향후 확장 스토리도 별도 표기

C) 소유자 + 제3자 소비자 + 시스템 액터(수집 에이전트/페르소나 아바타)까지 명시

D) Other (please describe after [Answer]: tag below)

[Answer]: B)

## Question 3
**수용 기준(Acceptance Criteria) 형식**은?

A) Given-When-Then (BDD 형식) — 테스트/자동화 연계 좋음

B) 불릿 체크리스트 — 가볍고 읽기 쉬움

C) 혼합 — 핵심 시나리오는 Given-When-Then, 부가 조건은 체크리스트

D) Other (please describe after [Answer]: tag below)

[Answer]: A)

## Question 4
스토리 **범위**를 어디까지 작성할까요?

A) MVP(요구사항 §5.1)만 — 향후/범위 외는 제외

B) MVP 중심 + 향후 항목은 "Future" Epic으로 얇게 표기 (추적만)

C) 전체(향후 포함) 동일 상세도

D) Other (please describe after [Answer]: tag below)

[Answer]: A)

## Question 5
스토리 **세분도(크기)** 선호는?

A) 촘촘하게 — 작은 단위(각 화면/동작 단위), 개수 많아짐

B) 중간 — 기능 단위(예: "세션 수집", "인터뷰 응답")로 적당히 묶음 (권장)

C) 굵게 — Epic 수준 위주, 세부는 수용 기준으로

D) Other (please describe after [Answer]: tag below)

[Answer]: B)
