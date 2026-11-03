# Dock acceptance status

## 2026-11-03 cooler workspace return

Installed0.4.0/sourceb18a4a1 passed startup/newPID/MCP0.4/engine4.18.2, expected-only packaged proxy marker change and
unchanged selected profile/page/all14keys. The user confirms physical Live continues after leaving the cooler page and
closing UC to the tray. They then report a pause/reconnection on each return. Retained real logs confirm repeated
`device task: check` → Live pause/Embedded handover → resume/new connection, caused by unconditional iframe startup checks.

Installed0.4.1/sourcebc9a864 reads a fresh worker state after UI hydration and omits that automatic firmware check when saved Live or a
physical preview owns the connection. Initial Live-off checks and explicit Check/Reconnect remain. Actual embedded-UI
regression failed before the fix (one healthy-entry check instead of zero), then passes for repeated entry, startup,
retry, holds, physical preview/tuning, fresh-state changes, unknown state and explicit Check. Quiet native lifetime
checks pass three actual Home/cooler return cycles with no added check tasks/no device hold and continuing frames,
followed by watchdog/tray/off/disable/Quit checks. Broader interface/settings/status/search/editor checks pass.

The user granted standing permission for tested idle-safe UC updates without repeated approval questions, recorded in
CLAUDE.md and the vault.0.4.1 packaging/install passed through the idle-safe updater. Startup/newPID44856, actual
MCP0.4.1, expected-only NSIS marker substitution and unchanged selected profile/page/all14keys verified. Physical reentry
smoothness remains separate from the automated no-check-task tests. Installer SHA256
`3C3E2C82AB0BCE7C2D42EC2ADA216CB34FBA168A9097D3861C35B739559BC0FC`; installed proxy SHA256
`AB9474E7894D5118D2090C5717658CA645371BCA60EB0A34108DB5EA818AEEF7`.

## 2026-11-03 background Live and interface checkpoint

User manually clicked cooler Reconnect and reports physical Live recovered. Root MCP independently verified connected,
normalNodes2/problemCodes[0,0], zero matched descriptor failures and no worker restart. Earlier same-day checks had
confirmed descriptor43 and only user-reported Embedded playback; the manual recovery is later evidence, not proof of
the initiating cause or long-run reliability. Ulanzi Studio currently owns the dock; UC correctly reports it blocked.

User confirms the physical display stops updating/returns to Embedded when leaving Cooler for dock. Prepared0.4.0 claims
saved Live independently of temporary editor streams, clears stale fit ownership after off/on and coordinates old
pipeline retirement with new startup. Twin/fit ownership and deterministic cleanup/retirement tests failed before fixes,
then passed. Four worker tests and14 LiveHost tests pass; one helper is ignored. A debug frame-rate test fails both with
and without the core change under the same load; that pre-existing rate limit is excluded and not claimed green.

Device switches move to General; enabled devices only appear in Home/sidebar/search. Large enable headers are removed,
the cooler fills the right pane, and shell/toolbar styling is refined. Browser checks cover failure/revert, no startup
enablement, selected-provider usage placement, empty devices, three widths, focus, autosave and editor leases.
129 UC core/11 host tests and quiet native profile/MCP/cooler checks pass. Final native lifetime checks verify positive
frame production after actual Cooler-to-Home navigation, beyond the11.5second temporary-stream watchdog and after
close-to-tray, with Live settings unchanged. Explicit Live off/device disable/Quit still pass. Fresh1280px fake native
cooler workspace screenshot was inspected. Release build/package passed from clean sourceb18a4a1; installer SHA256
`A84BE6F858A7F80807C6181C2938AFDB1DBCD2D68A11F653E083F392CAECB115`. Installation approval pending; installed version
remains0.3.5. Physical feel remains separate acceptance. Whole-repository Rust formatting has pre-existing unrelated
differences (UC core/autostart.rs; cooler benches/media_bench.rs); diff and JavaScript syntax checks pass.

## Previous installed checkpoint

Updated2026-11-02. Installed/running0.3.5, code source89fbe5d. User approved the idle-safe update; new startup/PID,
actual MCP version, packaged proxy identity, dock reconnection and unchanged selected profile/page/all14keys verified.
Both usage providers report verified status. Cooler cached health still reports descriptor43; no recovery is claimed.
Codex model-client acceptance passes; Claude acceptance, remaining physical dock/cooler/migration checks and Dial remain
open. Automated navigation failure checks do not establish the user's installed-app feel.

Installed0.3.5 addresses the user's cooler-page navigation freeze: ordinary section exit saves edits/imports but does not
await transient hardware cleanup. Loading bridge exit, held startup/cleanup, idle cleanup suppression, queued/failed
saves and active imports are covered in headless checks. Pending guide/fit timers are cancelled; the embedded iframe is
removed on exit. Independent review found the guide timer race, fixed and regression-tested, then no further issues.
129 core/11 host, full interface, status/search/editor/MCP and quiet native profile/agent/cooler checks pass. The native
cooler check activates Home/dock within1500ms while cleanup is deliberately held, confirms the dock stays connected and
the iframe is absent, and covers immediate cooler-to-General exit. Release packaging and installation verification pass;
user feel remains pending. The held branch uses parent-panel interception because Tauri's invoke property is not writable.
JavaScript syntax and diff checks pass. Whole-repository Rust formatting still reports pre-existing differences
(first:core/autostart.rs); this change modifies no Rust source files and does not reformat unrelated code.

Installer SHA256:`1DC514A4F103B111D965B1A466E633BD37C718331B3DA0ADA611276EDFAAB0AA`.
Installed request proxy SHA256:`C789C41739951A3BCF6BA6AD3CB46725A671DFABEE2DFAD231B0981B283F8FE1`.
The raw release executable matches the installed proxy after only the expected Tauri UNK-to-NSS bundle marker change.
Startup marks89fbe5d+ because acceptance documentation was updated during packaging; the compiled code/assets match
the89fbe5d commit. No source-code modification occurred during the build.

| Check | Status and evidence |
| --- | --- |
| Immediate physical face edits | User confirmed on 0.1.4: edited content appears automatically without an A/B round trip. |
| A/B presses | User tentatively reported the two-press check worked. Broader per-key checks remain pending. |
| Idle display and host liveness | User saw ten idle minutes without fallback on 0.1.2, but logs exposed hidden reconnects. 0.1.3 response probes then ran with zero host errors for hours. This is not a full physical/performance soak pass. |
| USB unplug/replug | User confirmed on 0.1.4: layout returned automatically without toggling Enabled or reopening UC. Logs show disconnect at 12:09:03 UTC, retry backoff and Connected plus a fresh full-page send at 12:09:34. Exact replug time was not captured; do not infer recovery latency from the disconnect timestamp. Retry errors during the intentional unplug are expected. |
| Folder and Back | User retested three physical Test/Back cycles on0.3.2 and reports responsive behavior. UC recorded six completed navigations, zero aborted; the latest host-receipt-to-transfer sample was72.44ms. The capture file missed those presses, so only the latest duration is retained. No before/after speedup or pixel latency is claimed. Virtual following passes native fake checks; user confirmation of following remains separate. [Follow-up backlog](backlog.md). |
| All physical keys and wide area | Remaining per-key coverage and firmware wide-area behavior pending. |
| Hotkey/text/launch/media/Monitor Toggle | Software/fake checks pass; user reported functions mostly work. Monitor Toggle on Key 10 (one Dell target) passed the requested physical off/on test after the path correction, and the user confirms it works after 0.2.3. Explicit audio preservation and other action/monitor combinations remain unverified. |
| Profiles, autosave and restart persistence | User confirmed on 0.1.4: profile, Test page and assignments remained after tray Quit/Start-menu reopen, and UC reconnected. Automated IPC/storage/quit tests also pass. |
| Usage and account states | Both providers reached verified status after0.3.1 and were still verified5min15sec after the first both-fresh sample. Codex briefly became unavailable and recovered automatically; cause unknown, so no uninterrupted reliability claim. Reader stayed ready/dock connected in all sampled checks. Exact account-panel comparison, account-switch/logout and physical gauge acceptance remain pending. |
| Reported AI Usage and Monitor Toggle failures | Monitor Toggle's installed-path failure is fixed in 0.2.3 and user-confirmed working. The0.3.1 reader launch fix is installed; reader now reports ready and both providers reached verified observations through actual MCP. Sustained refresh/physical gauge acceptance are checked separately. [Functional follow-ups](function-failures.md). |
| MCP status and layout configuration | Codex desktop model-client acceptance passed on 2026-11-02: status, scoped read, monitor discovery, preview, temporary inactive-profile apply/readback, stale-revision refusal and undo. All fourteen inactive-profile keys matched the baseline afterward; active Everyday selection and dock connection were preserved. Earlier opt-out and quiet native concurrency/import/Quit tests also pass. Claude client acceptance remains open. [Contract and evidence](agent-control.md). |
| Sleep/resume | Deferred at the user's request; cannot do this now. Remains pending and will be revisited later. |
| Editor usability/scaling | Headless checks at three widths and native profile/MCP checks pass. Installed0.3.2 retains keyboard focus and selected-display context on Undo; removed-page/disabled-control/outside-focus safeguards tested. Full user review pending. Minor preview flicker tracked below. |

## Cooler integration preview

Current recurring-failure incident: initial normal USB nodes had problem0 while writes timed out before any successful Live connection in the retained UC session. One exact user-approved USB port cycle was accepted but failed recovery; post-reset Windows reports matched descriptor failure43. User cannot access the screen USB cable now, so physical reconnect is deferred. 0.2.1 background diagnostics are installed and verified producing non-simulated local records. No initiating cause or recovery pass is claimed. [Evidence and monitoring](cooler-connection-health.md), [issue9](https://github.com/oldbugo/unified-control/issues/9).

Software checks pass for existing embedded themes UI, chunked imports, copied-data migration, saved Live settings, busy-safe Quit and worker crash/restart while dock stays connected. Corrected awaited IPC polling and actual native iframe routing are included in the checks. Physical Live/Embedded, imported user themes/settings and combined-device recovery remain pending. Sleep/resume stays deferred. Original CoolerScreen app/data are retained; it must be closed by the user before enabling the cooler engine in UC. No full phase2 acceptance or old-app replacement is claimed. See [architecture](device-app/architecture.md).

0.2.0 installer SHA-256: `917B7626FE2CE93B646F097A9DF983BCD15C57699644AE88CCFF51BBE635E693`. Running source `9b9ffca`; engine pin `58cb2da`. Startup 05:17:43 UTC, dock Connected and initial full page with zero errors; no cooler worker starts while disabled.

0.2.1 installer SHA-256: `2FB3B8C38011334D3643E0BF8FCD5E238FB34BD4CE7F578172FDE88DA22AFAB5`. Running source `f079a96`; engine pin `88cd041`. Startup 10:50:10 UTC/new PID verified; dock Connected with fresh full page and zero errors. First health transition 10:50:20 UTC reports descriptor_failure, normalNodes0, exactMatchedDescriptorCount1 and problemCodes[43], simulatedfalse. The first installer attempt refused busy work; a later idle-safe attempt succeeded without force quit.

## Nonblocking UI bug

[Preview flicker — issue #3](https://github.com/oldbugo/unified-control/issues/3): user saw a light flicker every few seconds in UC's software preview on 0.1.4. Physical edit refresh is confirmed working. Source inspection found every preview image was replaced before decoding on every polling tick, even for byte-identical frames. Version 0.1.5 retains unchanged images and decodes changed frames before replacement; stale/detached results are discarded. Browser and quiet native checks pass; installed user-visible confirmation is pending.

The 0.1.5 build includes `KeyPreviewRenderer`: unchanged image nodes are retained, replacements decode first and stale/detached results are rejected while temporary URLs are released. The headless suite passes unchanged-image/URL/decode-count checks, a delayed replacement retaining all fourteen old images, and late results after navigation. Issue remains open until installed user-visible verification.

Additional user feedback: typing display text can flash the preview at an apparently smaller size. The native input handler rebuilds HTML fallback content before requesting PNGs; issue #3 now includes that path. Navigation performance and virtual-page following are tracked separately in the [backlog](backlog.md).

The 0.1.5 typing fix keeps decoded key nodes, requests updated PNGs and aligns the preview face. The headless regression delays decoding during rapid typing, checks latest text persistence/render requests, preserves old image nodes and confirms the latest decoded frame wins. Virtual page following is included in the same build; physical confirmation is pending.
