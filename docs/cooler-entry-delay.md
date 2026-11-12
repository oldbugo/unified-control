# Cooler workspace entry delay
*Document type: dated evidence. Facts are true for the date shown. For the current state, read [status](status.md).*

Status: **open, user-reported on 2026-11-04**. Separate follow-up to dock-worker extraction.
The user notices a delay before the cooler workspace appears connected, including after rapid section switching.
They want near-instant return. A roughly fifteen-minute warm period was an optional idea, not a required design.
Prefer removing avoidable work before adding a retention timer.

## What the source establishes

- The enabled cooler worker and saved Live already continue when the editor leaves the cooler page or UC closes to the tray.
  Page navigation does not intentionally stop and reconnect that worker.
- `public/app.js` calls `CoolerPanel.unmount()` before replacing the page contents. The cooler iframe is removed.
  Returning creates a new `cooler/index.html` document in `public/js/cooler-panel.js`.
- The embedded document reruns its modules and initial hydration: `app_state`, sources/library reads,
  theme editor initialization and preview work where applicable, then a fresh `app_state` for the entry policy.
- Healthy Live owns its USB connection and retries. `shouldCheckScreenOnEntry` suppresses another automatic firmware
  check while Live or a physical preview owns the screen. This behavior is already installed in 0.4.1 and retained in 0.5.1.
- Therefore keeping the **engine** alive for fifteen minutes would not remove the known document-recreation work:
  that engine is already kept alive while enabled. Retaining an **editor document** would be a different change,
  requiring explicit suspension of visual loops, transient views and polling, plus safe restoration after worker recovery.

These facts identify a plausible UI-hydration cost. They do not establish which request or rendering step accounts
for the user's installed delay, or prove that the physical USB connection drops during page entry.

## Quiet software baseline

On 2026-11-04, `scripts/check-cooler-entry.cjs` passed against the actual bundled embedded UI in headless Chromium.
It covered repeated healthy Live entry without `device_check`, fresh-state entry decisions, explicit Check,
Live-off/editor-only-twin behavior and unknown-state refusal. No installed app, account or device was operated.

A temporary in-memory timing variant of that script measured iframe recreation through the exported `initialized`
promise for eight healthy Live visits. The first took 132.8 ms; seven later visits took 53.9–76.8 ms.
The fixture used immediate synthetic IPC replies and an empty library. Browser automation contributes to those
durations; they are not native WebView, real-library, first-paint, theme-preview or physical connection timings.
This does not reproduce or quantify the user's delay. No timing variant or fixture data was added to product code.

## Next investigation and completion check

1. Capture bounded durations from page selection to iframe bridge ready, first current worker state, library/theme
   hydration, and an editable theme preview. Measure repeated and rapid switching with a representative library.
   Record aggregate durations/counts only, without themes, paths or user contents in diagnostics.
2. Confirm whether worker generation, Live connection and device-check count remain stable across the same visits.
   Keep physical connection recovery separate from editor loading if they remain stable.
3. Remove the measured bottleneck first: parallelize truly independent reads or reuse validated metadata with revision
   invalidation as appropriate. Retain a suspended editor only if measurements justify its extra lifecycle complexity.
4. Verify imports/autosave/failed-save navigation guards, worker crash/recovery, disable/Quit, rapid-switch races,
   and absence of hidden preview/polling work. Ask the user to check the installed return-to-page feel.

Dock-worker extraction does not itself fix this issue. No cooler lifecycle or retention policy changed during this investigation.
