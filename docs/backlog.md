# Dock follow-up backlog

User feedback on 0.1.4, 2026-10-31. Folder and Back are confirmed functional. These follow-ups do not undo that acceptance result; no new install is required during current hands-on checks.

| Item | Priority and next check |
| --- | --- |
| [#3: preview flashes during polling and text entry](https://github.com/oldbugo/unified-control/issues/3) | Nonblocking bug. Polling fix is prepared in source: keep unchanged decoded images and decode replacements before swapping. Text input also rebuilds the entire dock into an HTML fallback, which can flash at a different apparent scale; extend the native typing path to retain decoded key nodes. Regress latest text persistence/render requests and delayed decoding, then verify the next installed build. |
| [#5: virtual view follows physical page changes](https://github.com/oldbugo/unified-control/issues/5) | Requested behavior. Physical Test/folder and Back must update the virtual page, page strip and previews without a manual editor click. Preserve pending edits and avoid status-follow/save feedback loops. Current status updates only the On dock header; the preview uses the editor-selected page. Account for the difference between logical navigation and completed physical image delivery. |
| [#4: improve Folder/Back response time](https://github.com/oldbugo/unified-control/issues/4) | Performance investigation. User estimates about one second; improvement if practical. No deliberate one-second navigation wait exists. Measure input receipt, handling, provider acquisition/rendering, all fourteen PNGs, ZIP creation and final transfer before optimizing. Compare static/usage pages and editor open/closed. Record only timings/counts. Preserve freshness/provenance checks, response probes, keepalive and input deduplication. No hardware latency target has been established. |

The renderer runs before input polling and builds all fourteen faces before transferring a navigated page; shared usage helper calls can block. These are identified code paths, not a measured cause of the physical delay. Reuse frames only with the existing freshness/account safeguards intact.

Issue #3 typing extension is now prepared and headless-tested: native text input retains decoded nodes until the latest requested frame is decoded; no HTML fallback is inserted while typing. The fix is not in installed 0.1.4. Issues #4 and #5 remain investigation/requested behavior, respectively.
