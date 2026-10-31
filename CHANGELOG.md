# Changelog

## 0.1.2 — 2026-10-31

Physical-test update: send the documented background small-window keepalive every five seconds after the first full page, only while the dock is enabled, connected and Studio is absent. Preserves the wide PNG and avoids resending unchanged images. Diagnostics count keepalives. This tests a host-idle fallback hypothesis: on 0.1.1 the user saw the dock's startup screen within 10–15 seconds while inbound heartbeats and clock partial updates continued with zero errors/reconnects. Physical efficacy is not yet established.

Installed quit/update requests now share the same default identity as the app's single-instance lock, independent of packaged APPDATA paths; debug/explicit-home channels remain isolated. The installer refuses to proceed if an existing app cannot be reached through its update guard.

## 0.1.1 — 2026-10-31

Compatibility and diagnostic update for a user-reported return to the physical dock's startup/default screen while UC still reports Connected. Tolerates nonstandard `0x010b` acknowledgement lengths, as the reference driver does; other input validation remains strict. Adds sanitized connection transitions, read-error kinds and heartbeat/ack/frame counters. Heartbeats and acknowledgements no longer force extra rendering. The cause of the physical fallback is not yet established; this is not a claim that it is fixed.

## 0.1.0 — 2026-10-31

Dock MVP acceptance preview. Adds native profiles/pages, A/B key editor, image/icon/date/live faces, all usage presentations, reviewed Windows actions, explicit dock control, Studio coexistence, reconnect, brightness and tray controls. Includes the managed reader runtime and monitor helpers in the installer. Autosave and Quit preserve pending imports and queued edits.

Software checks pass against isolated files, fixture providers and a simulated dock. Physical dock acceptance is pending; CoolerScreen and Dial engines are not integrated.
