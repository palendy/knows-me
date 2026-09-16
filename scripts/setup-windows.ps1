# knows-me — Windows 빌드 환경 한 번에 준비하기
#
#   PowerShell(관리자 권한 불필요)에서:
#   Set-ExecutionPolicy -Scope Process Bypass; .\scripts\setup-windows.ps1
#
# 하는 일: winget 으로 Rust(rustup) · Node.js LTS · Visual Studio C++ Build Tools ·
# WebView2 런타임 확인/설치 → npm install. 이미 있는 것은 건너뛴다.
$ErrorActionPreference = "Stop"

function Say($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Has($cmd) { return [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

if (-not (Has "winget")) {
  Write-Host "winget 이 없습니다. Microsoft Store에서 '앱 설치 관리자'를 설치한 뒤 다시 실행하세요."
  exit 1
}

# 1. C++ 빌드 도구 (Rust MSVC 툴체인의 링커). 이미 VS/Build Tools 가 있으면 건너뜀.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$hasCpp = (Test-Path $vswhere) -and ((& $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath) -ne $null)
if (-not $hasCpp) {
  Say "Visual Studio Build Tools (C++ 데스크톱 워크로드) 설치 — 수 분 걸립니다"
  winget install --id Microsoft.VisualStudio.2022.BuildTools --silent --accept-package-agreements --accept-source-agreements `
    --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
} else { Say "C++ 빌드 도구 있음" }

# 2. WebView2 런타임 (Windows 11 은 기본 포함, Windows 10 은 없을 수 있음)
$wv2 = Get-ItemProperty "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" -ErrorAction SilentlyContinue
if (-not $wv2) {
  Say "WebView2 런타임 설치"
  winget install --id Microsoft.EdgeWebView2Runtime --silent --accept-package-agreements --accept-source-agreements
} else { Say "WebView2 런타임 있음" }

# 3. Rust
if (-not (Has "cargo")) {
  Say "Rust 설치 (rustup)"
  winget install --id Rustlang.Rustup --silent --accept-package-agreements --accept-source-agreements
  $env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
} else { Say "Rust 있음: $(cargo --version)" }

# 4. Node.js LTS
if (-not (Has "node")) {
  Say "Node.js LTS 설치"
  winget install --id OpenJS.NodeJS.LTS --silent --accept-package-agreements --accept-source-agreements
  $env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
} else { Say "Node.js 있음: $(node --version)" }

# 5. 프론트엔드 의존성
Say "npm install"
Set-Location (Join-Path $PSScriptRoot "..")
npm install

Say "완료. 새 터미널을 열고(PATH 갱신) 다음 명령으로 실행/빌드하세요"
Write-Host "  npx tauri dev      # 개발 실행"
Write-Host "  npx tauri build    # 배포 번들 → desktop\target\release\bundle\  (msi, nsis 설치 파일)"
