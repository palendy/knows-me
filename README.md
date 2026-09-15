# knows-me

![knows-me 데모](docs/demo.gif)

**내 작업 기록에서 "나에 대한 지식"을 뽑아 개인 위키로 쌓고, 내가 허락한 부분만 팀원의 AI가 물어볼 수 있게 하는 데스크탑 앱.**

## 뭐 하는 앱인가

Claude Code·Codex 같은 AI 코딩 도구를 쓰다 보면, 내가 어떤 프로젝트를 어떻게 띄우고, 어떤 규칙을 지키고, 무엇을 결정했는지가 세션 기록에 남는다. knows-me는 그 기록을 읽어서:

1. **나에 대한 사실 후보를 뽑는다** — "이 프로젝트는 `./run.sh`로 띄운다", "PR은 squash merge만 한다" 같은 것.
2. **내가 확인한다** — 앱의 대기열에서 맞다/아니다, 공개/비공개를 정한다. 확인 전에는 아무것도 지식이 되지 않는다.
3. **위키로 쌓는다** — 내 PC 안에 암호화해서 저장한다. 밖으로 나가지 않는다.
4. **내 AI와 팀원의 AI가 물어본다** — 나는 내 지식 전부를, 팀원은 내가 공개로 지정한 범위만 MCP로 질의한다.

한 줄로: **"이 사람이라면 어떻게 할까?"를 사람 대신 내 지식이 답한다.**

지식을 뽑는 데 쓰는 LLM은 골라 쓸 수 있다: 내 PC의 **LM Studio·Ollama**, 이미 설치된 **Claude Code**, 또는 **OpenAI 호환 API·Anthropic API**.

## 5분 안에 띄우기

### 1. 준비물 설치

| OS | 한 번에 |
|---|---|
| **Linux (Ubuntu/Debian, WSL 포함)** | `bash scripts/setup-linux.sh` |
| **Windows** | PowerShell에서 `.\scripts\setup-windows.ps1` |

스크립트는 Rust, Node.js, Tauri가 필요로 하는 시스템 라이브러리를 설치하고 `npm install`까지 해 준다. 직접 설치하려면 [docs/build.md](docs/build.md).

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
   - Base URL: `http://localhost:1234/v1`
   - API 키: **비워 둔다**
4. **저장**. "현재 사용 중"에 `google/gemma-4-12b (localhost:1234)`처럼 표시되면 연결된 것.

다른 LLM(Claude Code, Ollama, OpenRouter, Anthropic)과 WSL에서 쓸 때의 주의점은 [docs/usage.md](docs/usage.md#2-llm-연결).

### 4. 첫 수집

앱 → **설정** → **연결 소스** → **수집**. 로컬의 Claude Code(`~/.claude/projects/`)·Codex(`~/.codex/sessions/`) 기록을 읽어 후보를 만든다. 결과는 **대기열** 탭에 쌓이고, 거기서 하나씩 확인하면 위키가 된다.

## 배포용 빌드

```bash
npx tauri build
```

결과물은 `desktop/target/release/bundle/` 아래에 생긴다 — Linux는 `.deb`·`.rpm`·`.AppImage`, Windows는 `.msi`·설치용 `.exe`. GitHub Actions([`.github/workflows/build.yml`](.github/workflows/build.yml))가 main 브랜치와 `v*` 태그에서 두 OS 번들을 자동으로 만든다.

## 더 읽을 것

| 알고 싶은 것 | 문서 |
|---|---|
| 빌드 환경 상세, WSL, 문제 해결 | [docs/build.md](docs/build.md) |
| 화면별 사용법, LLM 설정 전부, 명령줄 도구 | [docs/usage.md](docs/usage.md) |
| 수집이 왜 안 되나, 무엇이 들어오나 | [docs/collecting.md](docs/collecting.md) |
| 팀원에게 MCP로 공유하기 | [docs/team-sharing.md](docs/team-sharing.md) |
| 왜 이렇게 설계했나 (원래 README) | [docs/concept.md](docs/concept.md) |

## 개발자용

```bash
cd src-tauri && cargo test && cargo test --features llm-http   # Rust 코어 (GUI 불필요)
npm test                                                        # 프론트엔드
npm run dev                                                     # 브라우저에서 UI만 (목 데이터)
```

구조: Rust 코어 라이브러리 `src-tauri/`(암호화 저장·수집·LLM 게이트웨이·MCP 서버), Tauri 2 셸 `desktop/`, React 프론트 `src/`. 라이선스 MIT.
