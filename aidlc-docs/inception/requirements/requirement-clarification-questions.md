# knows-me 요구사항 후속 확인 질문

답변을 분석한 결과 두 가지만 더 확정하면 됩니다. 각 `[Answer]:` 뒤에 보기 letter를 적고 "완료"라고 알려주세요.

---

## 모호함 1: 페르소나 API 접근 제어 (Q6 재확인)
Q6에서 `D (Other)`를 고르셨는데 설명이 없었습니다. Q7=C(MVP는 로컬 내장 서버), Q8=A(MVP는 1인용)를 함께 보면 MVP 단계에서 제3자 원격 접근은 최소화되는 방향입니다. 이 전제에서 페르소나 API/챗의 접근 제어를 어떻게 할까요?

### Clarification Question 1
A) MVP는 로컬 전용(외부 노출 없음) — 접근 제어는 향후 확장으로. 로컬에서 동작 검증만 (Q7/Q8과 정합)

B) MVP부터 API 키 발급 방식 포함 — 로컬 서버라도 키로 호출자 인증

C) API 키 + scope(무엇을 대신 수행할지) 권한까지 MVP에 포함

D) Other (please describe after [Answer]: tag below)

[Answer] A

---

## 확인 1: 클라우드 LLM 사용과 로컬 전용 방침의 관계 (Q1 재확인)
Q1에서 **클라우드 LLM API 전용(B)** 을 고르셨습니다. 저장은 §3.5대로 로컬·암호화이지만, **가공/요약/비전/페르소나 추론 시 개인 데이터가 외부 LLM API로 전송**됩니다. 이 전송 정책을 어떻게 확정할까요?

### Clarification Question 2
A) 전부 전송 허용 — 편의·품질 우선(개인 PoC). 데이터가 외부 API로 나가는 것을 수용하고 그대로 진행

B) 소스별 전송 토글 — 기본 전송하되, 민감 소스(예: Gmail·개인 파일)는 사용자가 전송 제외 가능

C) 마스킹/최소화 후 전송 — 전송 전 식별정보·비밀 등을 최대한 제거하고 요약 요청

D) Other (please describe after [Answer]: tag below)

[Answer]: C
