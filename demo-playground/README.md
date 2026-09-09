# knows-me — 라이브 데모 플레이그라운드

심사자가 **우리 제품을 직접 실행해보게** 하는 페이지. 팀원 몇 명이 각자 앱에서
지식을 공유(터널+토큰)해 두면, 이 페이지에서:

- **Tier 2 (메인):** 심사자가 자연어로 질문 → 서버측 Claude 에이전트가 우리 MCP
  4툴(`list_categories`/`search_knowledge`/`get_page`/`get_guide`)을 **실제로 호출**해
  선택한 팀원의 지식을 조회하고 답한다. 설치 0.
- **Tier 3 (고급):** `claude mcp add` 한 줄로 심사자가 **자기 Claude Code**에 직접 연결.

Rust 코드는 건드리지 않는다. 터널 URL·토큰은 서버리스 함수의 **env에만** 있고
브라우저로 나가지 않는다(Tier 3의 직접연결 명령만 예외 — 아래 보안 주의).

## 구조

```
demo-playground/
  index.html / styles.css / app.js   정적 플레이그라운드(오너 선택 + 챗)
  api/chat.mjs                       Tier2: 서버측 에이전트 루프(MCP 4툴 호출)
  api/connect.mjs                    Tier3: 오너별 직접연결 명령(토큰 포함)
  api/owners.mjs                     오너 목록(id/name만, 비밀 제외)
  lib/owners.mjs                     env → 오너(터널 URL+토큰)
  package.json / .env.example
```

## 로컬에서 확인 (데스크톱 앱 없이 전체 루프 스모크)

`mock-mcp.mjs`가 실제 공유 리스너를 흉내 낸다(Bearer 필수 + 토큰 스코프 + 봉투).
데스크톱 앱 없이 브라우저→LLM(OpenAI 등)→MCP 전체 루프를 확인할 수 있다.

```bash
# 터미널 1 — 가짜 MCP 서버 (의존성 없음)
cd demo-playground
node mock-mcp.mjs            # OWNER1_* 예시를 출력한다

# 터미널 2 — 플레이그라운드
cp .env.example .env.local   # LLM_BASE_URL/LLM_API_KEY/LLM_MODEL 넣고, OWNER1_*을 위 출력값으로 교체
npx vercel dev               # http://localhost:3000  (첫 실행 시 프로젝트 링크 물어봄 → 새로 생성/연결)
```

브라우저에서 "어떤 범주를 물어볼 수 있어?" → deploy/onboarding만, "배포 규칙 있어?" → 답,
"연봉 협상 메모 있어?" → **접근 불가**로 나오면 정상. (`mock-mcp.mjs`는 스모크 전용.)

실제 앱으로 확인하려면 `OWNER1_URL/TOKEN`을 앱 공유 UI가 준 터널 URL·토큰으로 바꾸면 된다.

## 배포 (Vercel)

1. 이 폴더를 Vercel 프로젝트로 연결 (**Root Directory = `demo-playground`**, 프레임워크 프리셋 = Other).
2. 환경변수 설정 — `.env.example` 참고 (검증된 구성 = OpenAI):
   - `LLM_BASE_URL=https://api.openai.com/v1`
   - `LLM_API_KEY` (OpenAI 키, 필수)
   - `LLM_MODEL=gpt-4o-mini`
   - `OWNER1_NAME` / `OWNER1_URL` / `OWNER1_TOKEN`
   - `OWNER2_NAME` / `OWNER2_URL` / `OWNER2_TOKEN`
3. `npx vercel --prod` (또는 push 자동배포). 제출 페이지 링크 = 이 배포 URL.

> **LLM 게이트웨이:** 챗은 OpenAI 호환 엔드포인트로 호출한다. `LLM_BASE_URL`로
> OpenAI(검증됨)·Google Gemini·OpenRouter 등을 가리킬 수 있다(`.env.example` 옵션 A/B/C).
> **반드시 tool-calling(function-calling) 지원 모델**일 것. Vercel 함수 상한은 60s.
> (OpenRouter는 `OPENROUTER_*` 하위호환 인식 — 단 저한도 키는 402 예약 이슈 주의.)

## 심사 당일 런북 (30분 라이브)

**5분 전 — 오너들(각자 자기 기기):**
1. knows-me 앱 열기 → 볼트 잠금 해제
2. 설정 “공유” 탭 → MCP 켜기 → 터널 start → **URL 복사**
3. 컨슈머 토큰 발급(부여 범주 선택) → **토큰 복사** (한 번만 보임)

**운영자(1명):**
4. Vercel env에 각 오너의 `OWNER{i}_URL` / `OWNER{i}_TOKEN` 채우기
5. `npx vercel --prod` **1회** 배포 (~1분). 이후 30분간 그대로.
6. 제출 페이지 링크 = Vercel URL. 열어서 오너 전환 + 질문 스모크 1회 확인.

**끝나면:**
7. 각 오너 앱에서 **토큰 폐기(revoke)** + 터널 stop.

## 보안 주의

- `api/connect`(Tier 3)는 **의도적으로 토큰을 노출**한다(직접연결에 필요).
  30분 스코프 데모 전제 — **끝나면 반드시 revoke**. 상시 공개 금지.
- 노출되는 지식은 오너가 **공유(Shared)로 승인한 범주만**. 서버가 스코프를 강제하고,
  부여 안 된 범주·Private은 컨슈머에게 not-found로 보인다.
- `LLM_API_KEY`·오너 토큰은 서버측 env에만. `.env.local`은 gitignore됨.

## 다른 호스트로 옮기려면

프록시 함수(`api/*.mjs`)는 얇다. Cloudflare Pages는 `functions/` 디렉토리 +
`onRequest` 시그니처로, Netlify는 `netlify/functions/`로 래퍼만 바꾸면 된다.
정적 파일(`index.html`/`styles.css`/`app.js`)과 `lib/owners.mjs`는 그대로.
