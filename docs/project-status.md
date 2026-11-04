# Project scope and next work

Updated 2026-11-05. The user selected the existing priority order; dock isolation and shared-hardware slice2.6a are complete.
This is the current project overview; older dated records remain evidence, not current task lists.

## Goal and boundaries

One visible Windows tray app controls the D200H stream dock, Lovingcool SC70 cooler screen and, later, Ulanzi Dial.
Settings apply as changed. Closing the window keeps enabled devices running; Quit stops them safely after active work.
Devices are opt-in, Start with Windows is opt-in, repositories stay private and user data stays local.
Keep separate UC, cooler-engine, Ulanzi-driver and Toolkit repositories. Do not replace the standalone CoolerScreen
installation or delete its data as a cleanup step. It remains the rollback copy.

## Completed checkpoint

Installed UC **0.7.0**, source `b5b014034baf87597f5dac7202b2393e2249af92`, immutable tag `v0.7.0`.
PR25 contains shared hardware, pinning engine4.19.0/source88aad7d (CoolerScreen PR4). Later documentation does not change this build.

Version0.6.0 implements plan2.4 in a supervised dock process with host-only durable profiles, guarded IPC previews,
latest-layout recovery without action replay and busy-safe shutdown. Version0.7.0 adds one demand-driven shared hardware
collector and injected dock/cooler readings, suppressing replaced local aggregate readers. Software/native/packaging gates
and quiet update pass; startup, MCP, engine pin, connections and unchanged fourteen-key page are verified. Initial real
collector counters show1Hz cooler demand, equal CPU/RAM/GPU reads and no source failures. Shared AI/music/weather remain next.

| Area | Current result |
| --- | --- |
| Dock software | Profiles/pages, fourteen displays, strict two-face A/B, editing/autosave/undo, action adapters, keepalive/reconnect and Studio ownership protection are implemented. |
| Dock usability | Immediate physical face edits, persistence/reconnect and Monitor Toggle are user-confirmed. Folder/Back felt responsive in three retest cycles. Preview retention and virtual page-follow fixes are installed; final user confirmation remains open. |
| AI Usage | Canonical Windows launch defect fixed in0.3.1. Both providers refreshed beyond five minutes; a transient Codex failure recovered automatically. Exact account-panel, logout/switch and physical gauge acceptance remain open. Claude needs the authenticated Desktop Code companion. |
| MCP | Codex model-client status/read/preview/apply/conflict/undo acceptance passed. Layout edits coordinate with the visible editor. Claude model-client acceptance and physical-action tools remain open. |
| Cooler | Separate supervised engine4.19.0, embedded themes/library/settings, guarded imports and safe lifecycle are implemented. Prior user confirmation covers background Live and themes/settings; current source/connection recovery passes. Reentry avoids redundant automatic USB checks. |
| Shared hardware | One leased CPU/RAM/full NVIDIA collector supplies both workers. Local aggregate duplicates stay off; original expiry survives worker caches and dock delivery. Unused sources stop; independent consumer failure/recovery and actual installed1Hz counters are verified. |
| Cooler data | App-owned verification passes286 library files,22 gallery files and23 active references. No missing media or old-root dependencies. One UC theme edit/two regenerated previews retained; original rollback data kept. |
| Shell | Home/status cards, sidebar search, General device switches, enabled-only navigation, unframed cooler pane and overall polish are installed. Remaining visual review is acceptance/refinement work. |

Latest0.7.0 software gate:179 core and11 host tests plus62 reader tests pass, with three pre-existing ignored checks;
seven focused cooler shared tests/four worker tests and relevant headless/native shared/worker/profile/MCP/cooler/lifetime
suites pass. One isolated cooler renderer helper is invoked by its parent test.
Worker Rust modules and diff whitespace pass. Whole-repository formatting still has unrelated existing differences.
See [acceptance evidence](acceptance.md), [functional follow-ups](function-failures.md) and [migration contract](device-app/cooler-migration.md).

## Next implementation order

This follows the existing build plan. Acceptance items below can be closed alongside the relevant feature work;
they do not authorize device tests while the user is away.

| Order | Existing plan | Concrete outcome and completion check |
| --- | --- | --- |
| 1 | 2.6b Remaining shared services | Hardware slice2.6a is installed. Move AI usage, now-playing and weather ownership onto the shared demand model, preserving existing activation, cache, account/freshness and rate policies. Current music/weather remain cooler-only and AI dock-only; do not add new faces as an implicit part of ownership work. |
| 2 | 2.7 Cross-device actions | Dock keys select cooler themes, next/previous, Live on/off and brightness through named worker operations. Verify physical result and reflected cooler UI state with the user. |
| 3 | 2.8 Installer replacement | Data migration is done for the current setup. Define/test installer lineage, rollback and retirement of the separate CoolerScreen install after dependencies are ready. Retain untouched rollback data. |
| 4 | 2.9 Combined release gate | Retain two-way worker crash isolation/busy-safe Quit checks, finish privacy checks and one-hour CPU/memory soak against the standalone baseline. Sleep/resume stays deferred until the user reauthorizes it. |
| 5 | Phase3 Dial | Research host-mode switching from documented/captured evidence; never guess commands. Then map physical controls, implement its worker/page and test reconnect. Media-mode fallback has fewer distinct controls and must be explicit. |

Remaining phase2.5 shell work includes the planned update/privacy/diagnostics-export surfaces and further cooler styling.
Do not treat the installed shell polish as completion of every original General-page item.
Direct physical-action MCP tools, Studio profile import, signed updates and other device families remain later work or separate decisions.
Monitor Toggle is already built and confirmed; it is not a future implementation item.

## Acceptance and watch list

- Dock: remaining key/wide-area coverage, all action combinations, explicit audio preservation, scaling/usability review,
  installed preview flicker/virtual following, physical usage gauges and account-panel agreement/logout/account switching.
- Claude: actual model-client MCP acceptance. Keep Codex's completed acceptance separate.
- Cooler reentry: repeated-entry software/native tests pass; specific user confirmation of smooth return remains open.
- Combined devices: broader Live/Embedded transitions, crash recovery and measured soak remain distinct from current-setup continuity.
- Sleep/resume: explicitly deferred; do not test it on resume by default.
- Navigation: user reports responsive behavior; installed numerical timing is available. Investigate again only on a reproduced delay,
  with a baseline and bounded timing capture. No measured speedup or physical pixel latency is claimed.
- Cooler hardware: current Live/USB connection passed. Prior enumeration/write failures and their cause remain historical/unresolved;
  do not perform repeat resets, firmware commands or broad recovery as routine acceptance.

## Restart checklist

Read [current handoff](handoff-current.md), refresh the private vault, read standing instructions and inspect Git status.
Read running UC through MCP; preserve the user's settings and layout. If Studio owns the dock, UC correctly stays blocked;
the user quits Studio when they want dock testing. Never close it for them.
Reconfirm the next task with current user direction, then use a new `codex/` branch. No build/reinstall is needed for this documentation checkpoint.
