# Unified Control: current status

This is the one current-state page. Updated 2026-11-13. Older records are in [docs/archive/](archive/). History: docs/archive/ (do not read unless needed).

## What UC is

Unified Control (UC) is one visible Windows tray app. It runs the Ulanzi D200H stream dock, the Lovingcool SC70 cooler screen and, later, the Ulanzi Dial.
Devices are opt-in. Settings save as you change them. Quit stops the devices safely.

## State on 2026-11-13

- Installed UC: 0.9.2. Source: 8c194f2. Tag: v0.9.2.
- Cooler engine: 4.21.1. Source: 77c9086.
- The dock is connected.
- The cooler worker is running. Cooler Live is disconnected with a write_timeout error. The cause is open ([issue 9](https://github.com/oldbugo/unified-control/issues/9)).
- On 2026-11-09 the cooler worked after the user restarted the PC. That does not show the cause. Do not treat it as a fix.
- Connection diagnostics are on. Keep them on unless the user changes the setting.
- The freeze heartbeat switch (General, Freeze diagnostics) exists but is off. No real heartbeat trace exists yet.
  - When on, the host writes one line per minute to `freeze-heartbeat.log` (and `.previous.log`) in the UC app data folder.
  - It proves the host thread was alive. It does not prove frozen pixels, a GPU hang or a cooler freeze.
  - A gap can also mean sleep, exit or a disk error. Compare with Windows event times before you name a cause.
- Start with Windows is an opt-in General switch. The Run entry points to the installed exe with `--tray`. A real sign-in start was not tested.
- Windows LiveKernelEvent141 reports from 2026-11-10 point to dumps dated September and October. They do not show new GPU timeouts.
- Open pull requests: #36 (code cleanup) and #37 (docs cleanup). The user must merge them.

## Areas

| Area | Current result |
| --- | --- |
| Dock | Profiles, pages, 14 displays, A/B faces, editing, undo, actions and reconnect work. Edit refresh and Monitor Toggle are user-confirmed. Preview and page-follow fixes are installed. Final user confirmation is open. |
| AI usage | Both providers refresh past five minutes. Account-panel match, logout and switch, and physical gauges are not accepted yet. Claude needs the Desktop Code companion. |
| MCP | Codex client tests passed (read, preview, apply, conflict, undo). Claude client tests and physical-action tools are open. |
| Cooler | Separate engine 4.21.1 with themes, library, settings and safe lifecycle. Live is disconnected now (see above). Physical switching and brightness checks are open. |
| Shared services | One hardware collector, one account reader and one music and weather provider serve both workers. Verified in the installed app. |
| Cooler data | 286 library files, 22 gallery files and 23 references verified. No missing media. Original data kept as rollback. |
| Shell | Home cards, sidebar search, General switches and enabled-only navigation are installed. More visual review is open. |

## Next work, in order

1. **2.8 Installer replacement.** Done so far: read-only check and a synthetic backup, upgrade and rollback test (33 checks pass). Next: a verified backup and a reviewed registration and shortcut change. Done when the real NSIS retirement and a real 4.x rollback are tested. Keep the standalone CoolerScreen as the fallback. See [installer transition](device-app/installer-transition.md).
2. **2.9 Release gate.** Done when two-way worker crash isolation and busy-safe Quit are checked, privacy checks are finished, and a one-hour CPU and memory soak matches the standalone baseline. Sleep and resume stay deferred.
3. **Phase 3 Dial.** Done when host-mode switching is shown by documented or captured evidence (never guess commands), the controls are mapped, and a worker, a page and reconnect are tested.

## Open acceptance items

- Dock: more keys and wide areas, all action combinations, audio preservation, scaling review, preview flicker, physical usage gauges, account-panel match, logout and account switch.
- Claude MCP client test. Keep it apart from the Codex result.
- Cooler workspace entry: user confirmation of a smooth return. See [entry delay](cooler-entry-delay.md).
- Cross-device controls: physical theme, Live and brightness checks.
- Combined devices: wider Live and Embedded changes, crash recovery and soak test.
- Cooler hardware: find the cause of write_timeout. Keep diagnostics on. Do not do repeat resets or firmware commands as routine tests.
- Real freeze diagnosis and sign-in start test.
- Later work: update, privacy and diagnostics-export pages, signed updates, Studio profile import, physical-action MCP tools.
- Sleep and resume are deferred. Do not test them by default.

## Other docs

Evidence (dated records):
[acceptance](acceptance.md) · [cooler connection health](cooler-connection-health.md) · [function failures](function-failures.md) · [backlog](backlog.md) · [cooler entry delay](cooler-entry-delay.md) · [navigation timing](navigation-timing.md)

Reference (how it works):
[testing](testing.md) · [agent control (MCP)](agent-control.md) · [actions](actions.md) · [profiles](profiles.md) · [usage](usage.md) · [visuals](visuals.md) · [dock service](dock-service.md) · [architecture](device-app/architecture.md) · [cooler migration](device-app/cooler-migration.md) · [cross-device controls](device-app/cross-device-controls.md) · [dock worker](device-app/dock-worker.md) · [installer transition](device-app/installer-transition.md) · [shared hardware](device-app/shared-hardware-protocol.md) · [shared services](device-app/shared-services-protocol.md)

Also: [CHANGELOG](../CHANGELOG.md) · [README](../README.md)

## Restart checklist

1. Read the vault Start here, `CLAUDE.md` and this page.
2. Check Git status. Read the live state through the MCP tools. Do not trust this page for live facts.
3. Keep the user's settings and layout. If Ulanzi Studio owns the dock, UC stays blocked on purpose. The user quits Studio. Never close it for them.
4. Confirm the next task with the user. Create a branch named for your agent (for example `claude/` or `codex/`).
5. Do not reset devices, raise rights, or send guessed USB or firmware commands. Test with quiet fake instances. Use the idle-safe updater for approved updates.
6. A documentation change needs no build, no tag and no reinstall.
