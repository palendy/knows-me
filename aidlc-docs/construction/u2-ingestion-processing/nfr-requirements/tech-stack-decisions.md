# U2 Tech Stack Decisions — Ingestion & Processing

> 대상: `src-tauri/src/ingestion/`, `src-tauri/src/processing/` (Rust, 단일 crate `knows_me_core`).
> 원칙: 최소 의존, 크로스플랫폼, U1 계약(`Masker`/`LlmClient`/`CredentialStore`/`EncryptedStore`) 위에 구현.

## 1. 언어·런타임 (기확정)
- **Rust** (Tauri 코어) — Application Design AD2. async 런타임 `tokio`(계약이 `async_trait` 사용).

## 2. 크레이트 선정
| 목적 | 크레이트 | 근거 | 대안 |
|---|---|---|---|
| HTTP 클라이언트 (Notion/Gmail) | `reqwest` (async, rustls) | 성숙·async·TLS. Q4=A | `hyper` 직접(과한 저수준) |
| OAuth2 토큰 | `oauth2` (+ 커넥터 내부 갱신) | 표준 플로우, 토큰은 `CredentialStore` 보관 | 수동 구현 |
| 직렬화 | `serde` / `serde_json` (기존 계약이 이미 사용) | 공유 타입과 일관 | — |
| 파일 감시 | `notify` | 크로스플랫폼 FS 이벤트 + 배치 폴백. Q6=A | 폴링 전용(누락 위험) |
| PDF 파싱 | `pdf-extract` | 텍스트 추출 충분(MVP). Q5=A | `lopdf`(저수준) |
| DOCX 파싱 | `docx-rs` | DOCX 본문 추출. Q5=A | 수동 unzip+XML |
| 마스킹 규칙 | `regex` + 사용자 사전(설정) | 결정적·오프라인. Q8=A | LLM PII 탐지(향후) |
| 시간 | `chrono` (기존 계약 사용) | 공유 타입과 일관 | — |
| ID | `uuid` (기존 계약 사용) | TransferLogEntry id 등 | — |
| **PBT 프레임워크** | **`proptest`** (dev-dependency) | **PBT-09**: shrinking·seed 재현·custom strategy·`cargo test` 통합. Q1=A | `quickcheck`(shrinking 약함) |

## 3. PBT-09 준수 명세
- **선정**: Rust = `proptest` (dev-dependency, `[dev-dependencies] proptest = "1"`).
- **지원 확인**:
  - Custom generators/strategies for domain types ✔ (`RawItem`, 식별정보 포함 텍스트 strategy)
  - Automatic shrinking ✔
  - Seed-based reproducibility ✔ (`PROPTEST_CASES`, 실패 시 `.proptest-regressions` 로깅)
  - 기존 test runner 통합 ✔ (`cargo test`)
- **TypeScript**: U2는 백엔드 로직 전담이라 현재 TS PBT 대상 코드 없음. 프런트 계약 미러(`src/shared/contracts.ts`) 검증이 필요해지면 `fast-check` 추가(Q1=A). 현재는 N/A.
- **다국어 규칙**: PBT-적용 코드가 있는 언어(Rust)에 프레임워크 선정 완료. TS는 해당 코드 없어 N/A.

## 4. U1/U3 계약 사용 (U2가 구현하지 않음)
| 계약 | 소유 | U2 사용 |
|---|---|---|
| `Masker` | U1 | mask/unmask 호출(전송 전 필수) |
| `LlmClient` | U1 | summarize/classify/vision_extract 호출 |
| `CredentialStore` | U1 | Notion/Gmail 토큰 보관·로드 |
| `EncryptedStore` | U1 | cursor/seen/transfer.log/pending/file_skip ns |
| `KnowledgeApi.upsert` | U3 | 확실한 사실 저장(개발 중 mock) |
| `InterviewApi.enqueue` | U3 | 확인형/심화형 Queue(개발 중 mock) |

## 5. 의존성 추가 예정 (Cargo.toml)
```toml
[dependencies]
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }
oauth2 = "4"
notify = "6"
pdf-extract = "0.7"
docx-rs = "0.4"
regex = "1"
# 기존: serde, serde_json, chrono, uuid, async-trait, tokio

[dev-dependencies]
proptest = "1"
```
> 버전은 Code Generation 시 최신 안정 버전으로 확정하고 `cargo build`로 검증. 크레이트가 빌드/플랫폼 이슈가 있으면 대안(표의 우측)으로 교체.

## 6. 보안 관점 결정
- `reqwest`는 `rustls-tls`로(OpenSSL 시스템 의존 회피, 재현성↑).
- 모든 외부 토큰은 `CredentialStore` 경유 — 코드/파일 평문 금지(심사기준⑥, BR-C2).
- 마스킹은 오프라인 결정적 규칙 우선(네트워크·비용 없이 항상 동작, NFR-3 부합).
