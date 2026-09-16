# 사내 배포 가이드 (internal edition)

> 대상: knows-me를 **회사망 안에서** 배포·운영하려는 사람. 대상 환경은 **Windows**와 **WSL(Ubuntu)** 둘뿐이다.
> 일반 사용법은 [`usage.md`](usage.md), 빌드 상세는 [`build.md`](build.md).

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

## 1. 어디에 설치할 것인가 — Windows를 기본으로

사내 PC는 Windows이고, 개발자는 그 위의 WSL에서 일한다. 앱은 둘 중 한 곳에 설치한다.

| | **Windows에 설치** (권장) | **WSL(Ubuntu)에 설치** |
|---|---|---|
| 배포 파일 | `.msi` (또는 설치용 `.exe`) | `.deb` |
| 전제 조건 | WebView2 런타임 (Win11 기본 포함) | **WSLg — Windows 11에서만** |
| Claude Code 세션 | Windows 것 + **설치된 모든 WSL 배포판의 것**을 함께 읽는다 | 그 WSL 안의 것만 |
| LLM으로 로컬 Claude Code | Windows 설치본 / WSL 설치본 중 **골라서** 구동 | 그 WSL 안의 설치본 |
| 사내 Confluence·Jira | Windows 네트워크 설정(프록시·CA)을 그대로 탄다 | WSL 쪽 설정을 따로 맞춰야 한다 |
| LM Studio (보통 Windows) | `http://localhost:1234/v1` | 호스트 IP 필요 (§4) |
| 보관함 위치 | `%APPDATA%\app.knowsme.desktop\` | `~/.local/share/app.knowsme.desktop/` |

**권장: Windows 설치.** 이유는 세 가지다.

1. Windows에 설치해도 **WSL 안의 작업 기록을 읽는다.** 세션 수집은 `%USERPROFILE%`과 함께 `\\wsl.localhost\<배포판>\home\<계정>\`의 `.claude/projects`·`.codex/sessions`를 훑는다. WSL에서만 Claude Code를 쓰는 사람도 Windows 앱으로 수집된다.
2. 설정 → AI 모델 → **로컬 Claude Code**를 고르면 "Windows"와 "WSL · \<배포판\>"이 후보로 나온다. 로그인돼 있는 쪽을 고르면 된다.
3. Windows 10 PC에서는 WSL 안에 창을 띄울 수 없다(WSLg는 Windows 11 기능). Windows 설치는 그 제약이 없다.

WSL 설치는 개발 환경 안에서 같이 돌리고 싶을 때만 고른다.

> **둘 다 설치하면 보관함이 각각 따로 생긴다.** 비밀번호도, 모아 둔 사실도 공유되지 않는다. 한 사람이 한 곳만 쓰도록 안내한다.

## 2. 빌드와 배포

```bash
npx tauri build -- --features internal
```

| 빌드 환경 | 결과물 | 배포 대상 |
|---|---|---|
| Windows | `desktop\target\release\bundle\msi\*.msi`, `...\nsis\*.exe` | Windows 사용자 |
| WSL · Linux | `desktop/target/release/bundle/deb/*.deb` | WSL 사용자 |

**Windows 설치 파일은 Windows에서만 만들 수 있다.** WSL에서 빌드하면 `.deb`만 나온다. 두 가지가 다 필요하면 GitHub Actions에서 **Run workflow → edition: internal**을 고른다. `knows-me-windows-internal` / `knows-me-linux-internal` 아티팩트가 나온다.

개발 실행도 같은 플래그: `npx tauri dev -- --features internal`.

설치 후 첫 실행에서 사용자가 **보관함 비밀번호**를 정한다. 이 비밀번호는 복구 수단이 없다 — 잊으면 그 사람의 데이터는 끝이다. 배포 안내에 반드시 넣는다.

## 3. Confluence · Jira 연결

alpha-agent-v3가 사내 Confluence·Jira에 붙는 방식을 그대로 가져왔다.

- 대상은 **Server / Data Center**(Confluence 9.x, Jira 10.x). Atlassian Cloud가 아니다.
- 인증은 **개인 액세스 토큰(PAT)** 을 `Authorization: Bearer`로 보낸다. 프로필 → Personal Access Tokens에서 발급.
- 사내 Confluence가 **mirror 서버**(읽기 전용, 매일 새벽 동기화)로만 API를 열어 두었다면 서버 주소에는 mirror를, **링크용 주소**에는 원본 서버를 넣는다. API 호출은 항상 mirror로 가고 사실에 붙는 링크만 원본을 가리킨다. 데이터가 최대 하루 늦을 수 있다.
- 연결 시 `GET /rest/api/user/current`(Confluence) · `GET /rest/api/2/myself`(Jira)로 검증하고, 내 페이지·이슈 수를 세어 보여준다.

무엇을 어떻게 가져오는지는 [`usage.md` §3-1](usage.md#3-1-confluence와-jira-사내-server-dc).

### 오류를 읽는 법 (401 ≠ 403)

| 상태 | 판정 근거 | 안내 |
|---|---|---|
| 401 | — | PAT 만료·오류. 재발급 |
| 403 + `X-AUSERNAME` 헤더 | 서버가 요청자를 식별함 = **인증 통과** | 문서 분류(극비 등)·스페이스/프로젝트 권한 문제. **PAT를 다시 넣어도 안 풀린다.** 수집은 해당 항목만 건너뛴다 |
| 403 + `X-Authentication-Denied-Reason` | 서버가 인증 자체를 거부(CAPTCHA 등) | 브라우저로 한 번 로그인해 잠금 해제 후 PAT 재발급 |
| 200 + HTML | SSO가 로그인 페이지를 돌려줌 | PAT가 안 받아들여졌거나 주소가 API 서버가 아님 |
| 429 | mirror 분당 호출 한도(계정당 약 15회) | 자동으로 `Retry-After`만큼 기다렸다 재시도(최대 3회). 그래서 Confluence는 한 번에 10페이지씩 |

## 4. 네트워크 — 프록시, 사설 인증서, WSL 경계

앱은 OS 환경변수를 그대로 따른다. **앱이 도는 쪽**에 설정해야 한다 — Windows 앱이면 Windows에, WSL 앱이면 그 WSL에.

| 상황 | 설정 |
|---|---|
| 사내 서버가 사설 CA 인증서를 쓴다 | 그 CA를 **OS 신뢰 저장소**에 넣으면 끝 (Windows: 인증서 관리자의 "신뢰할 수 있는 루트 인증 기관", WSL: `/usr/local/share/ca-certificates/`에 넣고 `sudo update-ca-certificates`) |
| OS 저장소를 못 건드린다 | `SSL_CERT_FILE=<CA 경로>` 환경변수로 실행 |
| 외부는 프록시, 사내는 직결 | `HTTPS_PROXY=http://proxy:8080` + `NO_PROXY=confluence.example.com,jira.example.com,localhost,127.0.0.1` |

`verify=false` 같은 인증서 검증 끄기 옵션은 **없다**.

### WSL에서 앱을 돌릴 때의 경계

- **Windows의 LM Studio**: WSL의 `localhost`는 WSL 자신이다. `ip route | grep default`의 IP를 쓰고(예 `http://172.18.144.1:1234/v1`), LM Studio 서버 설정에서 "Serve on Local Network"를 켠다. `.wslconfig`에 `networkingMode=mirrored`를 쓰면 `localhost`로도 닿는다.
- **사내 프록시·CA**: Windows에 넣은 인증서가 WSL에 자동으로 따라오지 않는다. WSL 안에도 넣어야 한다.
- 이 두 가지를 맞추기 싫으면 Windows 설치를 쓰는 편이 빠르다.

## 5. LLM

지식 추출에 쓰는 LLM은 설정 화면에서 고른다([`usage.md` §2](usage.md#2-llm-연결)). 사내에서는 보통 셋 중 하나다.

| 선택 | 설정 |
|---|---|
| 사내 LLM 게이트웨이 (OpenAI 호환) | **OpenAI 호환** → Base URL + 모델 이름 (+ 키). 별도 헤더가 필요하면 **추가 헤더**에 `이름: 값` |
| 각자 PC의 LM Studio·Ollama | **OpenAI 호환** → 로컬 주소, 키는 비움 |
| 이미 쓰는 Claude Code | **로컬 Claude Code** → Windows/WSL 설치본 중 선택 |

**"현재 사용 중"에는 지금 살아 있는 클라이언트가 스스로 보고한 이름이 뜬다.** 설정대로 클라이언트를 만들지 못했으면 고른 모델 대신 `오프라인 (LLM 미연결 — 고정 응답)`이 보인다. 그 상태로 수집하면 사실이 고정 문구로 채워지므로, 배포 안내에 "여기를 먼저 확인하라"고 넣는다.

**데이터 전송** 정책의 기본값 "개인정보를 가리고 전송"은 사내판에서도 그대로다. Confluence·Jira 본문에 들어 있는 이메일·경로·토큰은 LLM에 가기 전에 마스킹된다. 무엇이 나갔는지는 설정 → 전송 기록.

## 6. 배포 전 점검

빌드하는 곳에서:

```bash
cd src-tauri
cargo test --features internal          # 카탈로그가 사내판인지 (Notion/Gmail 없음)
cargo test --features atlassian-http    # 커넥터 코드 컴파일 + 오프라인 테스트
cd .. && npm run typecheck && npm test
```

설치한 PC에서 (사람이 한 번씩):

1. 앱을 열고 보관함 비밀번호를 정한다 → 잠금 해제된다.
2. 설정 → AI 모델 → **"현재 사용 중"이 의도한 백엔드인지** 확인한다.
3. 설정 → 연결 소스 → Confluence·Jira **연결** → "연결됨 · \<이름\> · N개"가 뜨는지 확인한다. 이 응답이 곧 통합 테스트다.
4. Claude 카드의 **수집**을 한 번 돌리고, 대시보드 숫자가 오르는지 본다.
5. 설정 → 전송 기록에 방금 보낸 요청이 남았는지 본다.

Windows에 설치했다면 3~4번에서 **WSL 쪽 기록까지 잡히는지** 함께 확인한다 (수집 결과에 WSL에서 하던 프로젝트가 나오면 정상).
