# 사내 배포 가이드 (internal edition)

> 대상: knows-me를 **회사망 안에서** 배포·운영하려는 사람. 일반 사용법은 [`usage.md`](usage.md), 빌드는 [`build.md`](build.md).

## 0. 사내판이 뭐가 다른가

회사망에서는 Notion·Gmail에 닿지 않는다. 사내판은 그 둘을 **아예 빼고** 빌드한다 — 카드가 보이지도 않고, "끝까지 수집"이 그 소스를 돌지도 않는다.

| | 공개판 (기본) | 사내판 (`--features internal`) |
|---|---|---|
| Claude·Codex 세션 | ○ | ○ |
| 파일 | ○ | ○ |
| Confluence (Server/DC, PAT) | ○ | ○ |
| Jira (Server/DC, PAT) | ○ | ○ |
| Notion | ○ | **×** |
| Gmail | ○ | **×** |
| LLM | 설정 화면에서 선택 | 같음 — 사내 LLM 게이트웨이는 **OpenAI 호환**으로 URL 지정 |

코드는 하나고 cargo 피처 하나(`internal`)로 갈린다. 사내 호스트 이름·주소는 저장소에 넣지 않는다 — 사용자가 설정 화면에서 입력하고 암호화 보관함에만 남는다.

## 1. 빌드

```bash
npx tauri build -- --features internal
```

결과물 위치는 공개판과 같다(`desktop/target/release/bundle/`). GitHub Actions에서는 **Run workflow → edition: internal**을 고르면 Linux·Windows 번들이 `knows-me-linux-internal` / `knows-me-windows-internal` 아티팩트로 나온다.

개발 실행도 같은 플래그: `npx tauri dev -- --features internal`.

## 2. Confluence · Jira 연결

alpha-agent-v3가 사내 Confluence·Jira에 붙는 방식을 그대로 가져왔다.

- 대상은 **Server / Data Center**(Confluence 9.x, Jira 10.x). Atlassian Cloud가 아니다.
- 인증은 **개인 액세스 토큰(PAT)** 을 `Authorization: Bearer`로 보낸다. 프로필 → Personal Access Tokens에서 발급.
- 사내 Confluence가 **mirror 서버**(읽기 전용, 매일 새벽 동기화)로만 API를 열어 두었다면 서버 주소에는 mirror를, **링크용 주소**에는 원본 서버를 넣는다. API 호출은 항상 mirror로 가고 사실에 붙는 링크만 원본을 가리킨다. 데이터가 최대 하루 늦을 수 있다.
- 연결 시 `GET /rest/api/user/current`(Confluence) · `GET /rest/api/2/myself`(Jira)로 검증하고, 내 페이지·이슈 수를 세어 보여준다.

무엇을 어떻게 가져오는지는 [`usage.md` §3-1](usage.md#3-1-confluence--jira-사내-server--data-center).

### 오류를 읽는 법 (401 ≠ 403)

| 상태 | 판정 근거 | 안내 |
|---|---|---|
| 401 | — | PAT 만료·오류. 재발급 |
| 403 + `X-AUSERNAME` 헤더 | 서버가 요청자를 식별함 = **인증 통과** | 문서 분류(극비 등)·스페이스/프로젝트 권한 문제. **PAT를 다시 넣어도 안 풀린다.** 수집은 해당 항목만 건너뛴다 |
| 403 + `X-Authentication-Denied-Reason` | 서버가 인증 자체를 거부(CAPTCHA 등) | 브라우저로 한 번 로그인해 잠금 해제 후 PAT 재발급 |
| 200 + HTML | SSO가 로그인 페이지를 돌려줌 | PAT가 안 받아들여졌거나 주소가 API 서버가 아님 |
| 429 | mirror 분당 호출 한도(계정당 약 15회) | 자동으로 `Retry-After`만큼 기다렸다 재시도(최대 3회). 그래서 Confluence는 한 번에 10페이지씩만 |

## 3. 프록시 · 사설 인증서

앱은 OS 환경변수를 그대로 따른다.

| 상황 | 설정 |
|---|---|
| 사내 서버가 사설 CA 인증서를 쓴다 | 그 CA를 **OS 신뢰 저장소**에 넣으면 끝 (Windows: 인증서 관리자, Linux: `update-ca-certificates`). 앱은 OS 저장소를 쓴다 |
| OS 저장소를 못 건드린다 | `SSL_CERT_FILE=/path/to/corp-ca.pem` 환경변수로 실행 |
| 외부는 프록시, 사내는 직결 | `HTTPS_PROXY=http://proxy:8080` + `NO_PROXY=jira.example.com,confluence.example.com,localhost,127.0.0.1` |

`verify=false` 같은 인증서 검증 끄기 옵션은 **없다**. alpha-agent-v3도 사내 시스템에는 두지 않았다.

## 4. LLM

지식 추출에 쓰는 LLM은 설정 화면에서 고른다([`usage.md` §2](usage.md#2-llm-연결)). 사내에서는:

- 사내 LLM 게이트웨이가 OpenAI Chat Completions 호환이면 **OpenAI 호환** 선택 → Base URL + 모델 이름 (+ 키).
- 각자 PC의 LM Studio·Ollama도 같은 방식.
- 로컬 Claude Code가 설치·로그인돼 있으면 그대로 쓸 수 있다(텍스트가 Anthropic으로 나간다는 점은 클라우드와 같다).

**데이터 전송** 정책의 기본값 "개인정보를 가리고 전송"은 사내판에서도 그대로다. Confluence·Jira 본문에 들어 있는 이메일·경로·토큰은 LLM에 가기 전에 마스킹된다. 무엇이 나갔는지는 설정 → 전송 기록.

## 5. 배포 전 점검

```bash
cd src-tauri
cargo test --features internal          # 카탈로그가 사내판인지 (Notion/Gmail 없음)
cargo test --features atlassian-http    # 커넥터 코드 컴파일 + 오프라인 테스트
cd .. && npm test
```

실제 서버에 대한 연결 검증은 앱에서 카드의 **연결**을 눌러 한다 — 그 응답("연결됨 · 이름 · N개")이 곧 통합 테스트다.
