# 빌드 가이드 — Linux / Windows

> 대상: 이 리포를 처음 받은 사람. 앱을 개발 모드로 띄우고, 배포 번들을 만드는 방법.
> 사용법은 [`usage.md`](usage.md).

## 0. 무엇이 필요한가

knows-me는 **Tauri 2** 앱이다. 세 층으로 되어 있고 각각 도구가 다르다.

| 층 | 위치 | 도구 |
|---|---|---|
| Rust 코어 라이브러리 | `src-tauri/` | Rust stable (`cargo`) |
| 데스크탑 셸 (Tauri 2) | `desktop/` | Rust + **플랫폼 웹뷰 라이브러리** |
| 프론트엔드 (React/TS) | `src/` | Node.js 20 이상 + npm |

`npx tauri dev` / `npx tauri build` 한 명령이 세 층을 다 묶어 준다. 코어 라이브러리만 빌드·테스트할 때는 웹뷰 라이브러리가 필요 없다.

## 1. Linux (Ubuntu 22.04 / 24.04, Debian, WSL2)

### 자동

```bash
bash scripts/setup-linux.sh
```

### 수동

```bash
# Tauri 시스템 의존성
sudo apt-get update
sudo apt-get install -y build-essential curl wget file pkg-config \
  libwebkit2gtk-4.1-dev libssl-dev libxdo-dev \
  libayatana-appindicator3-dev librsvg2-dev

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Node.js (nvm 예시)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
nvm install --lts

# 프론트엔드 의존성
npm install
```

다른 배포판(Fedora, Arch 등)의 패키지 이름은 <https://tauri.app/start/prerequisites/#linux>.

### 실행 / 빌드

```bash
npx tauri dev      # 개발 실행 (Vite 핫리로드 + 데스크탑 창)
npx tauri build    # 배포 번들 → desktop/target/release/bundle/{deb,rpm,appimage}/
```

### WSL2에서

- **GUI**: Windows 11의 WSLg가 있으면 `npx tauri dev`로 창이 그대로 뜬다 (`echo $DISPLAY`가 `:0`이면 된다).
- **LM Studio가 Windows에 있으면** WSL 안의 앱에서 `localhost`로는 안 닿는다. [`usage.md` §2-3](usage.md#2-3-wsl에서-windows의-lm-studio에-붙기)을 본다.
- Windows용 설치 파일(.msi)은 WSL에서 만들 수 없다. Windows에서 직접 빌드하거나 CI 결과물을 쓴다.

## 2. Windows 10 / 11

### 자동

PowerShell(관리자 권한 불필요):

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\scripts\setup-windows.ps1
```

### 수동

1. **Visual Studio Build Tools** — "C++를 사용한 데스크톱 개발" 워크로드. <https://visualstudio.microsoft.com/visual-cpp-build-tools/>
   또는 `winget install Microsoft.VisualStudio.2022.BuildTools`
2. **WebView2 런타임** — Windows 11은 기본 포함. Windows 10은 <https://developer.microsoft.com/microsoft-edge/webview2/> 에서 Evergreen Bootstrapper.
3. **Rust** — `winget install Rustlang.Rustup` (MSVC 툴체인이 기본).
4. **Node.js LTS** — `winget install OpenJS.NodeJS.LTS`
5. 새 터미널을 열고(PATH 갱신) 리포 루트에서 `npm install`

### 실행 / 빌드

```powershell
npx tauri dev      # 개발 실행
npx tauri build    # 배포 번들 → desktop\target\release\bundle\{msi,nsis}\
```

## 3. 코어만 빌드·테스트 (GUI 도구 없이)

Rust 코어는 Tauri에 의존하지 않는다. CI 샌드박스든 서버든 Rust만 있으면 된다.

```bash
cd src-tauri
cargo test                       # 오프라인 (LLM은 canned 응답)
cargo test --features llm-http   # HTTP LLM 클라이언트 포함
cargo run                        # 헤드리스 데모 (온보딩→잠금해제→암호화 저장→마스킹)
```

`--release`로 테스트를 돌리면 dev 전용 Gmail 픽스처 모듈 때문에 컴파일이 실패한다. 테스트는 기본(debug) 프로필로 돌린다.

## 4. 빌드 피처

| 피처 | 의미 | 데스크탑 기본값 |
|---|---|---|
| `llm-http` | URL로 연결하는 LLM 클라이언트 (LM Studio·Ollama·OpenRouter·OpenAI·Anthropic) | **켜짐** |
| `notion-http` | 실제 Notion API 커넥터 | **켜짐** |

로컬 Claude Code CLI 백엔드는 피처 없이 항상 들어 있다. 완전 오프라인 셸이 필요하면 `npx tauri build -- --no-default-features`.

코어의 example(`llm_probe`, `chat_demo` 등)은 코어 크레이트 기준이라 `--features llm-http`를 직접 붙인다.

## 5. CI — GitHub Actions

[`.github/workflows/build.yml`](../.github/workflows/build.yml):

- 모든 push / PR: 코어 테스트(피처 on/off) + 프론트 typecheck·테스트
- `main` push, `v*` 태그, 수동 실행: **Linux·Windows 번들** 생성 → Actions 아티팩트 업로드
- `v*` 태그: 번들을 GitHub Release에 첨부

릴리스를 내려면:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

## 6. 문제 해결

**`webkit2gtk-4.1` not found (Linux)** — `libwebkit2gtk-4.1-dev`가 없다. §1의 apt 명령. Ubuntu 20.04는 4.1이 없으므로 22.04 이상을 쓴다.

**`link.exe not found` (Windows)** — C++ Build Tools가 없거나 PATH가 갱신되지 않았다. 설치 후 새 터미널.

**흰 화면** — `cargo build`로 만든 바이너리를 직접 실행했을 때. 프론트 에셋이 임베드되지 않는다. 항상 `npx tauri dev` / `npx tauri build`를 쓴다.

**`npm run dev`로 띄웠는데 아무것도 저장이 안 된다** — 그건 브라우저용 목(mock) 모드다. UI 확인용이지 백엔드가 붙지 않는다.

**AppImage 생성 실패 (Linux)** — `linuxdeploy` 다운로드가 막힌 네트워크. `.deb`만 필요하면 `npx tauri build --bundles deb`.

**Vite가 EBUSY로 죽는다 (Windows)** — Rust 빌드 출력 디렉터리를 감시하다 생기는 문제. `vite.config.ts`에 이미 제외돼 있으니, 리포를 최신으로 받았는지 확인.
