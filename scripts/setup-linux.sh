#!/usr/bin/env bash
# knows-me — Linux(Ubuntu/Debian, WSL 포함) 빌드 환경 한 번에 준비하기
#
#   bash scripts/setup-linux.sh
#
# 하는 일: Tauri 시스템 라이브러리(apt) → Rust(rustup) → Node.js 확인 → npm install
# 이미 있는 것은 건너뛴다. sudo 비밀번호를 한 번 물어본다.
set -euo pipefail

say() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }

if ! command -v apt-get >/dev/null 2>&1; then
  echo "apt-get 이 없는 배포판입니다. https://tauri.app/start/prerequisites/ 의 목록을 수동으로 설치한 뒤"
  echo "이 스크립트의 Rust/Node 단계만 이어서 실행하세요."
fi

if command -v apt-get >/dev/null 2>&1; then
  say "Tauri 시스템 의존성 설치 (apt)"
  sudo apt-get update
  sudo apt-get install -y \
    build-essential curl wget file pkg-config \
    libwebkit2gtk-4.1-dev libssl-dev libxdo-dev \
    libayatana-appindicator3-dev librsvg2-dev
fi

if ! command -v cargo >/dev/null 2>&1; then
  say "Rust 설치 (rustup, stable)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
else
  say "Rust 있음: $(cargo --version)"
fi
rustup component add rustfmt >/dev/null 2>&1 || true

if ! command -v node >/dev/null 2>&1; then
  say "Node.js 가 없습니다. LTS(20 이상)를 설치하세요: https://nodejs.org 또는 nvm"
  echo "  curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash && nvm install --lts"
  exit 1
else
  say "Node.js 있음: $(node --version)"
fi

say "프론트엔드 의존성 설치 (npm install)"
cd "$(dirname "$0")/.."
npm install

say "완료. 다음 명령으로 실행/빌드하세요"
echo "  npx tauri dev      # 개발 실행"
echo "  npx tauri build    # 배포 번들 → desktop/target/release/bundle/"
