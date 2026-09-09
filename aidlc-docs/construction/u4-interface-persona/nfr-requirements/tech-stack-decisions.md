# U4 Tech Stack Decisions (Interface & Persona)

> 제약: U1이 확정한 스택(Rust 단일 crate + React/TS)을 따른다. U4는 **자기 책임 범위에만** 의존성을 추가한다.

## 결정 요약
| # | 영역 | 결정 | 대안 | 채택 이유 |
|---|---|---|---|---|
| T1 | 로컬 HTTP 서버 | **axum 0.7 + tokio 1** | 직접 `std::net` 구현, actix-web, tiny_http | Tauri가 이미 tokio 기반 → 런타임 중복 없음. 라우팅/JSON 추출이 선언적이라 핸들러가 순수 로직에 집중. `std::net` 직접 구현은 HTTP 파싱·keep-alive를 직접 다뤄야 해 테스트/유지보수 비용이 큼 (사용자 확정) |
| T2 | 그래프 렌더 | **의존성 없는 자체 SVG** | react-force-graph, d3-force, cytoscape | 결정적 레이아웃을 순수 함수로 만들어 PBT 가능(BR-V4). 번들 경량, 라이선스/유지보수 부담 0. 개인 규모(≤500 노드)에 충분 (사용자 확정) |
| T3 | Rust PBT | **proptest 1** | quickcheck, arbitrary | PBT-09 권장 프레임워크. 커스텀 Strategy, 자동 shrinking, `proptest-regressions/` 회귀 파일로 재현성 확보(PBT-08) |
| T4 | TS PBT | **fast-check 3** | jsverify | PBT-09 권장. Vitest와 직접 통합, arbitrary 조합·shrinking·seed 출력 지원 |
| T5 | TS 테스트 러너 | **Vitest 2 + jsdom + @testing-library/react** | Jest | ESM/TS 네이티브, 설정 최소. RTL로 사용자 관점 예제 테스트(PBT-10 상보성) |
| T6 | 프론트엔드 프레임워크 | **React 18 + TypeScript 5** (U1 결정 승계, AD1) | — | Application Design AD1 |
| T7 | HTTP 직렬화 | **serde / serde_json** (U1 기존 의존성) | — | 공유 타입이 이미 `Serialize`/`Deserialize`. 추가 의존성 없음 |
| T8 | 비동기 런타임 | **tokio (rt-multi-thread)** | async-std | Tauri·axum 표준. U1 dev-dependency로 이미 사용 중 → 정식 dependency로 승격 |

## 추가되는 의존성 (Cargo.toml — 추가만, 기존 항목 미변경)
```toml
[dependencies]
axum = "0.7"                                    # T1 로컬 API 라우팅
tokio = { version = "1", features = ["rt-multi-thread", "net", "macros", "sync"] }  # T8
tower = "0.4"                                   # axum graceful shutdown 유틸

[dev-dependencies]
proptest = "1"                                  # T3 (PBT-09)
reqwest = { version = "0.12", default-features = false, features = ["json"] }  # 로컬 API 통합 테스트 클라이언트
```

## 추가되는 의존성 (package.json — U4가 신규 생성, 테스트 툴체인 한정)
```json
{
  "dependencies":    { "react": "^18", "react-dom": "^18" },
  "devDependencies": { "typescript": "^5", "vitest": "^2", "jsdom": "^25",
                       "@testing-library/react": "^16", "@testing-library/jest-dom": "^6",
                       "@vitejs/plugin-react": "^4", "fast-check": "^3",
                       "@types/react": "^18", "@types/react-dom": "^18" }
}
```
> **경계 주의**: `vite.config.ts`·`index.html`·`src/main.tsx`는 **생성하지 않는다**(U1 앱 셸 소유). U4는 `vitest.config.ts`만 두어 테스트 실행만 가능하게 한다. 번들러 선택권은 U1에 남는다.

## 기각한 선택지와 이유
- **actix-web**: 자체 런타임 색깔이 강해 Tauri의 tokio와 섞을 때 마찰. 기각.
- **d3-force**: 시뮬레이션이 비결정적(난수 초기화) → BR-V4(결정성)와 충돌하고 PBT 불가. 기각.
- **Jest**: TS/ESM 설정 부담이 Vitest 대비 큼. 기각.
- **quickcheck(Rust)**: shrinking 품질과 회귀 파일 지원이 proptest보다 약함. 기각.

## PBT 프레임워크 준수 확인 (PBT-09)
| 요구 | proptest | fast-check |
|---|---|---|
| 커스텀 생성기 | ✅ `Strategy`, `prop_compose!` | ✅ `fc.record`, `fc.constantFrom` |
| 자동 shrinking | ✅ | ✅ |
| seed 재현 | ✅ `PROPTEST_SEED` + 회귀 파일 | ✅ 실패 시 seed 출력 → `fc.assert(..., {seed})` |
| 테스트 러너 통합 | ✅ `cargo test` | ✅ Vitest |
| 의존성에 포함 | ✅ `[dev-dependencies]` | ✅ `devDependencies` |
