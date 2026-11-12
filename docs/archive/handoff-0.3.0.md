# UC 0.3.0 stand-down and next-session handoff

**Historical only.** Use [the current handoff](../handoff-current.md) and [project status](../project-status.md) for resuming.

Checkpoint: 2026-11-02 (Australia/Sydney). Feature development is stood down at 0.3.0. The installed app stays running.

**Superseded checkpoint:** Work resumed later on 2026-11-02. Actual Codex model-client MCP read/preview/apply/undo checks
passed with the temporary inactive-profile edit restored. The user approved installation of 0.3.1 / source e23be94;
startup/version/new PID, proxy hash and dock reconnection were verified, and both usage providers reached verified status.
Use current README/acceptance/function-failures/backlog and live MCP state. The 0.3.0 details below remain historical evidence.

## Start here

Open a new local Codex session in `C:/Users/pc/Documents/UnifiedControl`, rather than the separate Ulanzi plugins folder.
Read the root `AGENTS.md`, then call the registered `unified-control` MCP tools `uc_get_status` and `uc_get_layout`.
If the current client has not loaded the new connection, reload MCP or restart the client. Client-model tool discovery is
the remaining first-session check; the actual installed production MCP server has already been verified independently.

## Installed and verified

- App: **0.3.0**, build source `0909429f6edf49b09716a1b26461a218a01e4be6`, pinned by `v0.3.0`.
- Main executable and dedicated `UC-AgentRequest-v1.exe` have the same SHA-256:
  `2A549604A0BDD986C705248741F56508CC409D39027AEF88BE4583F80B852BCF`.
- Installer: local `dist/UnifiedControl_0.3.0_x64-setup.exe`, SHA-256
  `278B4392AF581077D2EFCE4F9404730F71F39B9F247F9DB818E0C6F42928297F`.
- User enabled **General > Local agent access**. Read-only production MCP verification reports seven tools, app 0.3.0,
  access enabled, dock Connected and a readable 14-key layout. Before/after revisions matched; the check changed nothing.
- Connection registered for Codex, Claude Code/Desktop Code tab and Claude Desktop chat. Existing settings are preserved.
- Monitor Toggle: user-confirmed working on Key 10 with one Dell target. Both faces are labelled Monitor.

## Source and repository boundaries

The completed UC development stack is consolidated onto private `oldbugo/unified-control` main. Release tags keep the
original build commits; later handoff/documentation commits do not change the installed binary. Completed branches and
owned helper worktrees are retired after preserving their history. Do not delete unrelated CoolerScreen/Claude worktrees.

The exact dependency pins remain unchanged:

| Repository | Installed UC pin | Responsibility |
| --- | --- | --- |
| `oldbugo/uc-ulanzi` | `9ea93c06ad8ff3ae6f01784a4403886593ffff5d` | Dock transport/driver |
| `oldbugo/coolerscreen` | `88cd041c4a4a19d66bd332b913ba34d745b075a2` | Supervised cooler worker |

Related completed driver/worker changes are consolidated onto their private mains as part of this stand-down. This is source
housekeeping, not a new CoolerScreen install or a hardware recovery claim. The separate Monitor Toggle/Toolkit repo is untouched.

## MCP behavior and evidence

Tools: status, scoped layout read, monitor discovery, preview, revision-checked apply, last-agent-write undo and job status.
Changes pass through the running UC host. Editor leases wait for imports/saves, reject active text/failed saves and reload
accepted state before unlocking. No physical action, firmware, USB reset or general shell tool is exposed.

Latest checks: 118 core and 11 binary tests; official-SDK modern/legacy protocol fixtures; actual Windows pipe ACL/identity,
frame/deadline/shutdown checks; 13 pure editor cases plus three headless cases; quiet native MCP/UI/storage and profile suites.
Native timing tests now verify actual blocked writer/File reads and elapsed time. The earlier nonwritable `T.invoke`
replacement did not establish those delayed branches; use the corrected tests. No automated physical/account effects.

See [MCP contract](../agent-control.md), [acceptance](../acceptance.md), [backlog](../backlog.md) and [functional failures](../function-failures.md).

## Open work; no new implementation during stand-down

- Actual Codex/Claude session discovery/use of the registered MCP tools after client reload.
- AI Usage: live reader/source remains unresolved; unknown/unverified readings must stay unknown.
- Cooler screen: repeated failure, Windows descriptor problem 43; USB cable inaccessible. Existing bounded background
  diagnostics remain enabled. No repeat reset/reboot or hardware recovery is claimed.
- Remaining physical acceptance and sleep/resume (explicitly deferred), navigation latency, preview/UI refinement and Dial.
- Direct physical-action MCP tools are a later checkpoint, separate from 0.3.0 status/configuration.

Start any next feature by confirming current MCP state and the user's chosen priority. Do not treat known open checks as passed.
