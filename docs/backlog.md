# Current project backlog

Updated2026-11-04. Development is paused. [Scope, phase status and ordered next work](project-status.md) is the current
implementation plan; [handoff](handoff-current.md) is the restart entry point. The next coding task is plan2.4 dock-worker
extraction, followed by shared providers and cross-device actions. No feature work starts during this cleanup checkpoint.

## Installed fixes with remaining acceptance

| Item | Current result and next check |
| --- | --- |
| [#3 Preview flicker](https://github.com/oldbugo/unified-control/issues/3) | Fixed/installed in0.1.5: retain unchanged decoded images, decode replacements before swapping, and keep preview nodes during typing. Browser/native tests pass. Remaining: user confirmation of installed preview/typing feel; investigate only a reproduced remaining fault. |
| [#5 Physical page following](https://github.com/oldbugo/unified-control/issues/5) | Implemented/installed in0.1.5, using successfully delivered physical page IDs, saved opt-out and pending-edit/import/save safeguards. Native fake Folder/Back/persistence checks pass. Remaining: explicit user confirmation that the virtual dock follows physical navigation correctly. |
| [#4 Folder/Back response](https://github.com/oldbugo/unified-control/issues/4) | Watch item. User reports three Test/Back cycles feel responsive on0.3.2. Numerical timing and held-render/ACK-burst fixtures are installed; no scheduling optimization or measured speedup is claimed. Reopen investigation on a reproduced delay with a fresh baseline. |
| [#6 AI Usage](https://github.com/oldbugo/unified-control/issues/6) | Reader launch fixed/installed in0.3.1; both providers refreshed beyond five minutes, and a transient Codex failure recovered. Remaining: physical gauges, exact account-panel comparison, account-switch/logout and reliability follow-up. Unknown/stale/unverified readings must suppress numbers. |
| [#7 Monitor Toggle](https://github.com/oldbugo/unified-control/issues/7) | Installed0.2.3 path fix, user-confirmed working on the selected Dell target. Broader action/monitor/audio combinations remain acceptance work. This is not an unimplemented feature. |

## Cooler and interface status

- 0.3.5 permits leaving an unavailable/loading cooler without waiting for transient preview cleanup. Saves/imports and
full disable/Quit cleanup remain guarded. Software/native held-cleanup tests pass; unavailable-device navigation feel
is not automatically established by a later healthy-screen check.
- 0.4.0 installs background Live ownership, General device toggles, enabled-only Home/sidebar/search, an unframed cooler
workspace and shell polish. User confirms physical Live survives leaving the page and closing UC to the tray.
- 0.4.1 skips repeat automatic firmware checks while Live owns the screen. Repeated-entry software/native checks pass;
explicit user confirmation of smooth reentry remains separate from the general theme/settings/Live check.
- 0.5.1 verifies independent cooler data/Sources migration while preserving UC edits and the original rollback copy.
Actual286 library/22 gallery files and23 active references pass. The user confirms existing themes/settings and physical Live.

Remaining interface work: installed scaling/usability review, further cooler styling, and the original General update,
privacy and diagnostics-export surfaces. Keep these separate from already-delivered shell/search/device-switch work.
Broader Live/Embedded transitions and combined recovery remain acceptance/release work. Installer-lineage replacement
is still open; original CoolerScreen installation/data remain rollback. [Migration contract](device-app/cooler-migration.md).

## Navigation evidence and boundaries

The older roughly-one-second Folder/Back report is historical. The0.3.2 retest records six completed navigations, zero
aborted, with only the latest72.44ms host-receipt-to-transfer sample retained. The capture missed those presses, so there
is no average, measured improvement or physical pixel latency. Input preparation/provider waits, face rendering,
delivery preparation, combined ZIP/transfer work, poll gaps and non-button events are now instrumented.

If a delay recurs, compare static/usage pages with the editor open/closed using bounded durations/counts. Preserve
account/freshness/expiry-at-delivery gates, strict A/B/input deduplication, folder history, keepalive/probes, fresh reconnect
rendering and page publication after successful delivery. [Capture guide](navigation-timing.md).

Sleep/resume is explicitly deferred. Real dock checks need the user to quit Studio; UC must not close it for them.
Claude model-client MCP acceptance, physical-action MCP tools and Dial remain open as listed in the project status.
