# Build the separate engine from an immutable private revision. No app starts.
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$pin=Get-Content -LiteralPath (Join-Path $projectRoot 'companions/cooler-engine.json') -Raw | ConvertFrom-Json
if ($pin.repository -ne 'https://github.com/oldbugo/coolerscreen.git' -or $pin.revision -notmatch '^[a-f0-9]{40}$') {throw 'Invalid cooler source pin'}
$sourceRoot=Join-Path $env:TEMP ('uc-cooler-source-'+$pin.revision)
if (-not (Test-Path -LiteralPath $sourceRoot)) {
    git clone --quiet --no-checkout $pin.repository $sourceRoot
    if ($LASTEXITCODE -ne 0) {throw 'Could not fetch private cooler source'}
    git -C $sourceRoot checkout --quiet --detach $pin.revision
    if ($LASTEXITCODE -ne 0) {throw 'Could not initialize pinned cooler checkout'}
}
git -C $sourceRoot cat-file -e ($pin.revision+'^{commit}')
if ($LASTEXITCODE -ne 0) {git -C $sourceRoot fetch --quiet origin $pin.revision; if ($LASTEXITCODE -ne 0) {throw 'Pinned cooler revision unavailable'}}
if (@(git -C $sourceRoot status --porcelain).Count) {throw 'Cooler checkout has local edits; preserved without checkout'}
git -C $sourceRoot checkout --quiet --detach $pin.revision
if ($LASTEXITCODE -ne 0) {throw 'Could not select pinned cooler source'}
$engineTarget=Join-Path $env:TEMP 'cs-target-connection-backend'
$previousTarget=$env:CARGO_TARGET_DIR
$previousConfig=$env:TAURI_CONFIG
try {
    $env:CARGO_TARGET_DIR=$engineTarget
    $env:TAURI_CONFIG='{"bundle":{"resources":[]}}'
    cargo build --manifest-path (Join-Path $sourceRoot 'src-tauri/Cargo.toml') --release --bin uc-cooler-worker --locked
    if ($LASTEXITCODE -ne 0) {throw 'Cooler worker build failed'}
} finally {$env:CARGO_TARGET_DIR=$previousTarget;$env:TAURI_CONFIG=$previousConfig}
$output=Join-Path $projectRoot 'src-tauri/runtime/cooler'
New-Item -ItemType Directory -Force -Path (Join-Path $output 'scripts') | Out-Null
Copy-Item -LiteralPath (Join-Path $engineTarget 'release/uc-cooler-worker.exe') -Destination $output -Force
$ffmpeg=Join-Path (Split-Path -Parent $projectRoot) 'CoolerScreen/node_modules/ffmpeg-static/ffmpeg.exe'
if ((Get-FileHash -LiteralPath $ffmpeg -Algorithm SHA256).Hash -ne $pin.ffmpegSha256) {throw 'Cooler ffmpeg differs from the reviewed binary'}
Copy-Item -LiteralPath $ffmpeg -Destination (Join-Path $output 'ffmpeg.exe') -Force
foreach ($name in @('ffmpeg.exe.LICENSE','ffmpeg.exe.README')) {Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $ffmpeg) $name) -Destination $output -Force}
Copy-Item -LiteralPath (Join-Path $sourceRoot 'LICENSE') -Destination (Join-Path $output 'CoolerScreen-LICENSE') -Force
foreach ($name in @('setup-winusb.ps1','restore-serial.ps1','reconnect.ps1')) {Copy-Item -LiteralPath (Join-Path $sourceRoot ('scripts/'+$name)) -Destination (Join-Path $output 'scripts') -Force}
Write-Output ('Staged pinned CoolerScreen engine '+$pin.revision+'. No app started.')
