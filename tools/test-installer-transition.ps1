# Pure inventory tests and owned TEMP fixtures only. No registry writes or apps.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'installer-transition.ps1')
$script:checks = 0
function Assert-UcTest([bool]$Condition,[string]$Name) {
    if (-not $Condition) { throw ('test_failed: '+$Name) }
    $script:checks++
}
function Assert-UcThrows([scriptblock]$Action,[string]$Code) {
    $caught=$false
    try { & $Action | Out-Null } catch { $caught=$_.Exception.Message -eq $Code }
    Assert-UcTest $caught $Code
}

$present = [pscustomobject]@{version='0.9.1';expectedLocation=$true;binaryObserved=$true}
$uc = ConvertTo-UcInstallationSummary uc @($present)
$cooler = ConvertTo-UcInstallationSummary cooler @([pscustomobject]@{version='4.18.1';expectedLocation=$true;binaryObserved=$true})
Assert-UcTest ($uc.registration -eq 'observed' -and $uc.version -eq '0.9.1') 'known observation'
Assert-UcTest ((ConvertTo-UcInstallationSummary uc @()).registration -eq 'not_observed') 'absent observation'
Assert-UcTest ((ConvertTo-UcInstallationSummary uc @($present,$present)).registration -eq 'ambiguous_registration') 'duplicate registry views'
Assert-UcTest ((ConvertTo-UcInstallationSummary uc @([pscustomobject]@{version='0.9.1';expectedLocation=$false;binaryObserved=$true})).registration -eq 'unsupported_location') 'custom location unverified'
$secret = 'seed-secret C:\private\file --uninstall-command'
$untrusted = [pscustomobject]@{app='uc';registration=$secret;version=$secret;expectedLocation=$true;binaryObserved=$true;UninstallString=$secret}
$inventory = [pscustomobject]@{apps=@($uc,$cooler);readyToRetire=$true;backupVerified=$true;releaseAccepted=$true;historicalReceipt=@{verified=$true}}
$plan = Get-UcInstallerTransitionPlan $inventory
Assert-UcTest ($plan.state -eq 'blocked' -and -not $plan.readyToRetire) 'caller flags never authorize retirement'
Assert-UcTest (-not $plan.historicalReceiptAuthorizesRetirement) 'receipt remains historical'
Assert-UcTest ($plan.required -contains 'future_effect_implementation' -and $plan.preserve -contains 'uc_dock_profiles') 'remaining effects explicit'
$privatePlan = Get-UcInstallerTransitionPlan ([pscustomobject]@{apps=@($untrusted,$cooler);payload=$secret})
Assert-UcTest (-not ($privatePlan | ConvertTo-Json -Depth 8 -Compress).Contains('seed-secret')) 'untrusted inventory redacted'
Assert-UcTest ($privatePlan.evidence -eq 'shell_observation_unverified') 'shell namespace not app evidence'

$root=New-UcInstallerFixture
$beforeOriginal=Get-UcFixtureHashes $root (Join-Path $root 'data/CoolerScreen')
$beforeUc=Get-UcFixtureHashes $root (Join-Path $root 'data/UnifiedControl')
$result=Invoke-UcInstallerRehearsal -Root $root
Assert-UcTest ($result.state -eq 'passed' -and $result.retired -and $result.rolledBack) 'separate lineage upgrade and rollback'
Assert-UcTest ($result.originalDataPreserved -and $result.ucDataPreserved -and $result.ucIdentityPreserved) 'originals profiles later UC edits and identity preserved'
Assert-UcTest (-not $result.realInstallerTested) 'rehearsal never claims actual NSIS acceptance'
Assert-UcTest ((Get-UcFixtureHashes $root (Join-Path $root 'data/CoolerScreen')) -eq $beforeOriginal) 'legacy data exact before after hashes'
Assert-UcTest ((Get-UcFixtureHashes $root (Join-Path $root 'data/UnifiedControl')) -eq $beforeUc) 'UC data exact before after hashes'
foreach ($relative in @('data/CoolerScreen/Sources','backup/CoolerScreen/Sources','rollback/CoolerScreen/Sources')) {
    $empty = Assert-UcFixturePath $root (Join-Path $root $relative)
    Assert-UcTest ([IO.Directory]::Exists($empty) -and @(Get-ChildItem -LiteralPath $empty -Force).Count -eq 0) ('empty directory retained: '+$relative)
}
Assert-UcThrows { Invoke-UcInstallerRehearsal -Root $root } 'fixture_backup_already_exists'
$completeBackupManifest=Get-UcFixtureHashes $root (Join-Path $root 'backup/CoolerScreen')
$emptyBackup=Assert-UcFixturePath $root (Join-Path $root 'backup/CoolerScreen/Sources')
[IO.Directory]::Delete($emptyBackup,$false)
Assert-UcTest ((Get-UcFixtureHashes $root (Join-Path $root 'backup/CoolerScreen')) -ne $completeBackupManifest) 'missing empty directory invalidates backup manifest'

$failedRoot=New-UcInstallerFixture
$failed=Invoke-UcInstallerRehearsal -Root $failedRoot -FailBackup
Assert-UcTest ($failed.state -eq 'backup_verification_failed' -and -not $failed.retired) 'bad backup prevents retirement'
Assert-UcTest ($failed.originalDataPreserved -and $failed.ucDataPreserved -and $failed.legacyProgramPreserved) 'bad backup preserves original data profiles and program'
$registration=Get-Content -LiteralPath (Join-Path $failedRoot 'registration.json') -Raw | ConvertFrom-Json
Assert-UcTest ($registration.coolerRegistered -and $registration.ucRegistered) 'bad backup leaves registrations'

Assert-UcThrows { Assert-UcFixturePath $root (Join-Path $root '../escape') } 'fixture_path_escape'
Assert-UcThrows { Assert-UcDistinctFixturePaths $root @((Join-Path $root 'data'),(Join-Path $root 'data/UnifiedControl')) } 'fixture_paths_overlap'
Assert-UcThrows { Assert-UcDistinctFixturePaths $root @((Join-Path $root 'data'),(Join-Path $root 'data')) } 'fixture_paths_overlap'
$unowned=Join-Path ([IO.Path]::GetTempPath()) ('uc-installer-transition-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $unowned | Out-Null
Assert-UcThrows { Write-UcFixtureText $unowned (Join-Path $unowned 'must-not-exist') 'x' } 'fixture_owner_invalid'
Assert-UcTest (-not (Test-Path -LiteralPath (Join-Path $unowned 'must-not-exist'))) 'unowned fixture untouched'

$linkTarget=New-UcInstallerFixture
$linkRoot=New-UcInstallerFixture
$link=Join-Path $linkRoot 'linked'
New-Item -ItemType Junction -Path $link -Target $linkTarget | Out-Null
Assert-UcThrows { Write-UcFixtureText $linkRoot (Join-Path $link 'must-not-exist') 'x' } 'fixture_linked_path'
Assert-UcTest (-not (Test-Path -LiteralPath (Join-Path $linkTarget 'must-not-exist'))) 'linked target untouched'
Assert-UcThrows { Copy-UcFixtureTree $linkRoot $link (Join-Path $linkRoot 'backup/linked') } 'fixture_linked_path'
$linkedCandidate='uc-installer-transition-'+[guid]::NewGuid().ToString('N')
Assert-UcThrows { New-UcInstallerFixtureRoot (Join-Path $link $linkedCandidate) } 'fixture_linked_path'
Assert-UcTest (-not (Test-Path -LiteralPath (Join-Path $linkTarget $linkedCandidate))) 'linked parent rejected before directory or marker write'
Assert-UcThrows { New-UcInstallerFixtureRoot $root } 'fixture_already_exists'

[pscustomobject][ordered]@{state='passed';checks=$script:checks;realAppsTouched=$false;registryWrites=$false;fixturesRetained=$true} | ConvertTo-Json -Compress
