# 사용 가이드

> 대상: 앱을 설치한 다음 실제로 쓰려는 사람. 동작 환경은 **Windows**와 **WSL(Ubuntu)** 두 가지다.
> 빌드는 [`build.md`](build.md), 사내 배포는 [`internal-release.md`](internal-release.md).

## 0. 앱이 Windows에 있나, WSL에 있나

설정이 갈리는 지점이 몇 군데 있어서 이걸 먼저 확인한다.

| | **Windows 앱** | **WSL(Ubuntu) 앱** |
|---|---|---|
| 수집되는 Claude·Codex 기록 | Windows 홈 + **설치된 모든 WSL 배포판의 홈** | 그 WSL 안의 홈만 |
| LLM으로 쓸 로컬 Claude Code | Windows 설치본 / WSL 설치본 중 선택 | 그 WSL 안의 설치본 |
| Windows에서 도는 LM Studio 주소 | `http://localhost:1234/v1` | 호스트 IP (§2-3) |
| 프록시·사설 인증서 설정 위치 | Windows | 그 WSL 안 |
| 보관함(데이터) 위치 | `%APPDATA%\app.knowsme.desktop\` | `~/.local/share/app.knowsme.desktop/` |

**Windows 앱이 WSL 기록까지 읽는다**는 게 핵심이다. WSL에서만 Claude Code를 쓰더라도 Windows에 설치한 앱으로 수집된다. 어느 쪽에 설치할지의 판단은 [`internal-release.md` §1](internal-release.md#1-어디에-설치할-것인가--windows를-기본으로).

**두 곳에 다 설치하면 보관함이 각각 따로 생긴다.** 비밀번호도 모아 둔 사실도 공유되지 않는다.

## 1. 처음 실행

```bash
npx tauri dev            # 개발 실행
# 또는 설치한 앱 실행 — Windows는 .msi/.exe, WSL은 .deb
```

1. **비밀번호 설정** — 첫 화면에서 보관함 비밀번호를 정한다. 이 비밀번호에서 암호화 키를 만들고(Argon2id), 저장되는 모든 것을 AES-256-GCM으로 암호화한다. 평문 키는 디스크에 남지 않는다. **잊으면 복구할 수 없다.**
2. **잠금 해제** — 이후 실행할 때마다 같은 비밀번호를 넣는다.

데이터는 §0의 경로에 전부 암호화돼 저장된다. 지우면 처음부터 다시 시작한다.

## 2. LLM 연결

지식 추출(요약·분류)과 페르소나 대화에 LLM이 쓰인다. **설정 → AI 모델**에서 고른다.

| 선택지 | 언제 | 필요한 것 |
|---|---|---|
| **로컬 Claude Code** (기본) | Claude Code가 이미 설치·로그인돼 있을 때 | 없음. `echo ping \| claude -p`가 답하면 된다 |
| **OpenAI 호환** | 내 PC의 LM Studio·Ollama, 또는 OpenRouter·OpenAI | URL (+ 클라우드는 API 키) |
| **Anthropic API** | Anthropic 키를 직접 쓸 때 | API 키 |

저장하면 즉시 반영된다. **"현재 사용 중"에는 지금 살아 있는 클라이언트가 스스로 보고한 이름이 뜬다** — 설정값을 되읽는 게 아니라서, 저장한 설정으로 클라이언트를 만들지 못했으면 고른 모델 대신 `오프라인 (LLM 미연결 — 고정 응답)`이 보인다. 그 상태에서는 요약·분류가 고정 문구로 채워지므로 수집을 돌리기 전에 먼저 고쳐야 한다.

잠금 해제 전에는 아직 클라이언트가 없어 `(잠금 해제 전)`이 붙은 예상값이 보인다. 터미널에서 앱을 띄웠다면 `[llm] client built: …` 줄이 같은 값을 찍는다.

> `.env` 파일의 LLM 설정은 **앱이 아니라 명령줄 도구(§6)용**이다. 앱은 설정 화면에 저장된 값을 우선한다.

### 2-1. LM Studio (내 PC, 무료, 키 없음)

1. LM Studio → 모델 로드 → **Developer/Server 탭에서 서버 시작** (기본 `http://localhost:1234`).
2. 서버 탭에 보이는 **모델 식별자**를 복사 (예 `google/gemma-4-12b`).
3. 앱 → 설정 → AI 모델 → **OpenAI 호환**:

| 항목 | 값 |
|---|---|
| 모델 이름 | `google/gemma-4-12b` |
| Base URL | `http://localhost:1234/v1` (`/v1` 유무는 상관없다) |
| API 키 | 비워 둠 |

4. 저장 → "현재 사용 중: `google/gemma-4-12b (localhost:1234)`".

실측(gemma-4-12b, 로컬 GPU): 요약 약 25초, 분류 약 30초, 대화 약 7초. thinking 모델은 추론에 토큰을 쓰므로 느리다. 속도가 급하면 LM Studio에서 thinking을 끄거나 더 작은 모델을 쓴다.

### 2-2. Ollama

```bash
ollama pull llama3.1 && ollama serve
```

| 항목 | 값 |
|---|---|
| 모델 이름 | `llama3.1` |
| Base URL | `http://localhost:11434/v1` |
| API 키 | 비워 둠 |

### 2-3. WSL에서 Windows의 LM Studio에 붙기

WSL 안에서 앱을 돌리고 LM Studio는 Windows에 있으면, `localhost`는 WSL 자신을 가리키므로 닿지 않는다.

1. LM Studio 서버 설정에서 **"Serve on Local Network"** 를 켠다 (0.0.0.0 바인딩).
2. WSL에서 Windows 호스트 IP를 찾는다:
   ```bash
   ip route | grep default        # 예: default via 172.18.144.1 ...
   curl http://172.18.144.1:1234/v1/models   # 모델 목록이 나오면 된다
   ```
3. Base URL에 `http://172.18.144.1:1234/v1` 처럼 그 IP를 쓴다.

WSL 네트워크를 mirrored 모드(`.wslconfig`의 `networkingMode=mirrored`)로 쓰면 `localhost`로도 닿는다.

### 2-4. OpenRouter / OpenAI

| 항목 | OpenRouter | OpenAI |
|---|---|---|
| 모델 이름 | `anthropic/claude-sonnet-4` 등 | `gpt-4o` 등 |
| Base URL | `https://openrouter.ai/api/v1` | 비움 |
| API 키 | `sk-or-v1-...` | `sk-...` |

키는 암호화 보관함에 저장되고 IPC로 되돌아오지 않는다. 비워 두고 저장하면 기존 키가 유지된다.

### 2-5. Anthropic API

**Anthropic API** 선택 → 모델 이름(`claude-sonnet-5` 등) + API 키.

### 2-6. 로컬 Claude Code

**로컬 Claude Code** 선택 → 감지된 설치 중 하나를 고른다. Windows 앱이면 "Windows"와 "WSL · \<배포판\>"이 각각 후보로 뜬다 — 두 설치본의 **로그인 상태가 다른 경우가 많으니** 실제로 쓰는 쪽을 고른다. 그다음 모델(`claude-opus-5` 권장, 빠른 건 `claude-haiku-4-5`).

호출마다 `claude -p` 세션이 하나 뜨므로 한 호출에 10초 남짓 걸린다. 텍스트는 Anthropic에 도달한다는 점에서 클라우드 백엔드와 같다.

### 2-7. 추가 헤더 (사내 게이트웨이 등)

호출에 별도 헤더가 필요한 LLM이 있다. 사내 게이트웨이의 라우팅 헤더, Azure 스타일의 `api-key`, OpenRouter의 `X-Title` 같은 것이다. **설정 → AI 모델 → 추가 헤더**에 한 줄에 하나씩 적는다.

```
api-key: 1234abcd
X-Gateway-Id: team-a
# 이 줄은 주석
```

- **OpenAI 호환**과 **Anthropic API**에만 적용된다. 로컬 Claude Code는 HTTP 호출을 하지 않는다.
- 여기 적은 헤더가 **마지막에** 붙는다. 그래서 `Authorization`을 적으면 위의 API 키 대신 그 값이 나간다 — 게이트웨이가 자체 인증 헤더를 요구할 때 쓰라는 뜻이다.
- 형식이 틀리면 **저장 시점에** 몇 번째 줄이 왜 틀렸는지 알려준다. 값에 한글이 섞여 있어도 (IME로 복사하다 섞이는 일이 잦다) 거기서 걸린다.
- 값은 나머지 설정과 같은 암호화 보관함에 저장된다.

### 2-8. 데이터 전송 정책

**설정 → 데이터 전송**:

- **개인정보를 가리고 전송** (권장) — 이메일·경로·토큰 같은 식별자를 마스킹한 뒤 보낸다.
- **원본 그대로 전송**
- **기기 안에서만 사용** — LLM에 아무것도 보내지 않는다 (추출은 사실상 멈춘다).

무엇을 보냈는지는 **설정 → 전송 기록**에 남는다 (탭을 열 때마다, 그리고 수집이 끝날 때마다 다시 읽는다). LM Studio처럼 로컬 LLM이면 PC 밖으로 나가는 것은 없다.

## 3. 소스 연결과 수집

**설정 → 연결 소스**. 카드마다 **수집** 버튼이 있고, 위의 "새로 온 것만 / 끝까지 수집 / 범위"로 모드를 고른다.

| 소스 | 어떻게 |
|---|---|
| Claude (Claude Code 세션) | 자동. `~/.claude/projects/`를 읽는다 (Windows 앱이면 WSL 쪽 홈까지) |
| Codex 세션 | 자동. `~/.codex/sessions/`를 읽는다 (같음) |
| 파일 | 폴더 지정 (텍스트·마크다운) |
| Notion | 연결 → Notion Integrations에서 만든 내부 통합 토큰 입력 |
| Gmail | 연결 → Gmail 주소 + 앱 비밀번호 |
| Confluence (Server/DC) | 연결 → 서버 주소 + 개인 액세스 토큰(PAT). §3-1 |
| Jira (Server/DC) | 연결 → 서버 주소 + PAT. §3-1 |

- **새로 온 것만** — 각 소스에서 한 묶음(세션 최대 30개)씩 증분으로 가져와 LLM으로 요약·분류하고 대기열·사실에 넣는다.
- **끝까지 수집** — 남은 기록을 전부. 처음이면 오래 걸린다.
- 실측: LM Studio gemma-4-12b(thinking)로 Claude 세션 한 묶음이 약 10분(요약·분류 호출 14회). Claude Code CLI나 클라우드 API가 더 빠르다.
- **수집 범위** — 어떤 프로젝트의 세션을 모을지 고른다. 처음엔 프로젝트 한두 개로 좁혀 시작하는 게 빠르다. Windows 앱이면 이 목록에 WSL 쪽 프로젝트도 함께 나온다.

자세한 것과 문제 진단은 [`collecting.md`](collecting.md).

### 3-1. Confluence와 Jira (사내 Server DC)

Atlassian **Cloud가 아니라 사내에 설치된 Server/Data Center**용이다. 인증은 **개인 액세스 토큰(PAT)** 하나다.

1. Confluence/Jira 오른쪽 위 프로필 → **Personal Access Tokens** → **Create token**. 토큰은 그 자리에서만 보이니 바로 복사한다.
2. 앱 → 설정 → 연결 소스 → Confluence 또는 Jira 카드의 스위치:

| 항목 | 값 |
|---|---|
| 서버 주소 | `https://jira.example.com` 처럼 REST API가 열려 있는 주소. 끝에 `/` 없이, `/wiki`·`/browse` 같은 경로 없이 |
| 개인 액세스 토큰 | 방금 복사한 값 |
| 링크용 주소 (Confluence만, 선택) | API 주소가 **mirror 서버**라면, 사람이 클릭할 링크에 쓸 원본 서버 주소 |

3. **연결**을 누르면 그 자리에서 검증한다 — "연결됨 · 홍길동 · 내가 작성·수정한 페이지 37개"처럼 누구로 인증됐고 무엇이 보이는지 알려준다.

무엇을 가져오나:

- **Confluence**: 내가 **만들거나 편집한 페이지**(`contributor = currentUser()`)의 본문. 페이지가 새 버전으로 바뀌면 다시 가져온다.
- **Jira**: 내가 **담당자이거나 보고자인 이슈**의 설명 + 댓글 전체. 진행 중에 한 번, 해결된 뒤 한 번 가져온다(상태가 바뀔 때마다는 아니다).
- 둘 다 **수정 시각 순으로 증분** 수집한다. "새로 온 것만"은 Confluence 10페이지 / Jira 25이슈씩, "끝까지 수집"은 남은 것 전부.

자주 보는 메시지:

| 메시지 | 뜻 |
|---|---|
| 인증에 실패했습니다 (401) | PAT가 틀렸거나 만료. 다시 발급 |
| 권한이 없습니다 (403) … PAT 문제가 아니라 | 인증은 됐지만 그 문서/프로젝트가 제한됨. **PAT를 다시 넣어도 소용없다.** 수집은 그 항목만 건너뛰고 계속된다 |
| JSON 대신 로그인/HTML 페이지 | SSO가 PAT를 안 받았거나, 주소가 API 서버가 아님 |
| 이상 문자(한글/공백 등) | 토큰을 복사하다 IME 글자가 섞임. 다시 붙여넣기 |

사내 프록시·사설 인증서 환경은 [`internal-release.md`](internal-release.md).

## 4. 화면

| 탭 | 하는 일 |
|---|---|
| **대시보드** | 수집 현황, 대기 중인 질문 수, 확정 사실 수, 핵심 기록 카드 |
| **대기열** | 인터뷰 질문과 후보 확인: 답하면 사실로 저장, 공개/비공개·범주 지정 |
| **나와 대화** | "나라면 어떻게?"를 저장된 사실을 근거로 대화. 답변 아래 "근거로 삼은 사실"이 붙는다 |
| **설정** | 데이터 전송 정책 · AI 모델 · 연결 소스 · 공유(MCP) · 전송 기록 |

핵심 규칙: **대기열에서 확인한 것만 지식이 된다.** 자동 수집물은 확인 전까지 비공개로 격리된다.

## 5. 팀원에게 공유 (MCP)

**설정 → 공유**를 켜면 MCP 서버 두 개가 뜬다. 화면에 각각의 주소(`127.0.0.1:<포트>/mcp`)가 표시된다.

- **나 (오너 주소)**: 내 Claude Code에서 화면에 보이는 오너 주소를 등록한다.
  ```bash
  claude mcp add --transport http knows-me http://127.0.0.1:<오너 포트>/mcp
  ```
- **팀원 (공유 주소)**: 같은 화면에서 토큰을 발급해 준다. 토큰에 부여한 범주만 보인다. 터널(cloudflared 등)로 공유 주소를 밖에 열 수 있다.

자세한 절차와 보안 규칙은 [`team-sharing.md`](team-sharing.md), 프로토콜은 [`mcp-contract.md`](mcp-contract.md).

## 6. 명령줄 도구 (GUI 없이)

코어 example들은 `.env`를 읽는다. `cp .env.example .env` 후 편집.

```bash
cd src-tauri

# LLM 연결이 되는지 30초 안에 확인 — 요약·분류·대화 한 번씩
LLM_PROVIDER=openai OPENAI_BASE_URL=http://localhost:1234/v1 OPENAI_MODEL=google/gemma-4-12b \
  cargo run --features llm-http --example llm_probe

# Confluence·Jira 연결이 되는지 — 검증 + 한 묶음 수집 + 커서 재개 (저장은 안 함)
CONFLUENCE_BASE_URL=https://confluence.example.com CONFLUENCE_PAT=... \
JIRA_BASE_URL=https://jira.example.com JIRA_PAT=... \
  cargo run --features atlassian-http --example atlassian_probe

# 확정 사실 / 대기열 덤프 — 앱 데이터 디렉터리와 보관함 비밀번호를 넘긴다
export KNOWSME_DATA_DIR=~/.local/share/app.knowsme.desktop KNOWSME_DEMO_PASSWORD='보관함 비밀번호'
cargo run --example dump_facts
cargo run --example dump_queue
```

`llm_probe`가 성공하면 이런 출력이 나온다:

```
backend: google/gemma-4-12b (localhost:1234)
[summarize] (25.4s) ...
[classify] (31.4s) ["certain", "personal", "practice", "public", ...]
[chat] (6.9s) ...
```

## 7. 자주 묻는 것

**설정에 "오프라인 (LLM 미연결 — 고정 응답)"이라고 나온다.** 저장한 설정으로 클라이언트를 만들지 못해 고정 응답 클라이언트가 대신 돌고 있다는 뜻이다. Base URL과 키를 확인하고 다시 저장한다. 이 상태로 수집하면 사실이 고정 문구로 채워진다.

**설정에 "오프라인 (키 미설정)"이라고 나온다.** OpenAI 호환에서 Base URL을 비운 채 키도 안 넣은 경우다. 로컬 서버면 Base URL을 넣고, OpenAI 자체면 키를 넣는다.

**LM Studio인데 "LLM request failed: connection refused".** 서버가 안 켜졌거나 포트가 다르다. `curl http://localhost:1234/v1/models`로 먼저 확인. WSL이면 §2-3.

**"LLM response hit the N-token limit".** thinking 모델이 추론에 예산을 다 썼다. LM Studio에서 thinking을 끄거나 다른 모델을 쓴다. (분류 예산은 4096 토큰.)

**수집했는데 대기열이 비어 있다.** "기기 안에서만 사용" 정책이면 추출이 안 된다. 그 외는 [`collecting.md` §6](collecting.md#6-자주-밟는-함정).
