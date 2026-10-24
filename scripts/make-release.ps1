# Builds the release installer into dist\ (copied from CoolerScreen's scripts\make-release.ps1; no ffmpeg, cs2 or portable zip):
#   dist\UnifiedControl_<version>_x64-setup.exe   NSIS installer (per user, no administrator)
# Needs: Rust (stable, MSVC), Node 18+ (npm install fetches the Tauri CLI). Run from the repo root:
#   powershell -ExecutionPolicy Bypass -File scripts\make-release.ps1
# Only the orchestrator builds releases (versions, tags and installs are theirs).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$pkg = Get-Content package.json -Raw | ConvertFrom-Json
$ver = $pkg.version
$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null

if (-not (Test-Path 'node_modules\.bin\tauri.cmd')) { npm install; if ($LASTEXITCODE -ne 0) { throw 'npm install failed' } }

npx tauri build
if ($LASTEXITCODE -ne 0) { throw 'tauri build failed' }
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'src-tauri\target' }
# The bundler names the installer after the product name, with a space ("Unified Control_<ver>_x64-setup.exe"); the copy in
# dist\ drops the space, because that is the only name `--update` and tools\install-when-idle.ps1 accept (relaunch.rs).
$setup = Join-Path $target "release\bundle\nsis\Unified Control_${ver}_x64-setup.exe"
if (-not (Test-Path $setup)) { throw "installer not found: $setup" }
Copy-Item $setup (Join-Path $dist "UnifiedControl_${ver}_x64-setup.exe") -Force
Get-ChildItem $dist -File | ForEach-Object { '{0}  {1:N1} MB' -f $_.FullName, ($_.Length / 1MB) }
