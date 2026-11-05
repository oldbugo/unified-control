# Current handoff: UC0.8.0 shared services

Checkpoint: **2026-11-05**, Australia/Sydney. Plan2.4 dock isolation and shared-services2.6 are complete and installed.
Leave the tray app running. Cross-device controls2.7 are next.

## Start here next time

1. Use `C:/Users/pc/Documents/UnifiedControl`, not the separate Ulanzi plugins repository.
2. Read the vault's `Start here.md`, perform its safe refresh check, then read UC `AGENTS.md`, `CLAUDE.md` and `README.md`.
3. Read [scope, completed work and ordered next tasks](project-status.md).
4. Check branch/status/remotes and running MCP `uc_get_status` / `uc_get_layout`; do not edit profiles directly.
5. Unless the user changes priority, the next coding task is **plan2.7: cross-device cooler controls**.
   Use named worker operations for theme selection, next/previous, Live and brightness; physical result/UI reflection need user acceptance.
   Installer replacement/release gates and Dial follow. Cooler workspace issue23 remains separate; measure before changing retention.

## Installed state

- UC0.8.0, clean build source`5dfec54bfa9ddcc136d80b7e719ea214af93151a`, pinned by`v0.8.0`.
- Cooler engine4.20.0/source`947aad3ca6e703194ae7b037da9ff2cfd6e40bba`, tagv4.20.0/CoolerScreen PR5;
  dock-driver pin`9ea93c06ad8ff3ae6f01784a4403886593ffff5d`.
- PR27 completes shared providers. The host alone writes profiles and owns one leased hardware collector and one account
  reader/rasterizer. A supervised device-free provider owns selected music/weather. Worker local duplicates stay off.
  Original source/transport deadlines, account/configuration binding, playback timing and bounded weather cache/rates remain.
  Idle provider can retain cache without active reads until UC quits. Later documentation has no binary/version change.
- Private issue23 tracks cooler workspace entry delay. The engine/Live already stays running; iframe hydration is repeated.
  The installed cause/timing is unmeasured. No timer or cooler-entry speedup is claimed.
- PR20 contains the completed cooler migration, retained through0.8.0.
- Actual app-owned comparison:286 library files,22 originals,6 themes,27 versions;22 byte-identical gallery files;23 active references.
  No missing files, changed original media, old-root dependencies, missing references, unreadable records or unstable files.
  One UC theme edit and two regenerated previews are preserved. Original standalone installation/data remain rollback.
- User confirms themes/settings look right and physical Live updates. Background Live away from the page/in the tray was already confirmed.
- Last installed checks: actual MCP0.8.0/engine4.20.0, new host with separate dock/cooler workers, one host-owned Node account
  helper and one provider-only child. Dock/cooler Live connected/no holds/errors, unchanged selected profile/page/all14keys.
  Both AI providers verified initially and331seconds later; reader/source ownership and process counts stay1, weather fetches stay1, hardware reads advance17to347 per family without failures. Connections/layout remain preserved. Bounded refresh evidence only; read fresh state on resume.
- Software evidence:193 core/11 host/62 reader plus cooler five receiver/seven music/four weather/nine worker-source cases,
  headless and quiet native shared/profile/MCP/hardware/worker/cooler/lifetime checks pass. Three UC ignores are existing;
  cooler renderer helper is invoked. One earlier opaque lifecycle poll failure passed unchanged-binary retry; cause unconfirmed.
  Normal pipe admission now tolerates brief contention for250ms before refusing; final affected native reruns pass.

## Safety and permissions that persist

The user permits tested UC updates without repeated install questions. Use the idle-safe updater, defer while busy,
restart quietly to the tray and verify afterward. This permission does not cover replacing unrelated apps.
Never force-close UC, CoolerScreen or Studio, and never launch the installed GUI from a packaged shell.
Use the dedicated request proxy/MCP; test instances must be debug, quiet, isolated and fake.
Sleep/resume remains explicitly deferred. No automatic resets/elevation, guessed USB commands or firmware operations.
Keep repositories private, real profiles/auth/media/diagnostics out of Git and opt-in integrations off unless the user enables them.

## Local cleanup and retained material

UC has one main checkout and no session worktree. Completed session branches are removed after merge.
`dist` retains0.8.0 plus previous0.7.0,0.6.0,0.5.1 and0.4.1 installers. No installer cleanup was requested for this milestone.
Seven known isolated cooler-migration fixtures remain under the Windows temporary folder: automatic approval review
blocked their recursive removal, including explicitly named paths. Do not claim they were deleted. Reusable dependency/build caches remain.
The user's installed apps, profiles, original cooler backup and unrelated project worktrees remain in place.
See [acceptance](acceptance.md) for checksums/evidence. The [0.3.0 handoff](handoff-0.3.0.md) is historical only.
