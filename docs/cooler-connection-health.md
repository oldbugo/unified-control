# Cooler connection health and recovery

User request, 2026-11-01: the physical cooler freezes repeatedly, cannot connect through UC or original CoolerScreen, and needs recovery plus background evidence. This feature runs inside the visible UC tray app, not a Windows service or a Codex automation.

## Current incident

Before recovery, Windows showed normal cooler nodes with statusOK/problem0. UC's worker repeatedly opened WinUSB but timed out attempting to write, sending zero bytes in those failed opens. Its first Live attempt at06:40:35UTC already failed; retained logs contain no successful Live connection in that session. A brief ownership/access conflict aligned with original CoolerScreen opening around07:05UTC, then write timeouts resumed. This is not evidence that UC first connected and later froze, nor proof that development work caused the fault.

The old Reconnect path treated any normal USB identity as already connected and returned without resetting, even when firmware writes timed out. The new manual ResetPresent path was dry-run verified against exactly one cooler parent/port/hub interface. After the user approved Windows elevation, one port-cycle request succeeded around10:31:17UTC, but normal enumeration did not return within twenty seconds. Windows subsequently showed descriptor failure/problem43. The reset did not recover the screen. Only a physical USB reconnect/power removal can establish the next recovery result; a port cycle need not remove power. No broad hub reset, PC reboot/sleep, firmware update or unrelated-device reset was performed.

User says the screen USB cable is not accessible now. Physical recovery is deferred; do not keep resetting it or reboot/suspend the PC. 0.2.1 is installed and running, and the real background log confirms the ongoing matched code43 failure. Logging is evidence collection, not a remedy for the physical fault.

## Local diagnostics

Connection diagnostics is visible on the cooler page, enabled for the user's explicit request and saved as cooler.diagnostics. Turning it off stops sampling and disk writes, including discarding in-flight samples. UC closing to the tray keeps the watcher; Quit stops it.

- Worker health every10seconds: current process generation/restart count, Live's typed connection state, retries, coarse connect/stream/hold/off stage, FPS and RX/status counters. This RPC opens no USB handle and sends no device command.
- Passive Windows ConfigurationManager snapshot every30seconds: expected node count/problem codes and descriptor failures matched to the cooler's remembered parent/location. Identifiers are used only in-memory for attribution and never logged. Unmatched failures are not blamed on the cooler.
- Transitions plus minute snapshots append to `%APPDATA%/UnifiedControl/devices/cooler/connection-health.jsonl`, at most2MiB plus one2MiB rotated `.jsonl.old`. Before first completed migration, status is in memory and no log creates/poisons the future worker-home folder.
- Worker-health RPCs skip occupied command traffic and latch after timeout until a valid reply or new generation. They cannot accumulate pipe writes, mutate failure-uncertainty flags, or leave Quit busy. Native PnP enumeration remains separate from the worker so a worker stall and Windows device failure can be distinguished.

Logged values use a strict whitelist: no raw helper/error text, IPC payloads, account identifiers, quota readings, filenames, media tokens or physical device IDs. Coarse stage/counters cannot prove the exact failed USB command, firmware internal state or visible pixels. Logs provide evidence for diagnosis, not an automatic explanation of cause.

## Manual recovery

Reset cooler USB is explicit. It flushes edits/previews, runs through the cooler's exclusive device scheduler, waits for Windows administrator approval and protects active user work. Normally enumerated recovery requires exactly one physical cooler parent (never an MI interface), valid parent/location and one present hub interface, with a fresh identity check before the sole port-cycle operation. Missing enumeration uses the existing exact remembered-pair selector. No diagnostic timer invokes reset/elevation.

After reset, the engine must answer a real info request. Otherwise the UI reports failure and asks for a physical reconnect of only the screen USB connection. Original CoolerScreen app/data and all dock functionality remain separate. No automatic repeated reset or pump/fan power operation is performed.

## Verification

109 UC core tests and five host tests pass (one helper ignored); four worker tests, ten device-service tests and twenty pure PowerShell selection/effect checks pass. Headless checks cover saved diagnostics and explicit reset routing without OS effects. Quiet native fake IPC verifies log sampling while the editor is closed, opt-out persistence, imports, crash isolation and clean Quit. Synthetic tests cover redaction, rotation, cold-migration ownership and a silent helper's health timeout without busy/pending leaks. Automated tests never touch real hardware; the single manual reset above is recorded separately and failed to recover the device.
