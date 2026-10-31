# Dock MVP preview 0.1.0

Built and software-tested on 2026-10-31. This is an acceptance preview, not a claim that the real dock is verified.
CoolerScreen and Dial are visible future pages; neither engine is integrated. Existing apps and Toolkit plugins are unchanged.

## What works in software

- Profiles/pages auto-save with stable IDs, journal recovery and startup backups. Two chosen faces per key, folder/back and page navigation.
- 196x196 keys and 458x196 wide PNGs: text, fixed icons, bounded PNG/JPEG pictures, clock/date, CPU/RAM and NVIDIA NVML GPU usage.
- All 16 Toolkit usage views. Claude has 15 applicable views; actual daily token counts are Codex-only. Unknown readings never become zero.
- Hotkeys/media/Unicode text, direct app/file/folder/URL launch with literal arguments and preserved Monitor Toggle helpers. Actions execute only from physical key events.
- Explicit dock enable, brightness, exact Studio-process exclusion, native ownership mutex, reconnect and duplicate-press suppression. Tray shows enable and profile/page.
- Usage helper supervision and shutdown. Account reads only run for configured usage faces in an enabled, connected dock profile. Fake mode never activates them.

The native dock engine is currently an owner thread in the host; separate device workers/crash isolation are phase 2 work. Sensors and usage have separate demand controls. GPU support requires the existing NVIDIA NVML driver; absent sources show unknown. Launch scripts and arbitrary shell commands are refused.

## Software evidence

88 core tests and four host tests pass, with one intentional child-test helper ignored. 61 reader tests and 105 unchanged Toolkit tests pass. The pinned driver foundation has 30 passing transport tests. Browser checks cover three widths, undo, imported picture reload, literal arguments, all usage choices, escaped labels, profile conversions and queued save recovery. Quiet debug native IPC exercises real profile/asset files and PNG previews through a simulated dock, Studio block/reconnect, exactly one action per press, disable and inactive account readers. Quit flushes pending picture imports and delayed/coalesced profile writes.

Review regressions: render admission is checked again after slow rendering; expired account images are replaced before delivery; settings save/apply is serialized; quit refusal happens before irreversible controller shutdown. Active monitor recovery finishes before the exit watchdog is armed. Tests never change real monitor/audio/input state.

Live Codex verification on 2026-10-31 ran for 360 seconds: 12 fresh samples and six distinct verified observation timestamps, including an observation after the five-minute cutoff. No model turns, token-history requests or auth-file reads. The owned helper exited cleanly. Claude's existing bridge output was observed separately without cleanup/writes and stayed stale throughout (zero fresh observations); full live Claude helper integration is not established. Its numerical gauges must stay suppressed. Live account switching/logout and physical readability remain acceptance checks. Current Codex metadata cannot uniquely distinguish same-name workspaces; see [usage limits](usage.md).

## Build and install

`npm run release` stages the locked reader dependency, installed Node binary and notices, then builds the per-user NSIS installer under `dist/UnifiedControl_0.1.0_x64-setup.exe`. The bundle includes monitor helpers, the reader and its Node runtime; it does not require Node on PATH after installation. Build staging reads no credentials or application settings. The Node installation used for staging must have its LICENSE file beside node.exe.

Use `tools/install-when-idle.ps1 -Setup <absolute installer path>`; never force-stop the app. Open Unified Control from the Windows Start menu after a fresh install. It starts with dock control off and Windows autostart off. Real dock access starts only when the user enables it; Studio is never closed by UC.

Installed 0.1.0 through that script on 2026-10-31, with the app left closed. Release source: `1b6926e3c322fe046bc87848d9ce5a211907197e`. Installer SHA-256: `F27DA87EA3CB1BDDDEE5CF89E6F1C1FC0D902B4C43F476ADA9088BF95CD79AF4`. The installed reader/resources were checked using `UC_RESOURCE_DIR=<install folder> node scripts/check-bundled-reader.cjs`: bundled Node and rasterizer create a physical-size unknown PNG, preserve inactive provider state and quit cleanly without starting UC or an account reader. Installation rollback and the user-visible Start-menu launch remain acceptance checks.

Packaging prevention: dependency installation happens in the isolated build copy, because running development readers can lock the source native rasterizer. Restart/update helpers wait for the exact parent process to finish action recovery rather than abandoning an update after 30 seconds.

## Hands-on acceptance, with the user present

1. Open UC from Start. Check the editor and set up a small layout, initially with a clock and text A/B key with No action.
2. Quit Ulanzi Studio from its tray yourself, then enable Stream dock in UC. Confirm Connected and correct images. Do not run both apps on the dock together.
3. Check all 13 keys, the wide area, exact A/B flips and folder/back navigation. Configure and check selected hotkey/text/launch/media actions in a suitable target. Monitor Toggle changes real displays and audio routes; test only when ready for that effect.
4. Add Codex/Claude usage faces, confirm fresh/stale/unavailable behavior and compare with account panels. Claude requires a supporting Desktop Code session. Never treat stale historical output as a current quota.
5. Check profile persistence, editor sizes/scaling, unplug/replug, sleep/resume, wide-area clock mitigation, partial-update speed and the planned one-hour hardware stability run. This is normal hardware acceptance; no daily-use trial is required.
6. Verify disable releases the dock and stops demand-driven sources; quit from tray and verify clean exit. Reopen and confirm settings. Keep Studio as fallback until acceptance passes.

Hardware actions, reconnect/sleep, monitor recovery, large-key behavior and installation rollback have not been physically verified by the software test suite. Phase 2 starts after acceptance, then CoolerScreen integration, then Dial.

## Idle-fallback retest

User-reported on 0.1.1: after a dock restart and enabling UC, the physical screen returned to the built-in startup/default display within roughly 10–15 seconds. UC diagnostics showed zero errors/reconnects, continuing ten-second inbound heartbeats and minute clock partial updates after the reported fallback. The physical display itself is not visible to the agent. This does not support calling the USB link disconnected or claiming the ACK patch fixed the issue.

0.1.2 tests a narrow host-idle hypothesis using the existing `0x0006` small-window command, explicit Background mode 2, immediately after the first full page and then every five seconds. The [Bitfocus implementation](https://github.com/bitfocus/companion-surface-ulanzi-stream-controller/blob/d467064ddab2a4508106244c05291931151b88ee/src/device.ts#L85) supplies this cadence; its [mode contract](https://github.com/bitfocus/companion-surface-ulanzi-stream-controller/blob/d467064ddab2a4508106244c05291931151b88ee/src/protocol.ts#L43) uses the earlier wide PNG/SmallViewMode2 baseline already sent by UC. No clock/stats overlay or guessed command is added. Both the reference constructor leaving keepalive off by default and the lack of D200H physical proof limit the conclusion: this is a controlled compatibility trial, not a proven watchdog requirement.

89 core tests plus five host tests pass. Deterministic fake steps at 0/4.999/5/9.999/10 seconds verify cadence, preserved PNG bytes for all thirteen keys and the wide display, no extra full/partial pages, and stop/reset on disable. Diagnostics count successful host keepalive writes; they do not establish firmware display state. The serial worker can be delayed by a blocking render/HID operation, so the cadence is scheduled rather than hard realtime.

Installed update IPC now uses the same stable default identity as the single-instance lock; explicit UC_HOME/debug requests retain isolated channels. The installer refuses to proceed if another UC process exists but cannot be reached through its update guard. This closes the packaged-path admission gap; a running-update check remains separate evidence.

0.1.2 artifact: release source `1322bc2c591ee614393b762dffd4f483e03b3bfd`, installer SHA-256 `BBBEE5605D1AEB979913056DD0C59ECDDEC53336D4C4B0F0708FE539997C3B85`. The quiet native check passed on the rebuilt 0.1.2 debug app. The real 0.1.1 update request used cooperative tray Quit, drained/stopped its dock, ran NSIS with exit 0, then restarted its original packaged-path exe. Startup logs proved that restarted copy was still 0.1.1. The installer script correctly failed its new-version check; root quit that copy through its normal request endpoint (exit 0). Start-menu launch and startup version must be verified before any 0.1.2 physical claim. This is a stale-path handoff during transition, not evidence that keepalive failed.

### 0.1.2 observed result and liveness correction

The user subsequently confirmed Start-menu launch of 0.1.2; startup logs verify source `1322bc2` at 06:51:12 UTC on 2026-10-31 from the ordinary install path. User-reported: about ten minutes of idle display stability, and enabling Stream dock restored the layout from the startup/default screen. Enabling sends a complete baseline; that is useful recovery evidence, not proof that the logo meant USB was disconnected.

Diagnostics after this run showed continuing five-second keepalive writes, zero passive heartbeats and repeated 35-second receive-timeout reconnects after clock/page acknowledgements. Eleven reconnect errors accumulated by 07:02 UTC while the user saw a stable display. Therefore do not mark this a clean idle pass or conclude that 0.1.2 solved all connection behavior. The reason passive heartbeats ceased is not established.

0.1.3 retains Background keepalive but actively checks a silent link: after 25 seconds without valid input, resend the user's already-selected brightness using the existing documented command, once per silent epoch. A real inbound reply renews liveness; write success does not. No reply still reaches the existing 35-second receive deadline and reconnect path. No new device command, brightness setting, monitor/audio effect or image replay is introduced. Physical verification must show actual response-probe traffic without recurrent timeout reconnects before broader action testing.

0.1.3 installed through the corrected script using the reviewed release executable solely for its `--update` request. Logs confirm cooperative quit, dock stop, installer exit 0, and ordinary-path restart at 07:09:58 UTC with source `878c2109dba8300f382f9fee3b2c9c17e474a49e`. Script verified the new process's startup version and PID. Installer SHA-256: `456D1C191E2FE92062AACF5B5260217FA8517F94D73990FFC86617FE668E2F9D`. The update preserved the existing profile and enabled state; no user layout was edited. 92 core and five host tests pass, including actual-RX-only renewal and unanswered-probe timeout. Longer live response-probe evidence is recorded after observation.
