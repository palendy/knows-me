# 빌드 지침 (전 유닛)

## 사전 요구
- Rust (rustup) — 검증 환경 1.97.1
- Node.js 20+ — 검증 환경 24.5.0
- Tauri 데스크탑 빌드 시: 플랫폼 webview 툴체인 (macOS는 Xcode CLT)

## 코어 라이브러리 (U1~U4 전부)
```bash
cd src-tauri
cargo build
cargo build --features llm-http   # 실제 Anthropic 클라이언트 포함 (선택)
```
> `llm-http` 없이 기본 빌드하면 네트워크 없이 완전히 빌드·테스트된다.

## 프론트엔드
```bash
npm install
npm run build        # tsc --noEmit + vite build -> dist/
```

## 데스크탑 앱 (Tauri 2)
```bash
npm install
npx tauri dev        # 개발 실행
npx tauri build      # 배포 번들
```
> ⚠️ 현재 데스크탑 셸은 U1 command 7개만 등록한다. U2·U3·U4 기능은 앱에서 보이지 않는다 — `integration-verification-report.md` §3 참조.

## 시크릿
코드에 시크릿을 넣지 않는다. LLM 자격증명은 U1 Vault 또는 환경변수로 주입한다.
