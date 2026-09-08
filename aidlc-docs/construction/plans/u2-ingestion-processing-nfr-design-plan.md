# U2 NFR Design Plan (Ingestion & Processing)

> Owner: Dev B · Branch: `construction/u2-ingestion-processing`
> 선행: NFR Requirements 승인. 대부분의 패턴(재시도·pending·마스킹·멱등)은 이미 결정됨.
> 이 단계 목적: 그 NFR을 **논리 컴포넌트/패턴**으로 U2 설계에 못박기.

## 산출물 체크리스트 (Step 6에서 생성)
- [x] `nfr-design/nfr-design-patterns.md` — P1~P9 패턴(마스킹 게이트웨이, 재시도+pending, 멱등 게이트, 오류 격리, 실행잠금, 투명성 로그, 커넥터 레지스트리, 결정적 마스킹)
- [x] `nfr-design/logical-components.md` — U2 논리 컴포넌트 11종 + 계약 매핑 + 관계도

## 카테고리 적용성 (증거 기반)
- **Resilience**: 적용 — 재시도/타임아웃, pending 큐, 소스 격리 (U2-NFR-REL/OFF)
- **Scalability**: 제한적 — 개인 PoC. "증분(전량 재스캔 지양)"만 해당 (U2-NFR-PERF3)
- **Performance**: 제한적 — 명시 SLA 없음 (U2-NFR-PERF)
- **Security**: 적용(최우선) — 마스킹 게이트웨이, 자격증명 Vault, 비저장 UnmaskMap (U2-NFR-SEC)
- **Logical Components**: 적용 — 큐(pending)·레지스트리·게이트 컴포넌트

---

## 명확화 질문 (남은 미세 결정만)

### Q1. 마스킹 게이트웨이의 강제(enforcement) 위치
"원문이 절대 마스킹 없이 LlmClient로 못 가게" 어떻게 구조적으로 강제할까요?
- A. U2 내부에 `LlmGateway` 래퍼를 두고, 텍스트 경로는 반드시 mask→log→LlmClient 순서를 통과(원문으로 직접 LlmClient 호출하는 경로를 코드상 제거). ★
- B. 규칙/리뷰로만 보장(별도 래퍼 없음)
- C. 기타(서술):

[Answer]:

### Q2. 재시도 파라미터 기본값 (U2-NFR-REL2)
- A. 타임아웃 30s, 최대 3회, 지수 백오프(1s→2s→4s), 그 후 pending 이월 ★
- B. 다른 값 지정 — 서술:
- C. 기타(서술):

[Answer]:

### Q3. pending 재개 트리거 (U2-NFR-OFF2)
- A. U1 Scheduler의 주기 tick에서 온라인이면 pending drain(수집 배치와 동일 주기에 편승) ★
- B. 온라인 복귀 이벤트 감지 시 즉시
- C. 기타(서술):

[Answer]:

### Q4. run-lock 범위 (동시성, BR-M2)
- A. 소스별 in-memory lock(프로세스 단일 인스턴스 가정) ★
- B. 저장소 기반 lock(다중 인스턴스 대비 — MVP 과함)
- C. 기타(서술):

[Answer]:
