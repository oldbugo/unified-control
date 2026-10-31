# Changelog

## Unreleased

Fix light flicker in UC's software dock previews: retain identical decoded images across polling, decode changed frames before swapping them in, and discard stale/detached results. Browser regressions cover unchanged-node retention, delayed decoding and navigation. Queued for the next build; installed 0.1.4 remains unchanged during hardware acceptance. Tracks issue #3.

Native text input also retains decoded preview nodes instead of rebuilding an HTML fallback on each keystroke. Rapid-typing regression verifies latest text persistence, render requests and the final decoded frame without a size flash.

## 0.1.4 — 2026-10-31

Editing one face of a physical key now shows that edited face as soon as its saved change reaches the dock, even if the key was displaying its other face. Clock, date, icon and other face edits no longer need an A/B round trip. Unrelated key selections, held-press deduplication and folder history stay intact. Name/action-only edits do not flip keys; a bulk change to both faces retains the current selection. Saving an edit never executes the key action.

## 0.1.3 — 2026-10-31

Fix the passive-only liveness check exposed by the 0.1.2 physical idle trial. The user saw a stable layout for about ten minutes, but diagnostics showed continuing keepalive writes, no passive heartbeats and repeated receive-timeout reconnects. After 25 seconds without input, send one probe using the already-selected dock brightness and wait for its real reply; retain the 35-second receive deadline if no response arrives. A successful write alone is never proof of liveness. Background image keepalive remains enabled.

The installer can use an explicitly supplied reviewed request-only UC executable when its installed-file view is shadowed by a packaged copy; it verifies the new process's logged startup version/PID rather than file metadata alone. All busy and unreachable-host guards remain enforced.

## 0.1.2 — 2026-10-31

Physical-test update: send the documented background small-window keepalive every five seconds after the first full page, only while the dock is enabled, connected and Studio is absent. Preserves the wide PNG and avoids resending unchanged images. Diagnostics count keepalives. This tests a host-idle fallback hypothesis: on 0.1.1 the user saw the dock's startup screen within 10–15 seconds while inbound heartbeats and clock partial updates continued with zero errors/reconnects. Physical efficacy is not yet established.

Installed quit/update requests now share the same default identity as the app's single-instance lock, independent of packaged APPDATA paths; debug/explicit-home channels remain isolated. The installer refuses to proceed if an existing app cannot be reached through its update guard.

## 0.1.1 — 2026-10-31

Compatibility and diagnostic update for a user-reported return to the physical dock's startup/default screen while UC still reports Connected. Tolerates nonstandard `0x010b` acknowledgement lengths, as the reference driver does; other input validation remains strict. Adds sanitized connection transitions, read-error kinds and heartbeat/ack/frame counters. Heartbeats and acknowledgements no longer force extra rendering. The cause of the physical fallback is not yet established; this is not a claim that it is fixed.

## 0.1.0 — 2026-10-31

Dock MVP acceptance preview. Adds native profiles/pages, A/B key editor, image/icon/date/live faces, all usage presentations, reviewed Windows actions, explicit dock control, Studio coexistence, reconnect, brightness and tray controls. Includes the managed reader runtime and monitor helpers in the installer. Autosave and Quit preserve pending imports and queued edits.

Software checks pass against isolated files, fixture providers and a simulated dock. Physical dock acceptance is pending; CoolerScreen and Dial engines are not integrated.
