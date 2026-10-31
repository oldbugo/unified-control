# Changelog

## 0.1.1 — 2026-10-31

Compatibility and diagnostic update for a user-reported return to the physical dock's startup/default screen while UC still reports Connected. Tolerates nonstandard `0x010b` acknowledgement lengths, as the reference driver does; other input validation remains strict. Adds sanitized connection transitions, read-error kinds and heartbeat/ack/frame counters. Heartbeats and acknowledgements no longer force extra rendering. The cause of the physical fallback is not yet established; this is not a claim that it is fixed.

## 0.1.0 — 2026-10-31

Dock MVP acceptance preview. Adds native profiles/pages, A/B key editor, image/icon/date/live faces, all usage presentations, reviewed Windows actions, explicit dock control, Studio coexistence, reconnect, brightness and tray controls. Includes the managed reader runtime and monitor helpers in the installer. Autosave and Quit preserve pending imports and queued edits.

Software checks pass against isolated files, fixture providers and a simulated dock. Physical dock acceptance is pending; CoolerScreen and Dial engines are not integrated.
