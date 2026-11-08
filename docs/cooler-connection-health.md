# Cooler connection health and recovery

User request, 2026-11-01: the physical cooler freezes repeatedly, cannot connect through UC or original CoolerScreen, and needs recovery plus background evidence. This feature runs inside the visible UC tray app, not a Windows service or a Codex automation.

## Current recovery:2026-11-09

**Recovery checked2026-11-09:** the user reports the physical cooler screen works after restarting/power-cycling the PC. The same installedUC0.9.1/engine4.21.1 records a complete successful connect at2026-11-08T22:29:19.536UTC (09:29:19SydneyNov9): open209ms, end_stream0ms, genuine info16ms, preserve_ending0ms, enter_live215ms, total643ms. The next health sample reports streaming19FPS, no holds, normal Windows nodes/problem0 and no worker restarts. Earlier retained attempts through2026-11-08T09:25:20UTC still failed at the initial write. This establishes recovery after the user-reported PC cycle without a software update, not the initiating cause or uninterrupted reliability. Diagnostics stay enabled; no agent reset/restart/sleep/physical action was used.

The failed state persisted across app-only restarts and a0.8 comparison; a PC power cycle later cleared it on the same0.9.1 build. This is consistent with retained device/USB state, but cannot distinguish device firmware, Windows transport/driver state or another effect of restarting. Do not label the disconnect fixed. Record the next first failing step, last successful stream, stop results, worker generation/holds and passive Windows evidence before any authorized recovery. Current log rotation is bounded; preserve incident boundary summaries at development checkpoints.

## Step diagnostics (0.9.1 / engine4.21.1)

User-approved2026-11-06 after reviewing the retained logs. A final healthy snapshot at03:38:42.267UTC precedes the update stop at03:38:55.019. First reopen03:39:02.043 closes with20 successfully sent bytes: EOS4 + genuine-info request7 + preserve-ending9. Matching current source localizes the next failed write to enter-Live; later opens fail before sending any successful bytes. Normal Windows nodes remain. This correlation does not identify firmware versus transport cause, and the failed0.8.0 comparison cannot rule out the initial transition as trigger.

New private `cs:connection_trace` events name each connect/normal-stop/fast-stop/cancel-stop step, finite result and elapsed milliseconds. Commands, order, sleep intervals, timeouts and original errors are unchanged; failed stop writes still permit the remaining best-effort steps. Memory-buffered records are emitted after the USB sequence, through a16-entry nonblocking queue with a250ms final drain. Excess or blocked diagnostics may be dropped; they never trigger retry/reset. No per-frame or keepalive log is added.

UC strictly validates schema1, operation/step/result enums, at most8steps and duration bounds, then appends `kind:live_steps` to the existing2MiB-plus-rotation connection-health log. The diagnostics switch, same serialized opt-out boundary and completed-migration guard apply. Events are internal, not forwarded to the iframe or MCP. No payload, ID, path or arbitrary error text is included. Successful writes confirm transport completion only, not visible pixels or acknowledged Embedded playback. Existing coarse health snapshots remain unchanged.

On2026-11-06, installed0.9.1 real records confirmed open succeeded and the initial end_stream write timed out in roughly500ms; the information-request step was never reached on those attempts. This was stronger evidence of that failure stage, not the initiating cause. Tests and installed evidence are recorded in [acceptance](acceptance.md). Original incident records below remain historical. No reset, firmware or physical recovery is part of this instrumentation.

## Earlier incident and recovery history

**Later recovery,2026-11-03:** after the user manually clicked Reconnect, they report physical Live working. Root MCP
confirms connected, normalNodes2/problemCodes[0,0] and zero matched descriptor failures, with the same worker generation.
The earlier same-day failed check and historical incident below remain valid observations at their own times. No agent
reset was performed. The user then reports physical Live returning to Embedded on device-page navigation; prepared0.4.0
addresses temporary stream ownership and retirement separately from USB enumeration. Long-run recovery remains open.

Before recovery, Windows showed normal cooler nodes with statusOK/problem0. UC's worker repeatedly opened WinUSB but timed out attempting to write, sending zero bytes in those failed opens. Its first Live attempt at06:40:35UTC already failed; retained logs contain no successful Live connection in that session. A brief ownership/access conflict aligned with original CoolerScreen opening around07:05UTC, then write timeouts resumed. This is not evidence that UC first connected and later froze, nor proof that development work caused the fault.

The old Reconnect path treated any normal USB identity as already connected and returned without resetting, even when firmware writes timed out. The new manual ResetPresent path was dry-run verified against exactly one cooler parent/port/hub interface. After the user approved Windows elevation, one port-cycle request succeeded around10:31:17UTC, but normal enumeration did not return within twenty seconds. Windows subsequently showed descriptor failure/problem43. The reset did not recover the screen. Only a physical USB reconnect/power removal can establish the next recovery result; a port cycle need not remove power. No broad hub reset, PC reboot/sleep, firmware update or unrelated-device reset was performed.

User says the screen USB cable is not accessible now. Physical recovery is deferred; do not keep resetting it or reboot/suspend the PC. 0.2.1 is installed and running, and the real background log confirms the ongoing matched code43 failure. Logging is evidence collection, not a remedy for the physical fault.

## Local diagnostics

Installed0.3.3 distinguishes the engine process from cached connection checks on Home and the cooler page. Accepted polls
update Home text in place. Read failures show unavailable; missing/disabled checks show unknown/off. Worker samples older
than60seconds and Windows USB evidence older than90seconds are labelled out of date. The text identifies a last report,
not a guarantee of current pixels. Windows USB samples have their own `pnpSampledAt` timestamp.

Enabled MCP status exposes the finite cached summary as `cooler.lastHealthSample`; reads do not run a worker RPC, enumerate
USB, write diagnostic samples or request recovery. Current worker state and the older health sample remain separate.

Live MCP verification after installation: worker running/ready, Live disconnected with descriptor_failure; Windows USB
available, simulatedfalse, normalNodes0, problemCodes[43], exactMatchedDescriptorCount1. Worker/USB timestamps were distinct
and recent. This confirms the existing enumeration incident remains; it does not identify the initiating cause or establish
physical recovery. No reset, reboot or sleep was part of this check.

Connection diagnostics is visible on the cooler page, enabled for the user's explicit request and saved as cooler.diagnostics. Turning it off stops sampling and disk writes, including discarding in-flight samples. UC closing to the tray keeps the watcher; Quit stops it.

- Worker health every10seconds: current process generation/restart count, Live's typed connection state, retries, coarse connect/stream/hold/off stage, FPS and RX/status counters. This RPC opens no USB handle and sends no device command.
- Passive Windows ConfigurationManager snapshot every30seconds: expected node count/problem codes and descriptor failures matched to the cooler's remembered parent/location. Identifiers are used only in-memory for attribution and never logged. Unmatched failures are not blamed on the cooler.
- Transitions plus minute snapshots append to `%APPDATA%/UnifiedControl/devices/cooler/connection-health.jsonl`, at most2MiB plus one2MiB rotated `.jsonl.old`. Before first completed migration, status is in memory and no log creates/poisons the future worker-home folder.
- Worker-health RPCs skip occupied command traffic and latch after timeout until a valid reply or new generation. They cannot accumulate pipe writes, mutate failure-uncertainty flags, or leave Quit busy. Native PnP enumeration remains separate from the worker so a worker stall and Windows device failure can be distinguished.

Logged values use a strict whitelist: no raw helper/error text, IPC payloads, account identifiers, quota readings, filenames, media tokens or physical device IDs. Coarse stage/counters cannot prove the exact failed USB command, firmware internal state or visible pixels. Logs provide evidence for diagnosis, not an automatic explanation of cause.

## Navigation and background cost (2026-11-02)

User-reported on installed0.3.4: entering Cooler screen could hold section navigation until failed-device work returned.
Source inspection found the leave handshake awaited transient twin/calibration cleanup even without edits; a frame
whose bridge had not loaded could also wait for the120second handshake deadline. The navigation fix is installed in0.3.5:
real edits/imports remain save-guarded, while transient cleanup is submitted to the parent without awaiting USB-dependent
replies. The embedded view is removed on navigation; no hidden iframe/polling loop is retained. Disable/reset/Quit keep
their full cleanup guard. Connection failure does not itself prohibit moving to another section. Headless checks hold
frame load, startup reads and cleanup while preserving import/failed-save guards. Fresh0.3.5 quiet native checks activate
Home/dock within1500ms while cleanup remains held, keep the dock connected, remove the iframe and cleanly stop the fake
worker/host. This is simulated fault evidence; installed user feel remains open.

Read-only process sampling of installed0.3.4 with the current descriptor43 failure: cooler worker CPU increased by
0.109375seconds over55.02seconds (about0.2% of one logical processor,0.012% of this16-logical-processor machine),
working set34.8MiB/private committed memory47.7MiB. An earlier30second sample rounded worker CPU to0.00% of one processor,
working set34.7MiB. The entire UC process tree used about0.82% total-machine CPU in that earlier window, including dock,
usage-reader Node and WebViews; it does not attribute that total to the cooler. Samples do not measure GPU or establish
healthy Live rendering cost. No installed app/settings/device state was changed to collect them.

The existing connection retry backoff is1,2,4,8,then15seconds, capped at15. Host health polling remains10seconds,
passive PnP30seconds and bounded logs as above. The UI delay and a CPU saturation fault are different claims:
these samples show a navigation wait and low worker CPU in the current failed-device state.

## Manual recovery

Reset cooler USB is explicit. It flushes edits/previews, runs through the cooler's exclusive device scheduler, waits for Windows administrator approval and protects active user work. Normally enumerated recovery requires exactly one physical cooler parent (never an MI interface), valid parent/location and one present hub interface, with a fresh identity check before the sole port-cycle operation. Missing enumeration uses the existing exact remembered-pair selector. No diagnostic timer invokes reset/elevation.

After reset, the engine must answer a real info request. Otherwise the UI reports failure and asks for a physical reconnect of only the screen USB connection. Original CoolerScreen app/data and all dock functionality remain separate. No automatic repeated reset or pump/fan power operation is performed.

## Verification

109 UC core tests and five host tests pass (one helper ignored); four worker tests, ten device-service tests and twenty pure PowerShell selection/effect checks pass. Headless checks cover saved diagnostics and explicit reset routing without OS effects. Quiet native fake IPC verifies log sampling while the editor is closed, opt-out persistence, imports, crash isolation and clean Quit. Synthetic tests cover redaction, rotation, cold-migration ownership and a silent helper's health timeout without busy/pending leaks. Automated tests never touch real hardware; the single manual reset above is recorded separately and failed to recover the device.
