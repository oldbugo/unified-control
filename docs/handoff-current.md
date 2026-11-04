# Current handoff: UC0.7.0 shared hardware

Checkpoint: **2026-11-05**, Australia/Sydney. Plan2.4 dock isolation and shared-hardware slice2.6a are complete and installed.
Leave the tray app running. Remaining shared AI/music/weather ownership is next, followed by cross-device controls.

## Start here next time

1. Use `C:/Users/pc/Documents/UnifiedControl`, not the separate Ulanzi plugins repository.
2. Read the vault's `Start here.md`, perform its safe refresh check, then read UC `AGENTS.md`, `CLAUDE.md` and `README.md`.
3. Read [scope, completed work and ordered next tasks](project-status.md).
4. Check branch/status/remotes and running MCP `uc_get_status` / `uc_get_layout`; do not edit profiles directly.
5. Unless the user changes priority, the next coding task is **plan2.6b: remaining shared providers**.
   AI usage remains dock-owned; now-playing/weather remain cooler-owned, with existing activation/cache/rate rules.
   Cross-device cooler actions, installer replacement/release gates and Dial follow in that order.
   The user-reported cooler workspace delay is a separate open issue23; measure entry stages before changing retention.

## Installed state

- UC0.7.0, clean build source`b5b014034baf87597f5dac7202b2393e2249af92`, pinned by`v0.7.0`.
- Cooler engine4.19.0/source`88aad7da5bbc49a4571b3376294adaf85ca4b780`, tagv4.19.0/CoolerScreen PR4;
  dock-driver pin`9ea93c06ad8ff3ae6f01784a4403886593ffff5d`.
- PR25 adds shared hardware to prior dock-worker extraction. The host alone writes profiles and owns one leased
  CPU/RAM/NVIDIA collector. Both workers consume validated snapshots; replaced local aggregate readers stay off.
  AI stays dock-owned. Later checkpoint documentation has no binary/version change.
- Private issue23 tracks cooler workspace entry delay. The engine/Live already stays running; iframe hydration is repeated.
  The installed cause/timing is unmeasured. No timer or cooler-entry speedup is claimed.
- PR20 contains the completed cooler migration, retained in0.6.0.
- Actual app-owned comparison:286 library files,22 originals,6 themes,27 versions;22 byte-identical gallery files;23 active references.
  No missing files, changed original media, old-root dependencies, missing references, unreadable records or unstable files.
  One UC theme edit and two regenerated previews are preserved. Original standalone installation/data remain rollback.
- User confirms themes/settings look right and physical Live updates. Background Live away from the page/in the tray was already confirmed.
- Last installed checks: actual MCP0.7.0/engine4.19.0, dock connected, separate dock child, cooler Live connected/no holds/
  errors, unchanged selected profile/page/all14keys. Real cooler demand drives a1Hz collector with equal CPU/RAM/GPU reads
  and zero source failures. Both AI providers verified initially and339seconds later; collector counts advanced14to352 each without failure in the same generation. This is bounded refresh evidence. Read fresh state on resume.
- Software evidence:179 core/11 host/62 reader tests plus seven focused cooler/four worker cases and headless/native
  shared/profile/MCP/worker/cooler/lifetime checks pass. Three UC ignored checks are existing; cooler renderer helper is
  invoked by its parent test. Native cases prove one collector, no local duplicates, hidden-page release, complete GPU
  fields, failure/expiry/recovery, peer isolation, both-way crashes and safe Quit. Broad fmt has unrelated existing differences.

## Safety and permissions that persist

The user permits tested UC updates without repeated install questions. Use the idle-safe updater, defer while busy,
restart quietly to the tray and verify afterward. This permission does not cover replacing unrelated apps.
Never force-close UC, CoolerScreen or Studio, and never launch the installed GUI from a packaged shell.
Use the dedicated request proxy/MCP; test instances must be debug, quiet, isolated and fake.
Sleep/resume remains explicitly deferred. No automatic resets/elevation, guessed USB commands or firmware operations.
Keep repositories private, real profiles/auth/media/diagnostics out of Git and opt-in integrations off unless the user enables them.

## Local cleanup and retained material

UC has one main checkout and no session worktree. Completed session branches are removed after merge.
`dist` retains0.7.0 plus previous0.6.0,0.5.1 and0.4.1 installers. No installer cleanup was requested for this milestone.
Seven known isolated cooler-migration fixtures remain under the Windows temporary folder: automatic approval review
blocked their recursive removal, including explicitly named paths. Do not claim they were deleted. Reusable dependency/build caches remain.
The user's installed apps, profiles, original cooler backup and unrelated project worktrees remain in place.
See [acceptance](acceptance.md) for checksums/evidence. The [0.3.0 handoff](handoff-0.3.0.md) is historical only.
