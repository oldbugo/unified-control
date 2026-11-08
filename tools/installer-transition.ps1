# Read-only installer preflight and an isolated synthetic rehearsal. No retirement implementation.
[CmdletBinding()]
param([ValidateSet('Preflight','Rehearsal')][string]$Mode = 'Preflight')

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function ConvertTo-UcInstallationSummary {
    param([ValidateSet('uc','cooler')][string]$App, [object[]]$Candidates = @())
    $status = if ($Candidates.Count -eq 0) { 'not_observed' } elseif ($Candidates.Count -gt 1) { 'ambiguous_registration' } else { 'observed' }
    $version = $null
    $expectedLocation = $false
    $binaryObserved = $false
    if ($Candidates.Count -eq 1) {
        $candidate = $Candidates[0]
        $expectedLocation = $candidate.expectedLocation -eq $true
        $binaryObserved = $candidate.binaryObserved -eq $true
        if ($candidate.version -is [string] -and $candidate.version -match '^\d{1,5}\.\d{1,5}\.\d{1,5}$') { $version = $candidate.version }
        if (-not $expectedLocation) { $status = 'unsupported_location' }
    }
    [pscustomobject][ordered]@{ app=$App; registration=$status; version=$version; expectedLocation=$expectedLocation; binaryObserved=$binaryObserved }
}

function Get-UcInstallerInventory {
    # Both registry views are observations only. Never read or execute UninstallString.
    $apps = @()
    foreach ($spec in @(
        @{app='uc'; product='Unified Control'; folder='Unified Control'; exe='UnifiedControl.exe'},
        @{app='cooler'; product='CoolerScreen'; folder='CoolerScreen'; exe='CoolerScreen.exe'}
    )) {
        $candidates = @()
        foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)) {
            $base = $null
            $key = $null
            try {
                $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser,$view)
                $key = $base.OpenSubKey(('Software\Microsoft\Windows\CurrentVersion\Uninstall\'+$spec.product),$false)
                if ($null -ne $key) {
                    $location = $key.GetValue('InstallLocation',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                    $version = $key.GetValue('DisplayVersion',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                    $expected = Join-Path $env:LOCALAPPDATA $spec.folder
                    $locationMatches = $location -is [string] -and $location.Trim('"').TrimEnd('\') -eq $expected.TrimEnd('\')
                    $binaryObserved = $false
                    if ($locationMatches) {
                        $binary = Join-Path $expected $spec.exe
                        $item = Get-Item -LiteralPath $binary -Force -ErrorAction SilentlyContinue
                        $binaryObserved = $null -ne $item -and -not $item.PSIsContainer -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)
                    }
                    $candidates += [pscustomobject]@{version=$version; expectedLocation=$locationMatches; binaryObserved=$binaryObserved}
                }
            } catch {
                # No native error/path text escapes. A failed view is not proof of absence.
                $candidates += [pscustomobject]@{version=$null; expectedLocation=$false; binaryObserved=$false}
            } finally {
                if ($null -ne $key) { $key.Dispose() }
                if ($null -ne $base) { $base.Dispose() }
            }
        }
        $apps += ConvertTo-UcInstallationSummary -App $spec.app -Candidates $candidates
    }
    [pscustomobject][ordered]@{schema=1; evidence='shell_observation_unverified'; apps=$apps}
}

function Get-UcInstallerTransitionPlan {
    param([Parameter(Mandatory)]$Inventory)
    # Caller-supplied flags can never authorize retirement. This tool has no effect path.
    $observations = @()
    foreach ($app in @('uc','cooler')) {
        $matches = @($Inventory.apps | Where-Object { $_.app -eq $app })
        if ($matches.Count -ne 1) {
            $observations += ConvertTo-UcInstallationSummary -App $app -Candidates @(@{version=$null;expectedLocation=$false;binaryObserved=$false})
        } else {
            $entry = $matches[0]
            $allowed = @('not_observed','ambiguous_registration','observed','unsupported_location')
            $registration = if ($entry.registration -in $allowed) { $entry.registration } else { 'unsupported_location' }
            $version = if ($entry.version -is [string] -and $entry.version -match '^\d{1,5}\.\d{1,5}\.\d{1,5}$') { $entry.version } else { $null }
            $observations += [pscustomobject][ordered]@{app=$app;registration=$registration;version=$version;expectedLocation=($entry.expectedLocation -eq $true);binaryObserved=($entry.binaryObserved -eq $true)}
        }
    }
    [pscustomobject][ordered]@{
        schema=1
        mode='preflight_only'
        state='blocked'
        readyToRetire=$false
        evidence='shell_observation_unverified'
        identities=@{uc='com.oldbugo.unifiedcontrol';cooler='com.oldbugo.coolerscreen';relationship='separate_installer_lineages'}
        observations=$observations
        required=@('fresh_app_owned_migration_check','verified_rollback_snapshot','reviewed_identity_and_shortcut_transition','release_acceptance','future_effect_implementation')
        historicalReceiptAuthorizesRetirement=$false
        preserve=@('uc_identity','uc_dock_profiles','uc_cooler_edits','original_cooler_data','last_4x_rollback')
    }
}

function Assert-UcUnlinkedFixturePath {
    param([Parameter(Mandatory)][string]$Path)
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        $item = Get-Item -LiteralPath $cursor -Force -ErrorAction SilentlyContinue
        if ($null -ne $item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'fixture_linked_path' }
        $next = [IO.Path]::GetDirectoryName($cursor)
        if ($next -eq $cursor) { break }
        $cursor = $next
    }
}

function New-UcInstallerFixtureRoot {
    param([Parameter(Mandatory)][string]$Root)
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\','/')
    # Reject linked TEMP ancestors before creating even a directory or marker.
    Assert-UcUnlinkedFixturePath $rootFull
    $tempFull = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\','/')
    if ([IO.Path]::GetDirectoryName($rootFull) -ne $tempFull -or [IO.Path]::GetFileName($rootFull) -notmatch '^uc-installer-transition-[a-f0-9]{32}$') { throw 'fixture_root_invalid' }
    if (Test-Path -LiteralPath $rootFull) { throw 'fixture_already_exists' }
    New-Item -ItemType Directory -Path $rootFull -ErrorAction Stop | Out-Null
    Assert-UcUnlinkedFixturePath $rootFull
    [IO.File]::WriteAllText((Join-Path $rootFull '.uc-installer-transition-owner'),[IO.Path]::GetFileName($rootFull))
    return $rootFull
}

function Assert-UcFixturePath {
    param([Parameter(Mandatory)][string]$Root,[Parameter(Mandatory)][string]$Path)
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\','/')
    $tempFull = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\','/')
    if ([IO.Path]::GetDirectoryName($rootFull) -ne $tempFull -or [IO.Path]::GetFileName($rootFull) -notmatch '^uc-installer-transition-[a-f0-9]{32}$') { throw 'fixture_root_invalid' }
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\','/')
    if ($full -ne $rootFull -and -not $full.StartsWith($rootFull+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'fixture_path_escape' }
    Assert-UcUnlinkedFixturePath $full
    $marker = Join-Path $rootFull '.uc-installer-transition-owner'
    if (-not [IO.File]::Exists($marker) -or [IO.File]::ReadAllText($marker) -ne [IO.Path]::GetFileName($rootFull)) { throw 'fixture_owner_invalid' }
    return $full
}

function Assert-UcDistinctFixturePaths {
    param([string]$Root,[string[]]$Paths)
    $checked = @($Paths | ForEach-Object { Assert-UcFixturePath -Root $Root -Path $_ })
    for ($i=0;$i -lt $checked.Count;$i++) {
        for ($j=$i+1;$j -lt $checked.Count;$j++) {
            $a=$checked[$i]; $b=$checked[$j]
            if ($a -eq $b -or $a.StartsWith($b+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $b.StartsWith($a+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'fixture_paths_overlap' }
        }
    }
}

function Write-UcFixtureText {
    param([string]$Root,[string]$Path,[string]$Text)
    $safe = Assert-UcFixturePath -Root $Root -Path $Path
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($safe)) | Out-Null
    [IO.File]::WriteAllText($safe,$Text,[Text.UTF8Encoding]::new($false))
}

function Get-UcFixtureHashes {
    param([string]$Root,[string]$Path)
    $safe = Assert-UcFixturePath -Root $Root -Path $Path
    $result = [ordered]@{}
    $result['.'] = 'directory'
    function Visit-UcFixtureDirectory([string]$Directory) {
        Assert-UcFixturePath -Root $Root -Path $Directory | Out-Null
        foreach ($item in @(Get-ChildItem -LiteralPath $Directory -Force | Sort-Object Name)) {
            Assert-UcFixturePath -Root $Root -Path $item.FullName | Out-Null
            $relative=$item.FullName.Substring($safe.Length).TrimStart('\','/')
            if ($item.PSIsContainer) {
                $result[$relative] = 'directory'
                Visit-UcFixtureDirectory $item.FullName
            } else {
                $result[$relative]='file:'+(Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash
            }
        }
    }
    Visit-UcFixtureDirectory $safe
    return ($result | ConvertTo-Json -Compress)
}

function New-UcInstallerFixture {
    $root = Join-Path ([IO.Path]::GetTempPath()) ('uc-installer-transition-'+[guid]::NewGuid().ToString('N'))
    $root = New-UcInstallerFixtureRoot $root
    Write-UcFixtureText $root (Join-Path $root 'installs/Unified Control/UnifiedControl.exe') 'synthetic UC old build'
    Write-UcFixtureText $root (Join-Path $root 'installs/CoolerScreen/CoolerScreen.exe') 'synthetic last 4.x build'
    Write-UcFixtureText $root (Join-Path $root 'data/UnifiedControl/profiles/everyday.json') '{"synthetic":"dock-profile"}'
    Write-UcFixtureText $root (Join-Path $root 'data/UnifiedControl/devices/cooler/theme.json') '{"synthetic":"later-UC-edit"}'
    Write-UcFixtureText $root (Join-Path $root 'data/CoolerScreen/settings.json') '{"synthetic":"legacy-settings"}'
    Write-UcFixtureText $root (Join-Path $root 'data/CoolerScreen/themes/theme.json') '{"synthetic":"legacy-theme"}'
    Write-UcFixtureText $root (Join-Path $root 'data/CoolerScreen/library/clip.bin') 'synthetic-media-bytes'
    [IO.Directory]::CreateDirectory((Assert-UcFixturePath $root (Join-Path $root 'data/CoolerScreen/Sources'))) | Out-Null
    Write-UcFixtureText $root (Join-Path $root 'registration.json') '{"ucRegistered":true,"coolerRegistered":true,"ucIdentity":"com.oldbugo.unifiedcontrol"}'
    return $root
}

function Copy-UcFixtureTree {
    param([string]$Root,[string]$Source,[string]$Destination)
    Assert-UcDistinctFixturePaths $Root @($Source,$Destination)
    $sourceFull=Assert-UcFixturePath $Root $Source
    $destinationFull=Assert-UcFixturePath $Root $Destination
    if (Test-Path -LiteralPath $destinationFull) { throw 'fixture_backup_already_exists' }
    Get-UcFixtureHashes $Root $sourceFull | Out-Null
    [IO.Directory]::CreateDirectory($destinationFull) | Out-Null
    foreach ($entry in @(Get-ChildItem -LiteralPath $sourceFull -Recurse -Force)) {
        $from=Assert-UcFixturePath $Root $entry.FullName
        $relative=$from.Substring($sourceFull.Length).TrimStart('\','/')
        $to=Assert-UcFixturePath $Root (Join-Path $destinationFull $relative)
        if ($entry.PSIsContainer) {
            [IO.Directory]::CreateDirectory($to) | Out-Null
        } else {
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($to)) | Out-Null
            [IO.File]::Copy($from,$to,$false)
        }
    }
}

function Invoke-UcInstallerRehearsal {
    param([string]$Root=(New-UcInstallerFixture),[switch]$FailBackup)
    $legacyData=Join-Path $Root 'data/CoolerScreen'; $ucData=Join-Path $Root 'data/UnifiedControl'
    $legacyInstall=Join-Path $Root 'installs/CoolerScreen'; $ucInstall=Join-Path $Root 'installs/Unified Control'
    $backup=Join-Path $Root 'backup/CoolerScreen'; $parked=Join-Path $Root 'retained-program/CoolerScreen'
    Assert-UcDistinctFixturePaths $Root @($legacyData,$ucData,$legacyInstall,$ucInstall,$backup,$parked)
    $oldHash=Get-UcFixtureHashes $Root $legacyData; $ucHash=Get-UcFixtureHashes $Root $ucData
    $programHash=Get-UcFixtureHashes $Root $legacyInstall
    Copy-UcFixtureTree $Root $legacyData $backup
    if ($FailBackup) { Write-UcFixtureText $Root (Join-Path $backup 'settings.json') 'synthetic verification failure' }
    if ((Get-UcFixtureHashes $Root $backup) -ne $oldHash) {
        return [pscustomobject][ordered]@{schema=1;mode='synthetic_rehearsal';state='backup_verification_failed';retired=$false;rolledBack=$false;originalDataPreserved=((Get-UcFixtureHashes $Root $legacyData) -eq $oldHash);ucDataPreserved=((Get-UcFixtureHashes $Root $ucData) -eq $ucHash);legacyProgramPreserved=((Get-UcFixtureHashes $Root $legacyInstall) -eq $programHash)}
    }
    Write-UcFixtureText $Root (Join-Path $ucInstall 'UnifiedControl.exe') 'synthetic UC replacement build'
    # Only this owned fixture is changed. No registry or shortcut API is called.
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($parked)) | Out-Null
    [IO.Directory]::Move((Assert-UcFixturePath $Root $legacyInstall),(Assert-UcFixturePath $Root $parked))
    Write-UcFixtureText $Root (Join-Path $Root 'registration.json') '{"ucRegistered":true,"coolerRegistered":false,"ucIdentity":"com.oldbugo.unifiedcontrol"}'
    $retiredSafely = (Get-UcFixtureHashes $Root $legacyData) -eq $oldHash -and (Get-UcFixtureHashes $Root $ucData) -eq $ucHash -and (Get-UcFixtureHashes $Root $backup) -eq $oldHash
    # Model reinstalling the retained last4.x program and reading a separate backup copy.
    [IO.Directory]::Move((Assert-UcFixturePath $Root $parked),(Assert-UcFixturePath $Root $legacyInstall))
    $rollbackData=Join-Path $Root 'rollback/CoolerScreen'
    Copy-UcFixtureTree $Root $backup $rollbackData
    Write-UcFixtureText $Root (Join-Path $Root 'registration.json') '{"ucRegistered":true,"coolerRegistered":true,"ucIdentity":"com.oldbugo.unifiedcontrol"}'
    [pscustomobject][ordered]@{schema=1;mode='synthetic_rehearsal';state='passed';retired=$retiredSafely;rolledBack=((Get-UcFixtureHashes $Root $rollbackData) -eq $oldHash -and (Get-UcFixtureHashes $Root $legacyInstall) -eq $programHash);originalDataPreserved=((Get-UcFixtureHashes $Root $legacyData) -eq $oldHash);ucDataPreserved=((Get-UcFixtureHashes $Root $ucData) -eq $ucHash);ucIdentityPreserved=([IO.File]::ReadAllText((Join-Path $Root 'registration.json')).Contains('com.oldbugo.unifiedcontrol'));realInstallerTested=$false}
}

if ($MyInvocation.InvocationName -ne '.') {
    if ($Mode -eq 'Rehearsal') { Invoke-UcInstallerRehearsal | ConvertTo-Json -Depth 8 }
    else { Get-UcInstallerTransitionPlan (Get-UcInstallerInventory) | ConvertTo-Json -Depth 8 }
}
