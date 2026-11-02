# Stage the reviewed MCP adapter into the release's isolated runtime copy.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$agentSource = Join-Path $projectRoot 'companions/agent-mcp'
$agentRuntime = [IO.Path]::GetFullPath((Join-Path $projectRoot 'src-tauri/runtime/agent-mcp'))
$agentParent = [IO.Path]::GetFullPath((Join-Path $projectRoot 'src-tauri/runtime')) + [IO.Path]::DirectorySeparatorChar
if (-not $agentRuntime.StartsWith($agentParent, [StringComparison]::OrdinalIgnoreCase)) { throw 'Agent runtime output is outside the workspace runtime folder' }
New-Item -ItemType Directory -Force -Path $agentRuntime | Out-Null
foreach ($name in @('server.mjs', 'package.json', 'package-lock.json', 'licenses')) {
    Copy-Item -LiteralPath (Join-Path $agentSource $name) -Destination $agentRuntime -Recurse -Force
}
Push-Location $agentRuntime
try {
    npm ci --omit=dev --ignore-scripts
    if ($LASTEXITCODE -ne 0) { throw 'MCP runtime dependency installation failed' }
} finally { Pop-Location }
Write-Output 'Staged the pinned local MCP adapter and its dependency notices. No app started.'
