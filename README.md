# knows-me

![knows-me 데모](docs/demo.gif)

**내 작업 기록에서 "나에 대한 지식"을 뽑아 개인 위키로 쌓고, 내가 허락한 부분만 팀원의 AI가 물어볼 수 있게 하는 데스크탑 앱.**

동작 환경은 **Windows**와 **WSL(Ubuntu)** 두 가지다.

## 뭐 하는 앱인가

Claude Code·Codex 같은 AI 코딩 도구를 쓰다 보면, 내가 어떤 프로젝트를 어떻게 띄우고, 어떤 규칙을 지키고, 무엇을 결정했는지가 세션 기록에 남는다. knows-me는 그 기록을 읽어서:

1. **나에 대한 사실 후보를 뽑는다** — "이 프로젝트는 `./run.sh`로 띄운다", "PR은 squash merge만 한다" 같은 것.
2. **내가 확인한다** — 앱의 대기열에서 맞다/아니다, 공개/비공개를 정한다. 확인 전에는 아무것도 지식이 되지 않는다.
3. **위키로 쌓는다** — 내 PC 안에 암호화해서 저장한다. 밖으로 나가지 않는다.
4. **내 AI와 팀원의 AI가 물어본다** — 나는 내 지식 전부를, 팀원은 내가 공개로 지정한 범위만 MCP로 질의한다.

한 줄로: **"이 사람이라면 어떻게 할까?"를 사람 대신 내 지식이 답한다.**

지식을 뽑는 데 쓰는 LLM은 골라 쓸 수 있다: 내 PC의 **LM Studio·Ollama**, 이미 설치된 **Claude Code**, 또는 **OpenAI 호환 API·Anthropic API**.

## 어디서 돌릴 것인가 — 먼저 고른다

| | **Windows에 설치** (권장) | **WSL(Ubuntu) 안에서** |
|---|---|---|
| 준비물 | WebView2 (Win11 기본 포함) | WSLg(창 표시) — **Windows 11 필요** |
| Claude Code 세션 수집 | Windows 것 + **WSL 것까지 자동으로 읽는다** | 그 WSL 안의 것만 |
| LLM에 로컬 Claude Code 쓰기 | Windows 설치본과 **WSL 설치본 중 선택** | 그 WSL 안의 설치본 |
| LM Studio(보통 Windows에서 실행) | `http://localhost:1234/v1` | 호스트 IP 필요 ([usage §2-3](docs/usage.md#2-3-wsl에서-windows의-lm-studio에-붙기)) |
| 보관함 위치 | `%APPDATA%\app.knowsme.desktop\` | `~/.local/share/app.knowsme.desktop/` |

**대부분은 Windows에 설치하면 된다.** Windows에서 돌려도 WSL 안의 Claude Code 기록을 읽고, LLM으로 WSL의 Claude Code를 구동할 수 있다. WSL 설치는 개발 환경 안에서 같이 돌리고 싶을 때만 고른다. 둘 다 설치하면 **보관함이 각각 따로** 생긴다 (한쪽 데이터가 다른 쪽에 보이지 않는다).

## 5분 안에 띄우기

### 1. 준비물 설치

| 환경 | 한 번에 |
|---|---|
| **Windows 10 / 11** | PowerShell에서 `Set-ExecutionPolicy -Scope Process Bypass; .\scripts\setup-windows.ps1` |
| **WSL(Ubuntu 22.04 이상) · Linux** | `bash scripts/setup-linux.sh` |

스크립트는 Rust, Node.js 20 이상, 웹뷰 라이브러리(Windows는 WebView2, Linux는 webkit2gtk)를 확인·설치하고 `npm install`까지 해 준다. 직접 설치하려면 [docs/build.md](docs/build.md).

### 2. 실행

```bash
npx tauri dev
```

창이 뜨면 **보관함 비밀번호**를 정한다. 이 비밀번호로 모든 데이터가 암호화된다 (잊으면 복구 불가).

### 3. LLM 연결 — 예: 내 PC의 LM Studio

1. LM Studio에서 모델을 하나 로드하고 **서버를 켠다** (기본 주소 `http://localhost:1234`).
2. 앱 → **설정** → **AI 모델** → **OpenAI 호환** 선택.
3. 입력:
   - 모델 이름: LM Studio 서버 탭에 보이는 식별자 (예 `google/gemma-4-12b`)
   - Base URL: Windows에서 앱을 돌리면 `http://localhost:1234/v1`, **WSL에서 돌리면 호스트 IP** (예 `http://172.18.144.1:1234/v1`)
   - API 키: **비워 둔다**
4. **저장**. "현재 사용 중"에 `google/gemma-4-12b (localhost:1234)`처럼 표시되면 연결된 것.

**"현재 사용 중"은 지금 실제로 호출되는 백엔드를 그대로 보여준다.** 설정을 저장했는데도 여기에 `오프라인 (LLM 미연결 — 고정 응답)`이 뜨면 그 설정으로는 클라이언트를 만들지 못한 것이다 (주소 오타, 키 누락 등).

사내 게이트웨이처럼 호출에 **별도 헤더**가 필요하면 같은 화면의 **추가 헤더**에 `이름: 값`을 한 줄씩 적는다.

LLM을 고르는 다른 방법들은 [docs/usage.md §2](docs/usage.md#2-llm-연결).

### 4. 첫 수집

앱 → **설정** → **연결 소스** → Claude 카드의 **수집**. Claude Code(`~/.claude/projects/`)·Codex(`~/.codex/sessions/`) 기록을 읽어 사실 후보를 만든다. 확실한 것은 바로 사실로 저장되고, 물어봐야 할 것은 **대기열** 탭에 질문으로 쌓인다. **나와 대화** 탭에서 "내가 요즘 제일 걱정하는 게 뭘까?"처럼 물어보면 저장된 사실을 근거로 답한다.

같은 화면에서 **Confluence·Jira**(사내 Server/DC, 개인 액세스 토큰)와 Notion·Gmail도 연결할 수 있다.

## 배포용 빌드

```bash
npx tauri build                       # 공개판
npx tauri build -- --features internal  # 사내판 (Notion·Gmail 제외)
```

| 빌드 환경 | 결과물 |
|---|---|
| Windows | `desktop\target\release\bundle\` 의 `.msi`, 설치용 `.exe` |
| WSL · Linux | `desktop/target/release/bundle/` 의 `.deb`, `.rpm`, `.AppImage` |

**Windows 설치 파일은 Windows에서만 만들 수 있다** (WSL에서는 못 만든다). GitHub Actions([`.github/workflows/build.yml`](.github/workflows/build.yml))가 두 환경의 번들을 자동으로 만든다.

사내 배포 절차는 [docs/internal-release.md](docs/internal-release.md).

## 더 읽을 것

| 알고 싶은 것 | 문서 |
|---|---|
| **사내 배포 — Windows/WSL 선택, Confluence·Jira, 프록시·사설 인증서** | [docs/internal-release.md](docs/internal-release.md) |
| 빌드 환경 상세, 문제 해결 | [docs/build.md](docs/build.md) |
| 화면별 사용법, LLM 설정 전부, 명령줄 도구 | [docs/usage.md](docs/usage.md) |
| 수집이 왜 안 되나, 무엇이 들어오나 | [docs/collecting.md](docs/collecting.md) |
| 팀원에게 MCP로 공유하기 | [docs/team-sharing.md](docs/team-sharing.md) |
| 왜 이렇게 설계했나 (원래 README) | [docs/concept.md](docs/concept.md) |

## 개발자용

```bash
cd src-tauri
cargo test                            # Rust 코어 (GUI 불필요, 오프라인)
cargo test --features llm-http        # HTTP LLM 클라이언트 포함
cargo test --features atlassian-http  # Confluence·Jira 커넥터 포함
cargo test --features internal        # 사내 배포판 카탈로그

cd ..
npm run typecheck && npm test         # 프론트엔드
npm run dev                           # 브라우저에서 UI만 (목 데이터)
```

구조: Rust 코어 라이브러리 `src-tauri/`(암호화 저장·수집·LLM 게이트웨이·MCP 서버), Tauri 2 셸 `desktop/`, React 프론트 `src/`. 라이선스 MIT.
