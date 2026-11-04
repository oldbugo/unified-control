# Dock worker boundary

Plan 2.4, implemented on 2026-11-04. Installation and verification results are recorded in
[acceptance](../acceptance.md); this contract alone is not evidence of physical-device acceptance.

## Ownership

UC owns the visible window/tray, settings, scheduler, MCP/editor leases and the only durable profile store.
The same executable runs a separate `--dock-worker` process before any GUI or single-instance initialization.
That child owns the dock driver, rendering, A/B and folder runtime, keepalive/reconnect, sensors, reviewed native
actions and the usage reader. Shared providers remain the next milestone; no additional account poller is introduced.
The host's dormant rendering resources serve assets, read-only monitor discovery and MCP's explicitly static preview.
Normal editor previews go to the child.

The child receives validated profile snapshots in a memory-only replica. It cannot write the profile journal,
startup backup or index. After a crash the replacement receives the latest durable host snapshot, brightness and
enabled setting. Queued button events and native actions are never replayed. Physical Folder/Back selection remains
runtime state, published only after successful delivery, with the existing editor-follow safeguards.

## Transport and recovery

Versioned, typed JSON messages use inherited private stdin/stdout pipes, without a public endpoint or network
command listener. Frames are bounded at 64 MiB; unknown operations/fields, malformed messages and incomplete frames
are rejected. Only reviewed configuration, previews and debug/fake input operations cross the boundary.
The supervisor retries after actual process exit with bounded backoff; it never starts a second driver beside a
living old child. Process status, generations and counters are local diagnostics, without user profile contents.
Worker device logs live under `devices/dock`.

Pipe loss, stale heartbeats or uncertain requests invalidate cached delivered-page and verified-usage claims.
Preview batches carry expiry deadlines through IPC. Guards run after batch rendering and on host receipt, and the
host rejects results from disconnected generations. Existing provider, provenance and physical-delivery guards remain.

## Safe shutdown

Host operations coordinate with a child-side admission lock covering transfers and actions. Busy disable/Quit is
refused; a successful quiesce blocks further work before the cooler shutdown/relaunch handshake. A refused cooler
shutdown or failed relaunch restores dock admission. Reply timeouts retain uncertain work rather than authorize a kill.

On parent-pipe loss the child closes admission, discards queued commands and drains an active action before exit.
The dock child is deliberately not put in a kill-on-close Job Object: terminating its descendants could interrupt
Monitor Toggle recovery or close a user application launched by a key. Normal shutdown waits for cooperative exit.
No automatic USB resets, firmware operations, forced app closure or new service is introduced.

## Verification boundaries

Use isolated quiet debug hosts and fake devices. Check distinct child identity, both-way crash isolation, cooler Live
continuity, current-profile recovery, no action replay, durable profile ownership, input deduplication, busy refusal,
preview expiry and cooperative child shutdown. Fake account tests do not establish live account freshness.
After a tested idle-safe install, read actual MCP state and compare the selected page/all fourteen keys with the
pre-install snapshot. Remaining physical usability, account switching and sleep/resume acceptance stay separate.

The [cooler workspace entry delay](../cooler-entry-delay.md) is a separate open interface issue. The cooler engine
already remains enabled across page switches; dock-worker extraction does not claim to improve its hydration time.
