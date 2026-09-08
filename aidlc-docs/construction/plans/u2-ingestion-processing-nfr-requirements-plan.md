# U2 NFR Requirements Plan (Ingestion & Processing)

> Owner: Dev B · Branch: `construction/u2-ingestion-processing`
> 선행: Functional Design 승인 완료. 상위 NFR(요구사항서 §5): NFR-2 프라이버시, NFR-3 오프라인, NFR-5 멱등, NFR-7 모듈확장.
> 활성 확장: Property-Based Testing (Partial — PBT-09 프레임워크 선정 **이 단계에서 blocking**)

## NFR 평가 체크리스트 (Step 6에서 생성)
- [ ] `nfr-requirements/nfr-requirements.md` — U2 관점 성능/신뢰성/보안·프라이버시/오프라인/유지보수 NFR
- [ ] `nfr-requirements/tech-stack-decisions.md` — Rust crate 선정(HTTP·직렬화·파일감시·PDF/DOCX·PBT 프레임워크) + 근거
- [ ] PBT-09: PBT 프레임워크 선정·문서화(Rust=proptest, TS=fast-check) — dependency로 명시

---

## 명확화 질문 (해당 [Answer]: 태그를 채워주세요)

애매하면 추천안(★)을 기본으로 진행합니다.

### Q1. PBT 프레임워크 (PBT-09, blocking)
U2는 Rust 백엔드 로직 중심. 마스킹 왕복·멱등 속성 테스트 대상.
- A. Rust=`proptest` (매크로 기반, shrinking·seed 재현). TS 계약 미러 검증이 필요해지면 `fast-check` 추가 ★
- B. Rust=`quickcheck`
- C. 기타(서술):

[Answer]:

### Q2. 성능 목표 (개인 PoC 규모)
수집·가공 처리량/지연 목표를 어느 수준으로 잡을까요?
- A. 명시적 SLA 없음. "체감 무한정 대기 없음"만 목표(배치는 백그라운드, 수동 트리거는 진행표시). LLM 호출 지연은 외부 요인으로 간주 ★
- B. 구체 수치 지정(예: 1회 배치 N건 M초 이내) — 서술:
- C. 기타(서술):

[Answer]:

### Q3. LLM 호출 신뢰성(재시도/타임아웃)
외부 LLM 호출 실패 시 U2 동작은?
- A. 타임아웃 + 제한적 재시도(지수 백오프 소수 회) 후 실패 시 pending으로 이월(BR-O). 앱은 계속 ★
- B. 재시도 없이 즉시 pending 이월
- C. 기타(서술):

[Answer]:

### Q4. Notion/Gmail HTTP 클라이언트·인증 방식
- A. `reqwest`(async) + 각 API의 OAuth2 토큰(CredentialStore 보관). 토큰 갱신은 커넥터 내부 처리 ★
- B. 최소 의존(std/hyper 직접) — 서술:
- C. 기타(서술):

[Answer]:

### Q5. 파일 파싱 라이브러리 범위 (US-1.5, BR-F1)
오피스/PDF 파싱을 MVP에서 어디까지 실제 구현?
- A. 텍스트/MD=네이티브, PDF=`pdf-extract`(또는 유사), DOCX=`docx-rs`(또는 유사). 실패 시 스킵+사유기록 ★
- B. MVP는 텍스트/MD + 이미지(비전)만 실제 구현, PDF/DOCX는 인터페이스만(향후) — 서술:
- C. 기타(서술):

[Answer]:

### Q6. 폴더 감시 방식 (US-1.5 AC1)
- A. `notify` crate(크로스플랫폼 파일시스템 이벤트) + 주기 배치 폴백 ★
- B. 주기 폴링만(이벤트 감시 없음)
- C. 기타(서술):

[Answer]:

### Q7. 스케줄러 소유권 (US-1.1 배치 주기)
주기적 배치를 도는 스케줄러는?
- A. U1 셸의 Scheduler가 U2 `IngestionApi.trigger`를 주기 호출(U2는 스케줄러를 소유하지 않고 계약만 노출) ★
- B. U2가 자체 스케줄러 소유
- C. 기타(서술):

[Answer]:

### Q8. 마스킹 강도 정책 (NFR-2, R1 — 유출 리스크)
마스킹 규칙 커버리지 목표는?
- A. 결정적 규칙 기반(정규식: 이메일·전화·토큰/키 패턴·URL 자격증명 + 사용자 사전(이름 등)). 미검출은 로깅해 규칙 보강. LLM 기반 PII 탐지는 향후 ★
- B. 규칙 + LLM 기반 PII 탐지 병행(비용·오프라인 트레이드오프) — 서술:
- C. 기타(서술):

[Answer]:
