# Current handoff: UC0.9.0 cross-device controls

Checkpoint: **2026-11-06**, Australia/Sydney. Plan2.4/shared-services2.6 and software2.7 are complete and installed.
Leave the tray app running. Cooler physical acceptance remains blocked by an unresolved USB write timeout.

## Start here next time

1. Use `C:/Users/pc/Documents/UnifiedControl`, not the separate Ulanzi plugins repository.
2. Refresh/read the vault Start here, then UC AGENTS.md, CLAUDE.md and README.md in full.
3. Read [project status](project-status.md), inspect Git status and running MCP status/layout. Preserve user changes.
4. Next recorded implementation: **2.8 installer replacement**, followed by combined release gates and Dial. Do not uninstall the standalone CoolerScreen or remove rollback data without a concrete reviewed migration plan.
5. Cooler issue23 (workspace entry delay) stays separate. No retention timer or measured speedup was included.

## Installed state and open connection failure

Installed0.9.0/source7c3ee6d, engine4.21.0/sourceda2cb2c, passed quiet startup/new-process, actual MCP version, connected dock, both verified AI providers and unchanged selected fourteen-key layout checks. Cooler Live remains disconnected with write_timeout. A quiet comparison install of0.8.0/engine4.20.0 reproduced the same timeout;0.9.0 was restored. The initiating cause is unknown. Windows reports two normal USB nodes/problem0; no resets, elevation, firmware or actual cooler actions were performed. Physical switching/brightness acceptance remains open.

- UC0.9.0 source7c3ee6daaac24f90e85d3ae7359fe392fae5d5cb/tagv0.9.0, private PR29/mainc88530e.
- Cooler engine4.21.0 sourceda2cb2c61965058604430a5153c0eb0975c6cbaa/tagv4.21.0, PR6/mainf7642fd4.
- Dock driver remains pinned9ea93c06ad8ff3ae6f01784a4403886593ffff5d.
- One host account reader and one provider-only child verified. Source worker has no failures; the cooler worker is ready with no restarts, while physical Live remains disconnected.
- All existing key assignments are preserved; no new cooler actions were assigned. All three saved pages were checked for rollback compatibility before the0.8.0 comparison. No actual cooler actions were used for testing.
- Original standalone installation/data remain untouched. Prior user confirmation of themes/settings/background Live is historical, not evidence of recovery from this failure.

## Software verification

202 UC core/11 host/62 reader tests pass (three existing ignored checks); cooler five action/19 scheduler/nine worker cases pass.
Full headless editor/cooler and quiet native cross-device/profile/MCP/dock-worker/cooler/lifetime/migration/shared-provider/hardware checks pass.
NativeW3tNZb proves fake button routing, deduplication, iframe reflection, Embedded preservation, busy after requester loss, no replay, correlated completion during Shutdown and EOF cancellation.
Touched worker/action formatting and whitespace pass; broad formatting retains unrelated prior differences. Release packaging and inactive bundled reader smoke pass.
[Contract](device-app/cross-device-controls.md) and [acceptance/checksums](acceptance.md).

## Persistent permissions and boundaries

Tested UC idle-safe updates are authorized; defer while busy, never force-close, restart quietly to the tray and verify afterward.
Use MCP/dedicated request proxy, never start installed GUI from a packaged shell. Tests are isolated quiet debug/fake instances.
No automatic resets/elevation, guessed USB commands or firmware. Sleep/resume stays deferred. Do not change unrelated apps, auth/media/profiles or device opt-ins.
Private repositories only. Completed branches/worktrees are removed after merge; documentation does not trigger a new version or reinstall.
`dist` retains0.9.0 plus0.8.0/0.7.0/0.6.0/0.5.1/0.4.1. No installer cleanup requested.
Seven old migration fixtures remain because earlier automatic approval blocked recursive deletion; reusable caches and unrelated worktrees remain.
