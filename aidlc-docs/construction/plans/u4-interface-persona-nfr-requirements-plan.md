# U4 NFR Requirements Plan (Interface & Persona)

> Owner: Dev D · 선행: U4 Functional Design 완료
> 근거 NFR: NFR-1(플랫폼), NFR-2(프라이버시), NFR-3(오프라인 저하), NFR-4(규모/반응성), NFR-6(이식성), NFR-8(PBT Partial)

## 체크리스트
- [x] 성능/반응성 목표 정의 (조회 3종, 챗/초안, 그래프 렌더)
- [x] 확장·규모 가정 정의 (1인, 수천~수만 사실)
- [x] 가용성/저하 정책 정의 (오프라인, LLM 실패, 잠금)
- [x] 보안 요구 정의 (로컬 바인딩, 마스킹, 시크릿, 로깅)
- [x] 유지보수/테스트 요구 정의 (PBT 프레임워크 선정 — PBT-09)
- [x] 사용성 요구 정의 (빈 상태, 오류 상태, 접근성)
- [x] Tech stack 결정 문서화

## 명확화 질문 & 결정

### Q1. 조회 응답 목표 (NFR-4)
- **[Answer]**: 로컬 데이터 1만 사실 기준 **대시보드/미니홈피 p95 < 300ms**, **그래프 조회+레이아웃 p95 < 500ms**(노드 500개 상한, 초과 시 연결도 상위 500개만 표시 + 안내). 개인 규모 가정이라 서버 확장 요구는 없음.

### Q2. 페르소나 응답 목표 (NFR-4, NFR-3)
- **[Answer]**: 외부 LLM 지연이 지배적이므로 **U4 자체 오버헤드 p95 < 100ms**(맥락 조합+마스킹+복원)만 목표로 하고, 전체 응답 시간은 SLO에서 제외. LLM 호출 타임아웃 **30초** 후 `External` 오류.

### Q3. 동시성 (로컬 API)
- **[Answer]**: 1인 사용 전제. **동시 요청 8개**까지 정상 처리(tokio 멀티스레드 런타임 기본), 초과분은 큐잉. 부하 테스트는 범위 외.

### Q4. 가용성/저하 (NFR-3)
- **[Answer]**: 조회 3종은 **LLM 없이 100% 동작**(하드 요구). 챗/초안만 네트워크 의존. 앱 재시작 외 복구 절차 없음(로컬 앱).

### Q5. 보안 (NFR-2, D7, 심사 기준 ⑥)
- **[Answer]**: (a) 리스너 `127.0.0.1` 고정, (b) 비-loopback Host 403, (c) 모든 LLM 전송 전 마스킹, (d) 시크릿 하드코딩 금지 — LLM 자격증명은 U1 Vault/환경변수, (e) 사실 본문·UnmaskMap 로깅 금지, (f) 오류 응답에 내부 경로 미포함.

### Q6. PBT 프레임워크 (PBT-09, NFR-8)
- **[Answer]**: Rust = **proptest**, TypeScript = **fast-check**. 둘 다 커스텀 생성기·shrinking·seed 재현을 지원.

### Q7. PBT 재현성 (PBT-08)
- **[Answer]**: proptest는 실패 케이스를 `proptest-regressions/`에 자동 기록(레포에 커밋) + `PROPTEST_SEED`로 재현. fast-check는 실패 시 seed를 출력하고 회귀 케이스를 예제 테스트로 승격. shrinking 비활성화 금지.

### Q8. 브라우저/플랫폼 (NFR-1)
- **[Answer]**: Tauri WebView(WebKit/WebView2)만 대상. 레거시 브라우저 지원 없음. ES2020 타깃.

### Q9. 이식성 (NFR-6)
- **[Answer]**: U4는 자체 영속 저장을 하지 않는다(모든 저장은 U3/U1 경유). 따라서 저장 포맷 요구는 U4에 N/A.

### Q10. 접근성/사용성 (심사 기준 ⑤)
- **[Answer]**: 키보드 조작 가능(그래프 노드 포함), 모든 뷰에 loading/empty/error 상태 명시, 한국어 UI 문구.
