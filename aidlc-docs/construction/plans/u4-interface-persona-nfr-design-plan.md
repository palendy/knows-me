# U4 NFR Design Plan (Interface & Persona)

> 선행: U4 NFR Requirements 완료. 목적 = NFR을 충족하는 설계 패턴/논리 컴포넌트 확정.

## 체크리스트
- [x] NFR별 적용 패턴 매핑 (성능·저하·보안·테스트)
- [x] 논리 컴포넌트 분해 및 책임 정의
- [x] 컴포넌트 간 인터페이스 확정 (U1/U3 경계 포함)
- [x] 결정적 알고리즘 명세(선정·레이아웃·맥락)
- [x] 오류 매핑 표 확정
- [x] PBT 준수 설계(생성기·시드·회귀) 반영

## 결정
- **Q1. 마스킹 강제 방식** — [Answer]: 타입 강제. `LlmClient::chat(&str, &MaskedText)` 시그니처상 마스킹되지 않은 텍스트를 넘길 수 없다. 추가로 PBT-03으로 런타임 검증.
- **Q2. 오프라인 저하 구현** — [Answer]: 조회 경로에 `LlmClient` 의존성을 아예 주입하지 않는다(컴파일 타임 분리). 구조적으로 조회가 LLM에 의존할 수 없다.
- **Q3. 그래프 상한 처리** — [Answer]: `normalizeGraph` 단계에서 연결도 상위 N(500) 절단 후 dangling edge 제거. 절단 여부를 `truncated` 플래그로 UI에 알림.
- **Q4. 로컬 API 종료** — [Answer]: `tokio::sync::oneshot` 종료 신호 + axum graceful shutdown. `stop()`은 idempotent(두 번 호출해도 안전).
