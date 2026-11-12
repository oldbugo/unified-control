# Installer transition: preflight and rehearsal
*Document type: reference. It describes how the part works. For the current state, read [status](../status.md).*

Phase 2.8 preparation, 2026-11-09. This is a read-only preflight and a synthetic rehearsal. It does not replace or uninstall either installed app and cannot authorize retirement.

## Existing identities

Unified Control currently uses product name `Unified Control`, binary `UnifiedControl.exe`, bundle identifier `com.oldbugo.unifiedcontrol`, its own installer folder, Apps & Features entry, shortcuts and startup value. CoolerScreen uses its own corresponding names. The generated Tauri NSIS installer derives its uninstall registry key from the product name, not the bundle identifier. Changing only the bundle identifier would not migrate the installer lineage and could disrupt app identity/data ownership.

Keep UC's identity, dock profiles and later cooler edits. The retained standalone installation/data remain fallback. The original phase 2.8 objective of one replacement app requires a separately reviewed transition of the old registration, shortcuts and startup choice. No arbitrary uninstall command or broad folder deletion is part of this preparation.

## Read-only preflight

Run from the repository:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/installer-transition.ps1
```

The tool reads only the exact current-user UC and CoolerScreen uninstall keys in 32/64-bit views and, for expected default install locations, checks the expected binary's existence. Multiple observations are labelled ambiguous; both registry views can refer to the same shared HKCU key, so this is not proof of duplicate installations. Custom locations remain unsupported/unverified. It never reads or executes `UninstallString`, writes the registry, changes startup/shortcuts, scans settings/media or opens a device. Output contains finite codes, booleans and validated version strings, not raw registry paths/commands/errors.

Shell observations can be stale in a packaged client's filesystem namespace. They are explicitly unverified, not an app-owned installation or data audit. A previous migration-verification receipt remains historical. Every result stays blocked and `readyToRetire:false`, even if caller-supplied fixture flags claim a backup or acceptance.

Before an actual transition, require a fresh app-owned migration comparison, a separately verified rollback snapshot, reviewed identity/shortcut handling, release acceptance and an implemented effect path. This tool does not claim those gates have passed. Current physical cooler recovery after the user's PC cycle does not by itself close all release gates; sleep/resume remains deferred.

## Synthetic rehearsal

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/installer-transition.ps1 -Mode Rehearsal
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-installer-transition.ps1
```

Only a newly claimed `uc-installer-transition-<random>` directory directly under the current temporary folder is used. Ancestor reparse checks run before directory/ownership-marker creation. An ownership marker, absolute path boundaries, overlap checks and repeated reparse-point checks guard later operations. Fixtures contain fake binaries, synthetic registrations, legacy settings/themes/media and UC dock profiles/later cooler edits. There are no actual registry entries, shortcuts, installers or app launches.

The rehearsal copies and verifies the synthetic rollback snapshot before modelling an upgrade. A failed comparison stops before retiring the synthetic legacy program/registration. On success, it parks that synthetic program, preserves the original data and UC identity/profiles, then models reinstalling the retained last 4.x program and reading a separate copy of the backup. Before/after manifests include explicit directory entries and SHA256 file hashes, so empty directories are preserved and a missing empty directory invalidates the comparison. The fixture includes an empty legacy Sources directory. This models the data/lineage contract; it is not a Windows NSIS install/uninstall test or a real 4.x rollback pass.

Fixtures are retained for review; the tool performs no recursive cleanup. Tests cover duplicate/custom inventory, redaction, caller flags, exact preserved bytes, rollback, failed backup, rerun refusal, escaped/overlapping paths, missing ownership and linked paths. No running user app, physical device or user profile is used.

Verification 2026-11-09: 33 Windows PowerShell checks pass, including empty-directory preservation and rejection of a linked fixture parent before any directory/marker write. Independent source review found no remaining actionable issue in this preparation scope. These are synthetic/pure checks; no real NSIS or 4.x rollback acceptance is claimed.
