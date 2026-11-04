# Changelog

## 0.7.0 — 2026-11-05

Collect CPU, RAM and full NVIDIA GPU readings once for interested dock/cooler displays. Expiring per-consumer demand
selects the required source families and fastest1/2/4Hz rate; unused sources stop. Cooler engine4.19.0 consumes injected
readings through both legacy stats and modern widgets, with replaced local aggregate readers disabled.

Preserve original observation/expiry through worker exchanges, cached status and dock pixel delivery. Missing, failed
or expired readings remain unknown; validated generation/sequence checks prevent replay. Independent coalescing delivery
keeps a stalled consumer from blocking its peer. Ephemeral probes cannot create durable-action uncertainty.

This is shared-services slice2.6a. AI usage, music, weather and other data sources retain existing ownership for now.
Cooler workspace entry delay remains separately tracked in issue23.
Installed clean sourceb5b0140 through the idle-safe updater; startup/newPID, actual MCP0.7.0/engine4.19.0, connected
devices and unchanged selected fourteen-key page verified. Real cooler demand drives one1Hz collector, with equal
CPU/RAM/GPU read counts and no source failures. Source/tag and private PR25 identify the release.

## 0.6.0 — 2026-11-04

Move the dock driver, renderer, profile runtime, native actions and account reader into a supervised process.
UC remains the only durable profile writer; the child uses validated memory-only snapshots. A replacement starts
with the latest saved layout and never replays button actions. Editor previews share the child reader and preserve
freshness deadlines through IPC. Quiesce/acknowledgement coordinates busy-safe disable, Quit and updates.
The cooler worker remains independent. Shared providers and cross-device actions follow in later milestones.
Installed clean source0117a5f through the idle-safe updater. Startup/newPID, separate dock child, actual MCP0.6.0,
device reconnection, expected-only proxy marker substitution and unchanged selected fourteen-key page verified.
165 core/11 host/62 reader tests and relevant headless/quiet native checks pass. PR22 is merged; v0.6.0 pins the build source.

Record the separately reported cooler workspace entry delay. The engine already stays running; repeated interface
hydration is a possible cost to measure. No warm timer or cooler-entry performance fix is claimed in this release.

## 0.5.1 — 2026-11-04

Complete cooler data migration verification while preserving UC edits and original rollback data. UC-owned custom source
definitions replace the old default-folder dependency. General offers a cancellable app-owned comparison and historical
summary; MCP reads cached finite counts without scanning. Atomic bounded receipts invalidate older success during interruption.
Generated theme previews are counted separately from original media; missing media and stale active paths still fail.
Installed sourceb67d928, startup/MCP/proxy/reconnection/layout verified. Actual286 library/22 gallery files and23 references
pass; user confirms themes/settings and physical Live.155 core/11 host and relevant headless/native checks pass.

## 0.5.0 — 2026-11-04

Introduce copy-once V2 source-definition migration and host-side data comparison. The real-data check conservatively
flags two changed generated previews, corrected in0.5.1. Original files and UC edits are preserved throughout.


## 0.4.1 — 2026-11-03

Opening the cooler workspace no longer interrupts an active Live stream with another automatic firmware check.
After editor hydration, use a fresh worker snapshot: Live owns its startup/retry/held connection, and physical previews
retain their connection too. An actually-off screen still gets its initial check. Explicit Check/Reconnect remain.

The user grants standing permission to install tested UC updates as needed without repeated approval questions;
the idle-safe updater, quiet restart and verification remain mandatory.
Installed from clean sourcebc9a864 through the idle-safe updater; startup/newPID, actual MCP0.4.1, packaged proxy and
unchanged selected profile/page/all14keys verified. Real reentry smoothness remains separate from the automated check.

## 0.4.0 — 2026-11-03

Move device enable switches into General. Only enabled devices appear in Home, the sidebar and search; enabling a device
does not leave settings, and failed changes preserve the previous state. Hide the unimplemented Dial entry. Remove large
device enable headers and let the cooler workspace use the full right pane without an outer frame or duplicate app name.
Refine navigation, spacing, typography, cards, settings and switches, with compact layouts and reduced-motion support.

Preserve saved cooler Live across editor page changes and close-to-tray. Claim temporary editor streams when Live is
enabled, clear stale temporary ownership after explicit off, and detach only the temporary stream being cleaned up.
Explicit Live off/device disable/Quit retain their behavior; autosave/import guards and local diagnostics remain.

Installed from clean sourceb18a4a1 with startup/newPID, actual MCP0.4/engine4.18.2, packaged proxy and unchanged14-key
layout verified. User confirms physical Live continues across page changes and close-to-tray; repeated entry is fixed above.

## 0.3.5 — 2026-11-02

Fix cooler section navigation waiting for unavailable-device preview cleanup. Real edits and imports still flush before
the frame is removed; transient preview/watch cleanup continues through the parent without blocking other sections.
Loading frames can be left before their bridge is ready. Idle visits do not enqueue redundant cleanup requests.
The embedded view is removed on exit, stopping its visual loops; enabled worker Live/diagnostics remain independent.
Disable, reset and Quit retain their full cleanup checks. No USB recovery or hardware fix is claimed.

Installed with user approval through the idle-safe updater. NewPID/startup version, actual MCP0.3.5, packaged proxy,
dock/provider reconnection and unchanged active14-key layout verified. Hardware recovery and user navigation feel remain open.

## 0.3.4 — 2026-11-02

Add the planned sidebar page search. Match page names and related terms such as usage, themes and MCP. Filtering does not
save or navigate; Enter opens the first result through the existing guarded navigation path. Escape clears the query.
Keyboard focus skips hidden results. Compact sidebars expose a search button and can expand/collapse without losing access.
Closing compact search clears its filter; resizing with an active query keeps search visible instead of hiding pages
behind an invisible filter.

Installed corrected source95a68ee through the idle-safe updater, with newPID/startup version, actual MCP version,
packaged proxy identity, unchanged active layout and dock/provider recovery verified. The earlier draft package was not installed.

## 0.3.3 — 2026-11-02

Refresh Home device cards in place when accepted status polls arrive, preserving keyboard focus. Distinguish the cooler
engine's state from its last connection check on Home and the cooler page. Missing, disabled, failed or out-of-date
checks remain distinct; cached connection reports are not a guarantee of current physical operation.

Expose a strict cached cooler-health summary through enabled MCP status, without triggering collection or recovery.
Worker and Windows USB sample timestamps are separate. Identifiers, paths, PIDs and arbitrary error messages are excluded.

Installed with startup/new PID, actual MCP version, packaged proxy identity and unchanged active layout verified. Dock
and both usage providers recovered. The last cooler sample still reports descriptor failure43; no hardware recovery.

Physical0.3.2 retest: the user reported three Test/Back cycles responsive; six navigations completed without aborts.
The latest host-receipt-to-transfer duration was72.44ms. No before/after speedup or physical pixel timing is claimed.

## 0.3.2 — 2026-11-02

Keep keyboard focus on the matching dock editor control after display/face/presentation/action changes. Tab continues
through the editor. Undo keeps the selected display on the same stable profile/page, with a safe fallback if that page
is removed. Explicit navigation and agent/editor coordination retain their existing behavior.

Add bounded numerical navigation timings to dock state and enabled MCP status. Separate input preparation, face rendering
(including usage-helper waits), delivery preparation and combined ZIP/transport work. Retain one latest completed
host-receipt-to-delivery sample and navigation completion/abort counts. No scheduling change or physical speedup is claimed.

Preserve guarded Codex error/stale states while continuing to hide all unverified numbers. Missing account identity still
suppresses otherwise valid readings. Simulated regressions cover throttling, recovery and history/token clearing.

Installed through the idle-safe updater during the user's requested continuation. New process/startup version, MCP0.3.2,
dock reconnection, unchanged active-page keys and both providers verified. Numerical timings are readable; physical
navigation capture/feel remains separate from automated checks.

## 0.3.1 — 2026-11-02

Fix AI Usage reader startup when Windows supplies canonical extended resource paths. Normalize trusted disk/UNC executable,
script and usage-home paths before starting Node, preserving Unicode and other device namespaces. The canonical-path
fixture reproduces `reader-unavailable` before the fix and passes afterward without account access. Live account refresh
and physical usage acceptance are checked separately. Installed through the user-approved idle-safe updater;
new process/startup version, dedicated proxy hash and dock reconnection verified. Both providers reached verified status.

Codex desktop MCP acceptance now covers actual model-driven status/layout/monitor reads, preview, inactive-profile
apply/readback, stale revision refusal and undo. The temporary face was restored and active dock selection preserved.
Claude model-client acceptance remains open.

## 0.2.1 — 2026-11-01

Add the requested background Connection diagnostics in UC, on with a saved opt-out. Read-only worker health runs every ten seconds; passive Windows USB status every thirty seconds. State transitions and minute snapshots go to bounded local rotating logs. Worker timeouts, USB write failures, matched descriptor failures and intentional holds remain distinct; raw messages, device/account IDs and media paths are excluded. Logging continues with the editor closed and stops when UC exits. No automatic USB resets or elevation.

Add explicit Reset cooler USB for a normally enumerated but unresponsive screen. It saves edits, uses the exclusive device job, requests Windows approval, selects one exact cooler parent/port/interface, resets once and checks for a genuine firmware reply. Failed enumeration keeps the existing exact remembered-port recovery path. Successful reset request alone is never reported as recovered hardware.

Current incident remains unresolved: initial PnP nodes were OK while writes timed out before the session's first successful Live connection. One user-approved targeted reset was accepted, but normal enumeration did not return; Windows then reported code43. Physical USB reconnect is the next recovery step. This does not establish development work, firmware, power or a cable as the initiating cause.

## 0.2.0 — 2026-11-01

Integrate CoolerScreen themes, library, Live, Embedded and settings into the cooler page using the existing interface and a separate supervised engine from the private CoolerScreen repository. Devices remain separately enabled; cooler starts off. First enable imports a separate copy of the old settings/library/gallery and preserves the fallback. Existing CoolerScreen must be closed by the user before UC takes ownership.

Use private inherited pipes with validated commands/events, bounded chunked file imports, retained busy-work guards, explicit saved-Live activation and independent worker restart. Autosaved cooler edits/imports flush before leaving or quitting. Bundle the pinned engine, reviewed ffmpeg and existing setup/reconnect scripts; apply a UI content security policy.

Fix Monitor Toggle discovery for PowerShell zero/one/many display output. Add safe AI Usage reader/provider status messages without weakening freshness/account gates. Both user-reported function failures remain open for verification. UI polish and physical cooler/migration acceptance are pending; sleep testing remains deferred.

Correct native async test waits to explicitly poll resolved IPC conditions. Native tests cover real host/worker IPC and iframe routing, imports, crash isolation, busy refusal and clean shutdown using fake devices only.

## 0.1.5 — 2026-11-01

The virtual dock follows completed physical profile/page navigation, including Folder and Back. Follow physical page is enabled by default for the user's requested behavior and can be turned off. Observational page following never sends navigation back to the dock and waits for pending edits/imports/saves; manual inspection and disconnected states do not cause polling to pull the editor back. The preference persists in app settings. Physical page status is published after full-page transfer succeeds.

Fix light flicker in UC's software dock previews: retain identical decoded images across polling, decode changed frames before swapping them in, and discard stale/detached results. Browser regressions cover unchanged-node retention, delayed decoding and navigation. Tracks issue #3.

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
