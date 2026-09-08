# U4 NFR Requirements (Interface & Persona)

> ID: `U4-NFR-*`. 상위 근거는 `requirements.md` §4 (NFR-1~8).

## 성능 / 반응성
| ID | 요구 | 목표 | 근거 | 검증 |
|---|---|---|---|---|
| U4-NFR-P1 | 대시보드 조회 | 1만 사실 기준 p95 < 300ms | NFR-4 | mock 벤치(예제 테스트) |
| U4-NFR-P2 | 미니홈피 조회+선정 | p95 < 300ms. 사실 수와 무관하게 **백엔드 호출 2회**(`search("")` + `graph()`)로 고정 — 사실당 `get()` 금지 | NFR-4 | 선정 함수 단위 벤치 |
| U4-NFR-P3 | 그래프 조회+레이아웃 | 노드 ≤500 기준 p95 < 500ms | NFR-4 | 레이아웃 순수 함수 벤치 |
| U4-NFR-P4 | 그래프 노드 상한 | 500개 초과 시 연결도 상위 500개만 렌더 + 안내 배너 | NFR-4(반응성) | 예제 테스트 |
| U4-NFR-P5 | 페르소나 U4 오버헤드 | 맥락 조합+마스킹+복원 p95 < 100ms (LLM 대기 제외). `get()` 호출은 `fetch_cap`(64)회 이하이며, **연관도 기준으로 상위를 남긴 뒤** 절단한다(id 순 절단 금지 — 연관 사실이 조용히 버려진다) | NFR-4 | 단위 측정 |
| U4-NFR-P6 | LLM 타임아웃 | 30초 초과 시 `AppError::External` | NFR-3 | 예제 테스트(느린 mock) |

## 규모 / 확장
| ID | 요구 | 근거 |
|---|---|---|
| U4-NFR-S1 | 1인 사용 전제. 수평 확장·멀티테넌시 요구 없음 | NFR-4, 범위 외(D9) |
| U4-NFR-S2 | 맥락 사실 수 상한 12(설정 가능) — 데이터가 커져도 LLM 호출 크기가 선형 증가하지 않음 | NFR-4 |
| U4-NFR-S3 | 로컬 API 동시 요청 8개까지 정상 처리, 초과는 큐잉 | 1인 사용 |

## 가용성 / 저하
| ID | 요구 | 근거 |
|---|---|---|
| U4-NFR-A1 | **하드 요구**: 대시보드·미니홈피·그래프는 네트워크 없이 100% 동작 | NFR-3 |
| U4-NFR-A2 | 챗/초안 실패는 뷰 단위로 국소화 — 다른 뷰에 영향 없음 | NFR-3, BR-E2 |
| U4-NFR-A3 | 잠금 상태에서는 조회·페르소나 모두 `Locked`로 거부하고 안내 표시 | FR-5.3, BR-E1 |
| U4-NFR-A4 | 로컬 API는 앱 수명과 동일. DR/페일오버 요구 없음(로컬 데스크탑) | FR-7.1, D8 |

## 보안 / 프라이버시
| ID | 요구 | 근거 | 검증 |
|---|---|---|---|
| U4-NFR-SEC1 | 로컬 API 리스너는 `127.0.0.1`에만 바인딩 | D7, US-6.2 AC2 | 통합 테스트(외부 IF 바인딩 부재 확인) |
| U4-NFR-SEC2 | 비-loopback `Host` 헤더 요청은 403 | D7 | 예제 테스트 |
| U4-NFR-SEC3 | 모든 외부 LLM 전송은 `Masker` 통과. 소유자 데이터를 `user_document` 한 곳에 모아 **단일 `mask()` 호출**로 처리하고 `system`은 소유자 데이터 없는 정적 템플릿으로 유지한다(BR-P2/P2a) | D2, NFR-2, R1 | PBT-03 + 타입 강제 + `system_prompt_carries_no_owner_data` |
| U4-NFR-SEC4 | `UnmaskMap`·사실 본문을 로그·오류 메시지에 포함하지 않음 | NFR-2 | 코드 리뷰 + 예제 테스트 |
| U4-NFR-SEC5 | 시크릿 하드코딩 금지 — LLM 자격증명은 U1 Vault/환경변수 | 심사 기준 ⑥ | grep 기반 검사 |
| U4-NFR-SEC6 | 오류 응답에 파일 경로·스택 트레이스 미포함 | 정보 노출 방지 | 예제 테스트 |
| U4-NFR-SEC7 | 마스킹 적용 사실을 UI에 상시 고지(투명성) | NFR-2 | RTL 테스트 |

> **주의**: Security Baseline 확장은 이 프로젝트에서 **비활성(Enabled=No)** 이다. 위 항목들은 확장 규칙이 아니라 **요구사항 문서(NFR-2, D7)에서 직접 유래한** U4 자체 요구다.

## 신뢰성
| ID | 요구 |
|---|---|
| U4-NFR-R1 | 모든 실패는 `AppError`로 정규화되어 반환된다. panic으로 프로세스를 죽이지 않는다(로컬 API 핸들러는 panic-safe). |
| U4-NFR-R2 | 맥락 조합·레이아웃·선정은 결정적이다 — 같은 입력 → 같은 출력. |
| U4-NFR-R3 | 로컬 API 포트 충돌 시 대체 포트를 탐색하고 실제 포트를 보고한다. |

## 유지보수 / 테스트
| ID | 요구 | 근거 |
|---|---|---|
| U4-NFR-M1 | PBT 프레임워크: Rust=**proptest**, TS=**fast-check** (PBT-09) | NFR-8 |
| U4-NFR-M2 | PBT는 shrinking 활성 + seed 재현 가능. proptest 회귀 파일을 레포에 커밋 (PBT-08). **생성기는 `Uuid::new_v4()` 같은 전역 RNG를 쓰지 않는다** — 시드가 제어하지 못하는 값이 섞이면 실패 재현이 성립하지 않는다 | NFR-8 |
| U4-NFR-M3 | 도메인 생성기(Fact/GraphDto/DraftRequest)를 재사용 가능한 테스트 유틸로 분리, 원시 타입 단독 생성기 금지 (PBT-07) | NFR-8 |
| U4-NFR-M4 | PBT와 예제 테스트를 파일/이름으로 명확히 구분 (`*_properties`, `*.property.test.ts`) (PBT-10) | NFR-8 |
| U4-NFR-M5 | 뷰는 `KnowsMeApi` 포트에만 의존 — U1 통합 시 어댑터 1개 교체로 완료 | 유지보수 |
| U4-NFR-M6 | `cargo clippy --all-targets -- -D warnings` 무경고 유지 (U1이 세운 기준 승계) | 코드 품질 |

## 사용성 / 접근성
| ID | 요구 |
|---|---|
| U4-NFR-U1 | 모든 뷰에 loading / ready / empty / error 4상태를 명시적으로 제공 |
| U4-NFR-U2 | 빈 상태는 다음 행동을 안내한다(예: "Queue에서 질문에 답하면 채워집니다") |
| U4-NFR-U3 | 그래프 노드는 키보드로 선택 가능(`role="button"`, Enter/Space, `aria-label`) |
| U4-NFR-U4 | UI 문구는 한국어 |

## 플랫폼 / 이식성
| ID | 요구 |
|---|---|
| U4-NFR-X1 | 대상 런타임은 Tauri WebView(WebKit/WebView2). ES2020 타깃, 레거시 브라우저 미지원 (NFR-1) |
| U4-NFR-X2 | 그래프 렌더는 외부 라이브러리 없는 표준 SVG — 플랫폼 간 렌더 차이 최소화 (NFR-1) |
| U4-NFR-X3 | U4는 자체 영속 저장을 하지 않으므로 저장 포맷 이식성(NFR-6)은 **N/A** |
