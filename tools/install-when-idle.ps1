# Installs a Unified Control build quietly, but only when the running app is idle. Copied from CoolerScreen's
# tools/install-when-idle.ps1 (user rules 2026-10-19); every UC build knows --quit and --update, so the old-build branches are gone.
# Only the orchestrator runs this. Usage:
#   powershell -ExecutionPolicy Bypass -File tools\install-when-idle.ps1 -Setup <UnifiedControl_x.y.z_x64-setup.exe>
#
# Running build: `UnifiedControl.exe --update <setup>` makes the APP quit cleanly, run the installer /S and start the new build
# with --background, all by itself (the helper is spawned by the real app, so everything runs outside a Claude shell's private
# MSIX copy of %APPDATA%). Exit codes of --update: 0 accepted and the app has exited, 2 busy (a job is running: nothing done, try
# again later), 3 no running instance, 4 not a Unified Control installer.
# Not running: install /S; the app is NOT started (a process started from a Claude shell would save into the private copy): the
# user opens it from the Start menu.
# This script never touches CoolerScreen (its own exe, folder and installer names).
# Exit codes of this script: 0 installed (see the message), 2 app busy or did not quit (nothing done), 1 failed.
param([Parameter(Mandatory)][string]$Setup, [string]$RequestExe)

$exe = Join-Path $env:LOCALAPPDATA "Unified Control\UnifiedControl.exe"
$requestPath = $exe
if ($RequestExe) {
    # An explicitly supplied reviewed UC build can send the same request-only
    # command when a packaged shell's installed-file view still contains an old
    # shadow copy. --update exits without launching UI if no host is present.
    if (-not [IO.Path]::IsPathRooted($RequestExe) -or -not (Test-Path -LiteralPath $RequestExe) -or [IO.Path]::GetFileName($RequestExe) -ine 'UnifiedControl.exe') { 'RequestExe must be an absolute existing UnifiedControl.exe'; exit 1 }
    $requestPath = $RequestExe
}
$name = [IO.Path]::GetFileName($Setup)
if ($name -notmatch '^UnifiedControl_\d+\.\d+\.\d+_x64-setup\.exe$') { "that file is not a Unified Control installer: $Setup"; exit 1 }
$want = [version]($name -replace '^UnifiedControl_(\d+\.\d+\.\d+)_.*$', '$1')
# Ask the app's own scoped quit/update endpoint whether it is running. Packaged
# shells can expose a virtualized process path unlike $exe; a string comparison
# must never bypass the app's busy/flush guard and fall through to NSIS.
if (Test-Path -LiteralPath $requestPath) {
    $updateStarted = Get-Date
    $p = Start-Process $requestPath -ArgumentList '--update', "`"$Setup`"" -WindowStyle Hidden -Wait -PassThru
    switch ($p.ExitCode) {
        0 {
            # the app quit; its helper now runs the installer and starts the new build. Wait for the new version to show.
            for ($i = 0; $i -lt 240; $i++) {
                Start-Sleep -Milliseconds 500
                $freshProcesses = @(Get-Process UnifiedControl -ErrorAction SilentlyContinue | Where-Object { $_.StartTime -ge $updateStarted })
                # Verify what actually started, not shadowed file metadata.
                $startupLines = @(Get-Content -LiteralPath (Join-Path $env:APPDATA 'UnifiedControl/device.log') -Tail 80 -ErrorAction SilentlyContinue)
                foreach ($freshProcess in $freshProcesses) {
                    $startupPattern = ' Unified Control (\d+\.\d+\.\d+) build \S+ started, pid ' + $freshProcess.Id + ', exe '
                    foreach ($startupLine in $startupLines) {
                        if ($startupLine -match $startupPattern -and [version]$Matches[1] -ge $want) { "updated to $($Matches[1]); startup version and new PID verified; Unified Control restarted itself quietly"; exit 0 }
                    }
                }
            }
            "the app quit and the installer was started, but the new build is not running yet: check again in a minute"; exit 1
        }
        2 { "the app is busy (a job is running): not installing, try again when idle"; exit 2 }
        3 {
            # A different packaged/default app-data scope can make the named
            # endpoint unreachable. Never let that bypass a live app's guard.
            if (Get-Process UnifiedControl -ErrorAction SilentlyContinue) {
                "a running Unified Control instance could not be reached by its update guard: quit it from its tray before installing"; exit 2
            }
        }
        4 { "that file is not a Unified Control installer: $Setup"; exit 1 }
        default { "the app did not quit (--update exit code $($p.ExitCode)): not installing"; exit 2 }
    }
}

$i = Start-Process $Setup -ArgumentList "/S" -WindowStyle Hidden -PassThru -Wait
if ($i.ExitCode -ne 0) { "installer failed: $($i.ExitCode)"; exit 1 }
# The installer can return before every file is written (CoolerScreen, 2026-10-19): wait for the new exe.
for ($k = 0; $k -lt 120; $k++) {
    if (Test-Path $exe) {
        $now = [version]((Get-Item $exe).VersionInfo.ProductVersion -replace '[^0-9.].*$', '')
        if ($now -ge $want) { "installed $now; Unified Control is closed: ask the user to open it from the Start menu"; exit 0 }
    }
    Start-Sleep -Milliseconds 500
}
"the installer finished but UnifiedControl.exe is still $now (wanted $want): check before telling the user"; exit 1
