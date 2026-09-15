# 사용 가이드

> 대상: 앱을 빌드한 다음 실제로 쓰려는 사람. 빌드는 [`build.md`](build.md).

## 1. 처음 실행

```bash
npx tauri dev            # 개발 실행
# 또는 설치한 앱(.msi / .deb / .AppImage) 실행
```

1. **비밀번호 설정** — 첫 화면에서 보관함 비밀번호를 정한다. 이 비밀번호에서 암호화 키를 만들고(Argon2id), 저장되는 모든 것을 AES-256-GCM으로 암호화한다. 평문 키는 디스크에 남지 않는다. **잊으면 복구할 수 없다.**
2. **잠금 해제** — 이후 실행할 때마다 같은 비밀번호를 넣는다.

데이터 위치:

| OS | 경로 |
|---|---|
| Linux | `~/.local/share/app.knowsme.desktop/` |
| Windows | `%APPDATA%\app.knowsme.desktop\` |

전부 암호화돼 있다. 지우면 처음부터 다시 시작한다.

## 2. LLM 연결

지식 추출(요약·분류)과 페르소나 대화에 LLM이 쓰인다. **설정 → AI 모델**에서 고른다.

| 선택지 | 언제 | 필요한 것 |
|---|---|---|
| **로컬 Claude Code** (기본) | Claude Code가 이미 설치·로그인돼 있을 때 | 없음. `echo ping \| claude -p`가 답하면 된다 |
| **OpenAI 호환** | 내 PC의 LM Studio·Ollama, 또는 OpenRouter·OpenAI | URL (+ 클라우드는 API 키) |
| **Anthropic API** | Anthropic 키를 직접 쓸 때 | API 키 |

저장하면 즉시 반영된다. "현재 사용 중"에 실제로 선택된 백엔드가 표시된다.

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

**로컬 Claude Code** 선택 → 감지된 설치 중 하나(Windows 네이티브 / WSL) 선택 → 모델(`claude-opus-5` 권장, 빠른 건 `claude-haiku-4-5`).

호출마다 `claude -p` 세션이 하나 뜨므로 한 호출에 10초 남짓 걸린다. 텍스트는 Anthropic에 도달한다는 점에서 클라우드 백엔드와 같다.

### 2-7. 데이터 전송 정책

**설정 → 데이터 전송**:

- **개인정보를 가리고 전송** (권장) — 이메일·경로·토큰 같은 식별자를 마스킹한 뒤 보낸다.
- **원본 그대로 전송**
- **기기 안에서만 사용** — LLM에 아무것도 보내지 않는다 (추출은 사실상 멈춘다).

무엇을 보냈는지는 **설정 → 전송 기록**에 남는다. LM Studio처럼 로컬 LLM이면 PC 밖으로 나가는 것은 없다.

## 3. 소스 연결과 수집

**설정 → 연결 소스**.

| 소스 | 어떻게 |
|---|---|
| Claude (Claude Code 세션) | 자동. `~/.claude/projects/`를 읽는다 |
| Codex 세션 | 자동. `~/.codex/sessions/`를 읽는다 |
| 파일 | 폴더 지정 (텍스트·마크다운) |
| Notion | 연결 → Notion Integrations에서 만든 내부 통합 토큰 입력 |
| Gmail | 연결 → Gmail 주소 + 앱 비밀번호 |
| Confluence · Jira · Knox Mail | 자리만 있음 (아직 미구현) |

- **소스 수집** — 각 소스에서 한 묶음씩(증분) 가져와 LLM으로 요약·분류하고 대기열에 넣는다. 금방 끝난다.
- **끝까지 수집** — 남은 기록을 전부. 처음이면 오래 걸린다 (LM Studio 기준 세션 하나에 1분 남짓).
- **수집 범위** — 어떤 프로젝트의 세션을 모을지 고른다. 처음엔 프로젝트 한두 개로 좁혀 시작하는 게 빠르다.

자세한 것과 문제 진단은 [`collecting.md`](collecting.md).

## 4. 화면

| 탭 | 하는 일 |
|---|---|
| **대시보드** | 확정된 사실·주제 요약, 최근 수집 현황 |
| **대기열** | 후보를 하나씩 확인: 맞다/아니다, 공개/비공개, 범주. 인터뷰 질문에도 여기서 답한다 |
| **미니홈피** | 내 지식 위키를 사람이 읽는 형태로 |
| **지식 그래프** | 사실과 주제의 연결을 그래프로 |
| **페르소나** | "나라면 어떻게?"를 내 지식 기반으로 대화 |
| **설정** | 전송 정책·AI 모델·연결 소스·공유·전송 기록 |

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

**설정에 "오프라인 (키 미설정)"이라고 나온다.** OpenAI 호환에서 Base URL을 비운 채 키도 안 넣은 경우다. 로컬 서버면 Base URL을 넣고, OpenAI 자체면 키를 넣는다.

**LM Studio인데 "LLM request failed: connection refused".** 서버가 안 켜졌거나 포트가 다르다. `curl http://localhost:1234/v1/models`로 먼저 확인. WSL이면 §2-3.

**"LLM response hit the N-token limit".** thinking 모델이 추론에 예산을 다 썼다. LM Studio에서 thinking을 끄거나 다른 모델을 쓴다. (분류 예산은 4096 토큰.)

**수집했는데 대기열이 비어 있다.** "기기 안에서만 사용" 정책이면 추출이 안 된다. 그 외는 [`collecting.md` §6](collecting.md#6-자주-밟는-함정).
