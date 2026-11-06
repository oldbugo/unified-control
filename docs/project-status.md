# Project scope and next work

Updated2026-11-06. The user selected the existing priority order; dock isolation, shared-services2.6 and software2.7 are complete.
This is the current project overview; older dated records remain evidence, not current task lists.

## Goal and boundaries

One visible Windows tray app controls the D200H stream dock, Lovingcool SC70 cooler screen and, later, Ulanzi Dial.
Settings apply as changed. Closing the window keeps enabled devices running; Quit stops them safely after active work.
Devices are opt-in, Start with Windows is opt-in, repositories stay private and user data stays local.
Keep separate UC, cooler-engine, Ulanzi-driver and Toolkit repositories. Do not replace the standalone CoolerScreen
installation or delete its data as a cleanup step. It remains the rollback copy.

## Completed checkpoint

Installed UC **0.9.0**, source `7c3ee6daaac24f90e85d3ae7359fe392fae5d5cb`, immutable tag `v0.9.0`.
PR29 adds typed dock-to-cooler controls, pinning engine4.21.0/sourceda2cb2c (CoolerScreen PR6).

Installed0.9.0/source7c3ee6d, engine4.21.0/sourceda2cb2c, passed quiet startup/new-process, actual MCP version, connected dock, both verified AI providers and unchanged selected fourteen-key layout checks. Cooler Live remains disconnected with write_timeout. A quiet comparison install of0.8.0/engine4.20.0 reproduced the same timeout;0.9.0 was restored. The initiating cause is unknown. Windows reports two normal USB nodes/problem0; no resets, elevation, firmware or actual cooler actions were performed. Physical switching/brightness acceptance remains open.

Plan2.4 provides independently supervised dock/cooler workers and host-only profiles. Plan2.6 adds one demand-driven
hardware collector, one host account reader/rasterizer and one supervised device-free music/weather provider. Workers
consume validated leased frames/readings with original source timestamps; replaced local readers stay off. Existing opt-ins,
account/history barriers, playback timing and weather cache/rates remain. The prior0.8.0 installation verified connected dock/cooler Live; the current connection failure and0.9.0 checks are recorded above.

| Area | Current result |
| --- | --- |
| Dock software | Profiles/pages, fourteen displays, strict two-face A/B, editing/autosave/undo, action adapters, keepalive/reconnect and Studio ownership protection are implemented. |
| Dock usability | Immediate physical face edits, persistence/reconnect and Monitor Toggle are user-confirmed. Folder/Back felt responsive in three retest cycles. Preview retention and virtual page-follow fixes are installed; final user confirmation remains open. |
| AI Usage | Canonical Windows launch defect fixed in0.3.1. Both providers refreshed beyond five minutes; a transient Codex failure recovered automatically. Exact account-panel, logout/switch and physical gauge acceptance remain open. Claude needs the authenticated Desktop Code companion. |
| MCP | Codex model-client status/read/preview/apply/conflict/undo acceptance passed. Layout edits coordinate with the visible editor. Claude model-client acceptance and physical-action tools remain open. |
| Cooler | Separate supervised engine4.21.0, embedded themes/library/settings, guarded imports and safe lifecycle are implemented. Prior user confirmation covers background Live and themes/settings; current cooler connection fails with write_timeout, reproduced on0.8.0 as well. Reentry avoids redundant automatic USB checks. |
| Shared hardware | One leased CPU/RAM/full NVIDIA collector supplies both workers. Local aggregate duplicates stay off; original expiry survives worker caches and dock delivery. Unused sources stop; independent consumer failure/recovery and actual installed1Hz counters are verified. |
| Shared AI/music/weather | One host account reader supplies bounded guarded frames; one provider-only process supplies selected cooler widgets. Original timing, cache/configuration binding, expiry and source/crash isolation pass. Idle provider retains bounded weather memory with readers off. Actual installed one-reader/one-provider process tree verified. |
| Cooler data | App-owned verification passes286 library files,22 gallery files and23 active references. No missing media or old-root dependencies. One UC theme edit/two regenerated previews retained; original rollback data kept. |
| Shell | Home/status cards, sidebar search, General device switches, enabled-only navigation, unframed cooler pane and overall polish are installed. Remaining visual review is acceptance/refinement work. |

Latest0.9.0 software gate:202 core/11 host/62 reader tests pass, with three existing ignored checks; cooler five dock-action/19 scheduler/nine worker cases pass. Headless and native cross-device/profile/MCP/hardware/worker/cooler/lifetime/migration/shared-provider suites pass. The earlier shared receiver/music/weather gates also passed. A prior opaque lifecycle poll failure passed unchanged-binary retry; initiating cause is unconfirmed.
Bounded normal pipe admission and improved fixture errors were added; final affected native reruns pass.
Worker Rust modules and diff whitespace pass. Whole-repository formatting still has unrelated existing differences.
See [acceptance evidence](acceptance.md), [functional follow-ups](function-failures.md) and [migration contract](device-app/cooler-migration.md).

## Next implementation order

This follows the existing build plan. Acceptance items below can be closed alongside the relevant feature work;
they do not authorize device tests while the user is away.

| Order | Existing plan | Concrete outcome and completion check |
| --- | --- | --- |
| 1 | 2.8 Installer replacement | Data migration is done for the current setup. Define/test installer lineage, rollback and retirement of the separate CoolerScreen install after dependencies are ready. Retain untouched rollback data. |
| 2 | 2.9 Combined release gate | Retain two-way worker crash isolation/busy-safe Quit checks, finish privacy checks and one-hour CPU/memory soak against the standalone baseline. Sleep/resume stays deferred until the user reauthorizes it. |
| 3 | Phase3 Dial | Research host-mode switching from documented/captured evidence; never guess commands. Then map physical controls, implement its worker/page and test reconnect. Media-mode fallback has fewer distinct controls and must be explicit. |

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
- Cross-device2.7: software/native fake route and iframe state reflection pass. Physical theme/Live/brightness checks remain open.
- Cooler hardware: current Live/USB connection fails with write_timeout, also reproduced on0.8.0. Cause/recovery remain unresolved;
  do not perform repeat resets, firmware commands or broad recovery as routine acceptance.

## Restart checklist

Read [current handoff](handoff-current.md), refresh the private vault, read standing instructions and inspect Git status.
Read running UC through MCP; preserve the user's settings and layout. If Studio owns the dock, UC correctly stays blocked;
the user quits Studio when they want dock testing. Never close it for them.
Reconfirm the next task with current user direction, then use a new `codex/` branch. No build/reinstall is needed for this documentation checkpoint.
