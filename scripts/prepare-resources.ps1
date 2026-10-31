# Build-only runtime staging. Copies reviewed source/dependencies and the installed
# Node distribution; no authentication files or application settings are read.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$readerSource = Join-Path $projectRoot 'companions/usage-reader'
$runtimeRoot = Join-Path $projectRoot 'src-tauri/runtime'
$runtimeRoot = [IO.Path]::GetFullPath($runtimeRoot)
$allowedParent = [IO.Path]::GetFullPath((Join-Path $projectRoot 'src-tauri')) + [IO.Path]::DirectorySeparatorChar
if (-not $runtimeRoot.StartsWith($allowedParent, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $runtimeRoot) -ne 'runtime') { throw 'Runtime output is outside the expected workspace folder' }
if (-not (Test-Path -LiteralPath (Join-Path $readerSource 'reader.mjs'))) { throw 'Usage reader source is missing' }
if (Test-Path -LiteralPath $runtimeRoot) { Remove-Item -LiteralPath $runtimeRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $runtimeRoot 'usage-reader'), (Join-Path $runtimeRoot 'node') | Out-Null
foreach ($name in @('reader.mjs', 'engine.mjs', 'source', 'licenses', 'package.json', 'package-lock.json')) {
    Copy-Item -LiteralPath (Join-Path $readerSource $name) -Destination (Join-Path $runtimeRoot 'usage-reader') -Recurse -Force
}
# Install into the isolated build copy. Never npm-ci a source reader that could
# have its native rasterizer loaded by a running development smoke test.
Push-Location (Join-Path $runtimeRoot 'usage-reader')
try { npm ci --omit=dev; if ($LASTEXITCODE -ne 0) { throw 'Usage reader dependency installation failed' } }
finally { Pop-Location }
foreach ($notice in Get-ChildItem -LiteralPath $readerSource -File | Where-Object { $_.Name -match 'LICENSE|NOTICE' }) {
    Copy-Item -LiteralPath $notice.FullName -Destination (Join-Path $runtimeRoot 'usage-reader') -Force
}
$nodeSource = (Get-Command node.exe -ErrorAction Stop).Source
$nodeLicense = Join-Path (Split-Path -Parent $nodeSource) 'LICENSE'
if (-not (Test-Path -LiteralPath $nodeLicense)) { throw 'Node license notices are missing; runtime staging refused' }
Copy-Item -LiteralPath $nodeSource -Destination (Join-Path $runtimeRoot 'node/node.exe') -Force
Copy-Item -LiteralPath $nodeLicense -Destination (Join-Path $runtimeRoot 'node/LICENSE') -Force
Write-Output 'Staged Node, usage reader and third-party notices. No app started.'
