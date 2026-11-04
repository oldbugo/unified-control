# Current handoff: UC0.6.0 dock worker

Checkpoint: **2026-11-04**, Australia/Sydney. The user resumed development and selected the existing priority order.
Plan2.4 dock-worker separation is complete and installed. Leave the tray app running; shared services are the next task.

## Start here next time

1. Use `C:/Users/pc/Documents/UnifiedControl`, not the separate Ulanzi plugins repository.
2. Read the vault's `Start here.md`, perform its safe refresh check, then read UC `AGENTS.md`, `CLAUDE.md` and `README.md`.
3. Read [scope, completed work and ordered next tasks](project-status.md).
4. Check branch/status/remotes and running MCP `uc_get_status` / `uc_get_layout`; do not edit profiles directly.
5. Unless the user changes priority, the next coding task is **plan2.6: shared services**.
   Cross-device cooler actions, installer replacement/release gates and Dial follow in that order.
   The user-reported cooler workspace delay is a separate open issue23; measure entry stages before changing retention.

## Installed state

- UC0.6.0, clean build source`0117a5f712ffafed312f9fcfafe41309bfd182d9`, pinned by`v0.6.0`.
- Cooler engine4.18.2/source`1644a4ee00f46a78160e1ed7f902507d11192648`; dock-driver pin`9ea93c06ad8ff3ae6f01784a4403886593ffff5d`.
- PR22 contains dock-worker extraction. The host alone writes profiles; the child owns driver/rendering/runtime/actions
  and the existing reader. Shared providers are still pending. Later checkpoint documentation has no binary/version change.
- Private issue23 tracks cooler workspace entry delay. The engine/Live already stays running; iframe hydration is repeated.
  The installed cause/timing is unmeasured. No timer or cooler-entry speedup is claimed.
- PR20 contains the completed cooler migration, retained in0.6.0.
- Actual app-owned comparison:286 library files,22 originals,6 themes,27 versions;22 byte-identical gallery files;23 active references.
  No missing files, changed original media, old-root dependencies, missing references, unreadable records or unstable files.
  One UC theme edit and two regenerated previews are preserved. Original standalone installation/data remain rollback.
- User confirms themes/settings look right and physical Live updates. Background Live away from the page/in the tray was already confirmed.
- Last installed checks: dock connected, separate dock child, cooler Live connected/no holds/errors; selected dock
  profile/page/all14keys preserved. Both providers initially verified. Read fresh state on resume.
- Software evidence:165 core/11 host/62 reader tests plus relevant headless/native checks pass, with three existing
  ignored checks. Native tests cover both-way crashes, EOF action drain, current-profile recovery/no replay, busy refusal,
  Folder/Back follow and queued-edit/import tray Quit. Whole-repo formatting has existing unrelated differences.

## Safety and permissions that persist

The user permits tested UC updates without repeated install questions. Use the idle-safe updater, defer while busy,
restart quietly to the tray and verify afterward. This permission does not cover replacing unrelated apps.
Never force-close UC, CoolerScreen or Studio, and never launch the installed GUI from a packaged shell.
Use the dedicated request proxy/MCP; test instances must be debug, quiet, isolated and fake.
Sleep/resume remains explicitly deferred. No automatic resets/elevation, guessed USB commands or firmware operations.
Keep repositories private, real profiles/auth/media/diagnostics out of Git and opt-in integrations off unless the user enables them.

## Local cleanup and retained material

UC has one main checkout and no session worktree. Completed session branches are removed after merge.
`dist` retains0.6.0 plus previous0.5.1 and0.4.1 installers. No further artifact cleanup was requested for this milestone.
Seven known isolated cooler-migration fixtures remain under the Windows temporary folder: automatic approval review
blocked their recursive removal, including explicitly named paths. Do not claim they were deleted. Reusable dependency/build caches remain.
The user's installed apps, profiles, original cooler backup and unrelated project worktrees remain in place.
See [acceptance](acceptance.md) for checksums/evidence. The [0.3.0 handoff](handoff-0.3.0.md) is historical only.
