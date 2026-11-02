# Dock follow-up backlog

User feedback on 0.1.4, 2026-10-31. Folder and Back are confirmed functional. Updated 2026-11-01: preview fixes and virtual page following are built and software-tested for 0.1.5; user-visible verification remains pending.

| Item | Priority and next check |
| --- | --- |
| [#3: preview flashes during polling and text entry](https://github.com/oldbugo/unified-control/issues/3) | Nonblocking bug. Polling fix is prepared in source: keep unchanged decoded images and decode replacements before swapping. Text input also rebuilds the entire dock into an HTML fallback, which can flash at a different apparent scale; extend the native typing path to retain decoded key nodes. Regress latest text persistence/render requests and delayed decoding, then verify the next installed build. |
| [#5: virtual view follows physical page changes](https://github.com/oldbugo/unified-control/issues/5) | Requested behavior. Physical Test/folder and Back must update the virtual page, page strip and previews without a manual editor click. Preserve pending edits and avoid status-follow/save feedback loops. Current status updates only the On dock header; the preview uses the editor-selected page. Account for the difference between logical navigation and completed physical image delivery. |
| [#4: improve Folder/Back response time](https://github.com/oldbugo/unified-control/issues/4) | Performance investigation. User estimates about one second; improvement if practical. No deliberate one-second navigation wait exists. Measure input receipt, handling, provider acquisition/rendering, all fourteen PNGs, ZIP creation and final transfer before optimizing. Compare static/usage pages and editor open/closed. Record only timings/counts. Preserve freshness/provenance checks, response probes, keepalive and input deduplication. No hardware latency target has been established. |

The renderer runs before input polling and builds all fourteen faces before transferring a navigated page; shared usage helper calls can block. These are identified code paths, not a measured cause of the physical delay. Reuse frames only with the existing freshness/account safeguards intact.

Read-only follow-up on 2026-11-02: the worker consumes one inbound event per loop after rendering, so queued ACK/heartbeat
messages are another plausible delay. A Folder/Back event schedules rendering immediately. The next measurement batch
should record bounded durations/counts for polling, provider input, face rendering, ZIP creation and transfer, then compare
static/usage pages with the editor open/closed. Add fake held-render and ACK-burst fixtures before changing scheduling.
Preserve post-render admission, expiry-at-delivery, strict A/B/input deduplication, folder history, keepalive/probes,
fresh reconnect rendering and page-status publication only after successful delivery. No timings or speedup are claimed.

Issues #3 and #5 are implemented for 0.1.5. Native text input retains decoded nodes until the latest requested frame is decoded. Virtual page following uses successfully delivered page IDs, has a saved opt-out, waits for active edits/imports/saves and never saves observational navigation back. Browser checks cover these guards; quiet native fake IPC verifies Folder/Back following and preference persistence. Physical confirmation remains pending. Issue #4 remains an investigation; no navigation speedup is claimed. Sleep/resume is deferred at the user's request.
