# 수집 가이드 — 무엇이 언제 들어오고, 안 들어올 때 뭘 보나

> 대상: knows-me 오너 / 개발자. 이 문서는 **수집(collect) → 가공(process) → 지식베이스·큐**
> 경로를 실제로 돌리고 진단하는 법이다. 컨셉은 [`README.md`](../README.md),
> 공유 계약은 [`team-sharing.md`](team-sharing.md).

## 0. 한 장 요약

```
세션 파일 → [커넥터] 다이제스트 → [마스킹] → [LLM] 요약·분류 → 확정 사실 / 인터뷰 큐
              ↑ 커서(워터마크 2개)                ↑ 기본 백엔드 = 로컬 Claude Code CLI
```

| 궁금한 것 | 보는 곳 |
|---|---|
| 확정된 사실이 뭐가 있나 | `cargo run --release --example dump_facts` |
| 확인 대기 중인 게 뭐가 있나 | `cargo run --release --example dump_queue` |
| 이 세션 하나가 뭘 뽑는지 | `cargo run --release --example ingest_roots` |
| 특정 세션을 앱 볼트에 넣기 | `cargo run --release --example ingest_into_vault` |

## 1. 평소 수집 — 앱의 「수집」 버튼

앱에서 수집을 누르면 세션 루트(`~/.claude/projects`, `~/.codex/sessions`)를 훑어
**한 번에 최대 30개** 파일을 가져온다. 전부 안 가져오는 건 의도된 것이다 —
가공 한 건마다 LLM 왕복이 한 번(이상) 붙기 때문에, 수백 개짜리 디렉터리를 한 번에
돌리면 몇 시간이 걸린다.

### 커서는 워터마크 두 개다

```
        oldest                          newest
  ────────┼──────── 이미 처리됨 ─────────┼────────→ (시간)
   백필 대상                              신규 대상
```

- `newest`보다 새 파일 → **전진 수집**
- `oldest`보다 오래된 파일 → **백필**(남은 예산으로)
- 그 사이 → 처리 완료로 간주, 건너뜀

그래서 「수집」을 반복해서 누르면 **중복 수집이 아니라 백필이 진행**된다.
`수집 27 · 건너뜀 1`처럼 보이면 재수집이 아니라 밀린 작업이 줄고 있는 것이다.
남은 개수는 수집 결과의 `remaining`으로 표시된다.

### ⚠️ 워터마크는 단조적이다

`newest`는 앞으로만, `oldest`는 뒤로만 움직인다. 되감기가 없다.

**따라서 오래된 프로젝트 디렉터리를 일반 수집 경로로 지정하면 안 된다.** 백필
경계가 그 파일의 시각까지 한 번에 끌려 내려가고, **그 사이에 있던 세션 전부가
"처리 완료"로 표시되어 영영 들어오지 않는다.** 특정 세션만 넣고 싶으면 §4를 쓴다.

## 2. LLM 백엔드

기본값은 **로컬 Claude Code CLI**(`claude -p`)다. 별도 API 키도, `llm-http`
피처도 필요 없다 — 이 프로젝트를 쓰는 사람은 이미 Claude를 쓰고 있다는 전제다.

`.env`(`.env.example` 참고)로 바꾼다. `.env`는 **모든 빌드에서** 읽힌다:

```bash
LLM_PROVIDER=claude-cli
CLAUDE_CLI_MODEL=claude-sonnet-5   # 급하면 claude-haiku-4-5
```

알아 둘 것:

- **느리다.** 호출 한 번에 에이전트 세션이 하나 뜬다 — 측정값 약 12초. 세션 하나에서
  사실을 여러 개 뽑으므로 분류 호출도 그만큼 늘어난다. 30건 수집이 15분쯤 걸린다.
- **나가는 건 같다.** 로컬 CLI를 거칠 뿐 텍스트는 Anthropic에 도달한다. 마스킹
  계약은 HTTP 백엔드와 똑같이 적용된다(호출자는 `MaskedText`만 넘긴다).
- 설정 화면의 "현재 환경"과 예제들의 `llm :` 헤더는 실제로 선택된 백엔드를
  표시한다. 라벨과 선택 로직(`build_client`)이 같은 함수를 공유하므로 어긋나지
  않는다. 환경변수로 추측하던 시절엔 모든 호출이 로컬 CLI로 가는 동안 헤더가
  "anthropic"이라고 적혀 있었다.

HTTP 백엔드(OpenRouter 등)를 쓰려면 `--features llm-http` + 해당 키가 필요하다.

## 3. 뭐가 들어왔는지 보기

```bash
# 확정 사실 + 주제 페이지
KNOWSME_DATA_DIR=.knowsme-demo cargo run --release --manifest-path src-tauri/Cargo.toml --example dump_facts

# 인터뷰 큐 (후보 본문 전문)
KNOWSME_DATA_DIR=.knowsme-demo cargo run --release --manifest-path src-tauri/Cargo.toml --example dump_queue
```

**둘 다 봐야 한다.** 분류기가 `uncertain`으로 판정한 후보는 확정 사실이 아니라
큐로 간다. 확정 목록만 보면 *"추출이 안 됐다"* 와 *"추출됐고 확인을 기다린다"* 가
똑같아 보인다. 실제로 겪은 사례:

```
$ dump_facts   | grep 아바타카드   → 없음        ← "추출 실패"로 오독
$ dump_queue   | grep 아바타카드   → 확인 [Concept] «avatar-card, ai-dlc»
                                     "아바타 카드"라는 개념을 사용하며, 이를 AWS
                                     AI-DLC의 persona/sub-agent 개념 자리에…
```

## 4. 특정 세션만 넣기

### 4-1. 먼저 스크래치 볼트에서 확인 (`ingest_roots`)

"이 세션이 X를 뽑아 주나?"는 실제 파이프라인에 통과시켜 봐야 안다. 자동 탐지
배치로는 그 세션에 도달하는 데만 수 시간이 걸린다.

```bash
KNOWSME_SESSION_ROOTS=~/.claude/projects/<프로젝트-디렉터리> \
KNOWSME_DATA_DIR=/tmp/vault-확인용 \
  cargo run --release --manifest-path src-tauri/Cargo.toml --example ingest_roots
```

루트는 `:`로 여러 개 줄 수 있다. `$HOME`을 위조하는 방식은 쓰지 말 것 — Claude
CLI의 자격증명까지 같이 옮겨가 LLM 백엔드가 죽고, rustup도 깨진다.

### 4-1½. 진행 상황 보기 — 터미널은 끝날 때까지 조용하다

두 예제 모두 진행 표시가 없고 결과를 마지막에 한 번에 찍는다. `수집 시작…`에서
10분 넘게 멈춰 보여도 대개는 정상이다. 살아 있는지는 볼트를 보면 안다:

```bash
V="$KNOWSME_DATA_DIR/store"
ls "$V/dHJhbnNmZXIubG9n" | wc -l   # 전송 로그 = 지금까지의 LLM 호출 수
ls "$V/ZmFjdHM"          | wc -l   # 확정 사실
ls "$V/cXVldWU"          | wc -l   # 인터뷰 큐
pgrep -f 'claude -p'               # 호출 중이면 자식 프로세스가 보인다 (매번 새 pid)
```

전송 로그 개수가 늘고 있으면 진행 중이다. (디렉터리 이름은 네임스페이스의
base64다 — `dHJhbnNmZXIubG9n` = `transfer.log`.)

**소요 시간 어림**: 세션당 호출은 1회가 아니다. **요약 1회 + 뽑힌 사실마다 분류
1회**라 세션당 평균 5~6회가 된다. 세션 15개면 90회 안팎, 호출당 약 12초로
**15~20분**. 세션 수 × 12초로 잡으면 크게 빗나간다.

> `find -newermt '-2 minutes'`로 "최근 변경"을 재려 하면 시간대 때문에 0건이
> 나올 수 있다. `ls -lt`로 마지막 파일의 시각을 직접 보는 편이 확실하다.

### 4-2. 앱 볼트에 주입 (`ingest_into_vault`)

커서를 **건드리지 않고** 지정한 루트를 볼트에 넣는다(§1의 위험 회피).

```bash
# 1) 앱을 닫는다 — 암호화 저장소는 잠금 없는 파일이다. 두 프로세스가 쓰면 안 된다.
# 2) 볼트를 백업한다.
cp -R "$HOME/Library/Application Support/app.knowsme.desktop" \
      "$HOME/Library/Application Support/app.knowsme.desktop.bak"

# 3) 비밀번호는 셸에서 직접 입력한다 (에코 없음, 히스토리에 남지 않음).
read -rs "?볼트 비밀번호: " KNOWSME_DEMO_PASSWORD && export KNOWSME_DEMO_PASSWORD

KNOWSME_SESSION_ROOTS=~/.claude/projects/<프로젝트-디렉터리> \
KNOWSME_DATA_DIR="$HOME/Library/Application Support/app.knowsme.desktop" \
  cargo run --release --manifest-path src-tauri/Cargo.toml --example ingest_into_vault

unset KNOWSME_DEMO_PASSWORD
```

끝나면 앱을 다시 띄우고 인터뷰 큐에서 확인하면 확정 사실이 된다.

두 예제 모두 `KNOWSME_DATA_DIR`·`KNOWSME_DEMO_PASSWORD`에 기본값이 없다. 실볼트를
다루는 도구에 기본값을 두면 오타 하나로 **두 번째 볼트가 조용히 생긴다**.

## 5. 자주 밟는 함정

**흰 화면이 뜬다.**
`cargo build`만으로는 Tauri의 프론트엔드 임베딩이 다시 돌지 않는다. `dist/`만
바뀌면 빌드 스크립트가 재실행되지 않아, 바이너리 안에는 예전 에셋 해시를 가리키는
`index.html`이 남는다. `target/release/knows-me`를 직접 실행하지 말고
`npx tauri dev`(개발) / `npx tauri build`(배포)를 쓴다. `npm run dev`(1420 포트)는
**목 어댑터**라 백엔드가 붙지 않는다 — UI 확인용이지 동작 확인용이 아니다.

**knows-me가 자기 프롬프트를 수집한다.**
CLI 백엔드는 Claude Code를 구동하고, Claude Code는 그 요청을 자기 트랜스크립트에
user 턴으로 기록한다 — 이 커넥터가 스캔하는 바로 그 디렉터리에. 지금은 프롬프트
상수의 첫 줄로 걸러낸다. 프롬프트를 고쳐 써도 필터가 따라가지만, **새 프롬프트를
추가하면 `own_prompt_openers()`에도 추가**해야 한다.

**용어가 세션에 잔뜩 나오는데 안 뽑힌다.**
`grep -c`로 센 숫자는 오해를 부른다. 커넥터가 LLM에 넘기는 건 트랜스크립트 전문이
아니라 **다이제스트**(오너 발화 + 프로젝트 경로 + 도구/명령/파일 프로필)다. 도구
결과 안에만 있는 용어는 애초에 모델이 보지 못한다. 역할별로 세어 볼 것:

```bash
python3 - <<'EOF'
import json, collections, sys
term, path = sys.argv[1], sys.argv[2]
c = collections.Counter()
for line in open(path, errors='replace'):
    if term not in line: continue
    try: o = json.loads(line)
    except: continue
    if o.get('type') != 'user': continue
    content = (o.get('message') or {}).get('content')
    blocks = [content] if isinstance(content, str) else content or []
    for b in blocks:
        if isinstance(b, str): c['오너 발화'] += b.count(term)
        elif b.get('type') == 'text': c['오너 발화'] += b.get('text','').count(term)
        else: c['도구 결과'] += json.dumps(b, ensure_ascii=False).count(term)
print(dict(c))
EOF
```

오너 발화가 0이면 그 세션으로는 안 나온다. 다른 프로젝트 디렉터리를 찾아야 한다.

**오너 발화는 마지막 12개만 실린다.**
다이제스트는 세션의 끝(결론이 나는 곳)을 우선한다. 아주 긴 세션의 초반에만 나온
이야기는 빠질 수 있다 — `MAX_PROMPTS`(`ingestion/connectors/session.rs`).
