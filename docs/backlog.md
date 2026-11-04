# Dock follow-up backlog

Prepared 0.5.0 addresses the remaining cooler data migration: custom source definitions become UC-owned; General can
verify the copied data without touching USB or rescanning on status reads. Original data is retained as rollback.
Installer-lineage replacement and the broader phase 2 release gates remain separate. [Details](device-app/cooler-migration.md).

2026-11-03: user reports manual cooler Reconnect recovered Live, but physical Live stops when leaving its page.0.4.0
prepares worker ownership/retirement fixes, General-only device enable switches, enabled-only Home/sidebar/search,
an unframed cooler pane and overall UI polish. Simulated tests are distinct from installed physical acceptance.

Cooler navigation regression reported2026-11-02 on0.3.4: entering an unavailable cooler could delay switching sections.
Fix installed in0.3.5: do not await transient USB preview cleanup on section navigation; retain saves/imports and full
shutdown guards, leave still-loading frames, remove the iframe on exit and avoid idle cleanup request flooding.
Startup/version, dock/provider reconnection and preserved layout verified; user feel remains pending. [Evidence/background cost](cooler-connection-health.md).

Planned phase2.5 sidebar search is installed in0.3.4. It filters page names and fixed related terms without saving or
navigating; Enter uses the existing guarded page activation. Three-width/compact keyboard, focus and filtering checks,
plus native Search-to-General/cooler/dock navigation, pass. Closing compact search restores every page; active filters stay
visible across compact resizing. User review remains separate from these software checks.

User feedback on 0.1.4, 2026-10-31. Folder and Back are confirmed functional. Updated 2026-11-01: preview fixes and virtual page following are built and software-tested for 0.1.5; user-visible verification remains pending.

| Item | Priority and next check |
| --- | --- |
| [#3: preview flashes during polling and text entry](https://github.com/oldbugo/unified-control/issues/3) | Nonblocking bug. Polling fix is prepared in source: keep unchanged decoded images and decode replacements before swapping. Text input also rebuilds the entire dock into an HTML fallback, which can flash at a different apparent scale; extend the native typing path to retain decoded key nodes. Regress latest text persistence/render requests and delayed decoding, then verify the next installed build. |
| [#5: virtual view follows physical page changes](https://github.com/oldbugo/unified-control/issues/5) | Requested behavior. Physical Test/folder and Back must update the virtual page, page strip and previews without a manual editor click. Preserve pending edits and avoid status-follow/save feedback loops. Current status updates only the On dock header; the preview uses the editor-selected page. Account for the difference between logical navigation and completed physical image delivery. |
| [#4: improve Folder/Back response time](https://github.com/oldbugo/unified-control/issues/4) | Performance investigation. User estimates about one second; improvement if practical. No deliberate one-second navigation wait exists. Measure input receipt, handling, provider acquisition/rendering, all fourteen PNGs, ZIP creation and final transfer before optimizing. Compare static/usage pages and editor open/closed. Record only timings/counts. Preserve freshness/provenance checks, response probes, keepalive and input deduplication. No hardware latency target has been established. |

The renderer runs before input polling and builds all fourteen faces before transferring a navigated page; shared usage helper calls can block. These are identified code paths, not a measured cause of the physical delay. Reuse frames only with the existing freshness/account safeguards intact.

Physical retest on0.3.2,2026-11-02: the user reports three Test/Back cycles feel responsive. Live MCP recorded six completed
navigations, zero aborted and a latest host-receipt-to-transfer duration of72.44ms. Earlier durations are unavailable;
the bounded capture missed the presses. No code scheduling change or measured speedup is attributed to this result.
Keep the older delay report as a watch item; this sample does not call for a speculative performance change.

Read-only follow-up on 2026-11-02: the worker consumes one inbound event per loop after rendering, so queued ACK/heartbeat
messages are another plausible delay. A Folder/Back event schedules rendering immediately. The next measurement batch
should record bounded durations/counts for polling, provider input, face rendering, ZIP creation and transfer, then compare
static/usage pages with the editor open/closed. Add fake held-render and ACK-burst fixtures before changing scheduling.
Preserve post-render admission, expiry-at-delivery, strict A/B/input deduplication, folder history, keepalive/probes,
fresh reconnect rendering and page-status publication only after successful delivery. No timings or speedup are claimed.

Version 0.3.2 adds fixed-size numerical timing diagnostics to dock state and MCP status: input preparation,
face rendering (including any usage-helper wait), delivery preparation, combined ZIP creation/transport writes, poll gaps
and non-button event counts. The last completed navigation spans host receipt to successful delivery; it cannot measure
physical press-to-host delay. Counts before a button are not measured queue depth. A cadence-limited log retains the same
summary without profile names, IDs, action values or account data. Fake held-render, FIFO ACK/heartbeat burst and abort
fixtures check attribution without changing scheduling. These diagnostics are installed in 0.3.2; hardware timing and
any optimization remain pending. [Capture guide](navigation-timing.md).

Issues #3 and #5 are implemented for 0.1.5. Native text input retains decoded nodes until the latest requested frame is decoded. Virtual page following uses successfully delivered page IDs, has a saved opt-out, waits for active edits/imports/saves and never saves observational navigation back. Browser checks cover these guards; quiet native fake IPC verifies Folder/Back following and preference persistence. Physical confirmation remains pending. Issue #4 remains an investigation; no navigation speedup is claimed. Sleep/resume is deferred at the user's request.
