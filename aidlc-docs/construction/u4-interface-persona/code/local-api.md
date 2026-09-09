# 로컬 페르소나 API (US-6.2)

> MVP 정책(D7/D8): **loopback 전용, 인증 없음, 앱 실행 중에만 동작.** 원격 노출·API 키는 범위 외(FR-7.3).

## 시작/종료
```rust
use std::sync::Arc;
use knows_me_core::persona::{LocalApiServer, PersonaService, DEFAULT_PORT};

let persona = Arc::new(PersonaService::new(knowledge, masker, llm));
let mut handle = LocalApiServer::start(persona, DEFAULT_PORT).await?;
println!("http://127.0.0.1:{}", handle.port());   // 실제 바인딩된 포트
// ...
handle.stop().await;   // 멱등 — 두 번 호출해도 안전
```

- 기본 포트 **8765**. 사용 중이면 8765..8785에서 첫 가용 포트를 쓰고 `handle.port()`로 알려준다.
- `port`에 `0`을 주면 OS가 임의 포트를 고른다(테스트용).

## 엔드포인트

### `POST /chat`
```http
POST /chat HTTP/1.1
Host: 127.0.0.1:8765
Content-Type: application/json

{ "prompt": "내 배포 절차 알려줘" }
```
```json
{ "text": "main 에 머지되면 make deploy 로 배포한다" }
```

### `POST /draft`
```http
POST /draft HTTP/1.1
Host: 127.0.0.1:8765
Content-Type: application/json

{ "kind": "Email", "prompt": "배포 일정 공유 메일 써줘" }
```
```json
{ "text": "제목: 배포 일정 공유\n\n안녕하세요, ..." }
```
`kind`: `"Email"` | `"Message"` | `"Post"`

### `GET /health`
```json
{ "status": "ok", "persona": true }
```

## 오류
| 상태 | 의미 | 트리거 |
|---|---|---|
| 400 | 요청 본문 해석 불가, 빈/과길이 프롬프트 | `AppError::InvalidInput` |
| 403 | `Host` 헤더가 loopback이 아님 | DNS rebinding 방어 (BR-A2) |
| 404 | 대상 없음 | `AppError::NotFound` |
| 423 | 앱이 잠겨 있음 | `AppError::Locked` |
| 502 | 외부 LLM/네트워크 실패 | `AppError::External` |
| 500 | 그 외 내부 오류 | `Crypto`/`Io`/`Serde` |

오류 본문은 `{ "error": "<메시지>" }`. 파일 경로·스택 트레이스를 포함하지 않는다(U4-NFR-SEC6).

## 보안 특성
1. 리스너는 `127.0.0.1`에만 바인딩된다 — 외부 인터페이스에 소켓 자체가 없다. `0.0.0.0` 바인딩 경로는 코드에 존재하지 않는다.
2. `Host`가 `127.0.0.1` / `localhost` / `[::1]`(포트 접미사 무시)이 아니면 403. 브라우저 페이지가 rebinding으로 접근하는 경로를 막는다.
3. MVP는 인증이 없다(D7). 같은 기기의 다른 프로세스는 접근할 수 있으므로, 원격 노출을 도입할 때 반드시 FR-7.3(API 키/스코프)을 먼저 구현해야 한다.

## curl 예시
```bash
curl -sS -X POST http://127.0.0.1:8765/chat \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"내 배포 절차 알려줘"}'
```
