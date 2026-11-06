# Device acceptance evidence

Current checkpoint: installed0.9.0/engine4.21.0 with software-tested cross-device controls. Dock/usage checks pass; cooler Live has an unresolved USB write_timeout, also reproduced on0.8.0. Physical cooler acceptance is blocked.
[Project status and remaining work](project-status.md) is the current task list. Dated sections below are historical evidence,
and older unresolved/approval-pending statements are superseded by later results.

## 2026-11-06 precise cooler connection diagnostics: software gate

Prepared0.9.1 pins engine4.21.1/source77c9086.206 UC core tests pass (three existing ignored),11 host tests, eight engine session/trace tests and ten worker/source cases. New tests cover each failed handshake stage, all existing cancellation points, exact command/wait/timeouts, continued stop attempts after errors, finite redaction/bounds, queue saturation/drain, host-only event routing, opt-out/migration and shared log rotation. Independent review found no command/timing/error/privacy regression. The250ms bound covers trace draining; existing stdout protocol replies can still wait on a stalled parent. Close/ok means the existing void close returned, not independently verified USB release.

Quiet fake native cooler fixtureMzzouL and lifetimeOQoaoQ pass: actual worker IPC, background sampling/opt-out, imports/busy-safe Quit, crash/restart isolation and Live surviving page/tray transitions. No real USB/reset/firmware effects or user data are used. Bundling/installed verification pending; current0.9.0 remains running with the known write_timeout. Instrumentation does not itself recover the device.

## 2026-11-06 cross-device controls: software gate

Installed0.9.0 pins engine4.21.0/sourceda2cb2c.202 UC core/11 host/62 reader tests pass (three existing ignored checks),
plus cooler five dock-control/19 scheduler/nine private-worker cases. Full headless editor and cooler checks pass.
Native cross-device fixtureW3tNZb verifies fake physical Button→dock→host→cooler actions, deduplicated presses,
selection/cycle/Live/brightness with iframe title/switch/brightness readout updates, unchanged Embedded, and no device
auto-enablement. Admitted host action busy survives dock loss and refuses Quit/both disables; dock/cooler replacements
never replay. Raw wrong-ID completion is ignored, correct completion bypasses draining Shutdown, and EOF cancels waiters.
Existing profile/MCP/dock-worker/cooler/cooler-lifetime/migration/shared-provider/hardware native gates pass. These use synthetic sources,
throwaway themes and fake devices; actual visual switching/brightness and physical coverage remain user acceptance.

Independent reviews checked strict schema/catalog bounds, atomic target admission, original-session correlation, action
busy across crashes and shutdown, scheduler/Live USB handoff, fixed documented brightness operation, fresh relative
baselines and distinct requested/reported readback. Bounded completion enqueue prevents a stalled original channel from
holding a completed target action forever. Missing/mismatched brightness readback emits unconfirmed state without retry.
Headless testing caught detached numeric input modifying another operation; current-element/operation guards fix it.
New/worker Rust formatting and whitespace pass; broad repository formatting retains unrelated existing differences.
Installed0.9.0/source7c3ee6d, engine4.21.0/sourceda2cb2c, passed quiet startup/new-process, actual MCP version, connected dock, both verified AI providers and unchanged selected fourteen-key layout checks. Cooler Live remains disconnected with write_timeout. A quiet comparison install of0.8.0/engine4.20.0 reproduced the same timeout;0.9.0 was restored. The initiating cause is unknown. Windows reports two normal USB nodes/problem0; no resets, elevation, firmware or actual cooler actions were performed. Physical switching/brightness acceptance remains open.

Release source7c3ee6daaac24f90e85d3ae7359fe392fae5d5cb is preserved byv0.9.0; private PR29 merged as c88530e86745fd65870c07f4ecf6b4e8da17e737. Engine source/tagda2cb2c61965058604430a5153c0eb0975c6cbaa/v4.21.0 and PR6/mainf7642fd4a1dfd8e258e045901617b6bb664bad55 are remotely verified. Installer SHA256:30133DB008B3D3EC803D656047D9B12796C87F5026887F89F72F2C9E35E04DEA. Bundled inactive reader smoke passes. Installed request proxy matches the build except the expected three-byte NSIS marker substitution; SHA256:C51DF832F527D7B102DE98F853C152A4FD3FE1C5AD7DEE810E89F8A8168B15C9.

## 2026-11-05 remaining shared services: software gate

Installed0.8.0/source5dfec54 pins cooler engine4.20.0/source947aad3.193 UC core/11 host/62 reader tests pass, with three existing ignored
UC checks. Five cooler shared receiver cases (isolated renderer helper invoked), seven music, four weather and nine
worker/source cases pass. Headless interface/cooler and native profile/MCP/shared-services fixtures pass.
The native shared-services fixture proves two selected account providers/five deduplicated A/B views with no local account
reader; no cold/no-widget/no-place source work; one provider-only process; idle native/HTTP counters unchanged and same-place
weather cache reuse. Error/stale/logout/unverified modes produce unknown AI pixels. Separate delivery pauses expire each
consumer, source/worker crashes retain peer ownership, matching-place weather is explicitly stale on failure and changed
places cannot inherit old data. Quit ends owned helpers. Synthetic sources and fake devices only.

Independent review corrected mixed-account frame coherence, original source/transport deadlines, preferred-player epoch
binding, metadata bounds, strong artwork identity, restarted ID uniqueness, exact DTO parity, bounded helper backoff,
fail-closed source retirement and inactive weather-cache retention. Native transport stalls use bounded ephemeral writes;
mutation admission refuses a held pipe without durable uncertainty. Private source exchanges reject editor injection.
New modules/worker formatting and diff whitespace pass. Whole-repository formatting retains existing unrelated differences.
The original hardware/dock-worker/cooler native checks also pass. One lifecycle app-state poll failed with an opaque error; the unchanged-binary retry passed, so the cause remains unconfirmed. The fixture now preserves finite error detail. Normal pipe admission separately waits through brief contention for at most250ms, with short-contention and stalled-writer regressions. Final affected shared-services/lifecycle reruns pass. Quiet idle-safe install verified new startup/process tree, actual MCP0.8.0/engine4.20.0, connected dock/cooler Live without holds/errors and the exact selected fourteen-key layout. One actual Node account helper and one provider-only process are parented by the host. Initial status: two verified AI providers, one reader start/source epoch, one provider process/music watcher, one weather fetch, no source failures. Hardware reads17 per family at1Hz. No actual source demand was activated for verification; existing configured widgets supplied demand.
The installed proxy differs from the built binary only by the expected three-byte UNK→NSS bundle marker; both are5543424bytes.
Installer SHA256 `456DF885F95091E592A5A968FB6A82080570FC9D5E5677B10492A5D09244811A`; proxy SHA256
`E726752B37C6C31772B4E03C418B959D5847FCDE57AC8575A1E5E75EEA156E40`. Standalone installation/data untouched.
At331seconds after the initial installed sample, both AI providers remain verified, readerStarts/sourceEpochs/processStarts/musicStarts remain1, weatherFetches remains1, and no source failures are reported. The same owner/process generations remain; AI renders36to676, provider exchanges17to330, hardware reads17to347 per family. Dock/cooler Live remain connected without holds/errors, and the selected fourteen-key layout is unchanged. This is bounded refresh evidence beyond the five-minute cutoff, not uninterrupted reliability or physical readability acceptance.

## 2026-11-05 shared hardware

Installed0.7.0/sourceb5b0140, pinning engine4.19.0/source88aad7d.179 UC core/11 host/62 reader tests pass (three existing
ignored UC checks), plus seven focused cooler shared cases/four worker cases. The isolated cooler renderer helper is
actually invoked by its parent test. Quiet native shared/profile/MCP/worker/cooler/lifetime and browser interface/cooler/
editor suites pass. Final shared fixture proves one collector, union rates, zero replaced local polls, full GPU fields,
hidden-page release, provider failure/paused expiry/recovery, independent peer delivery, both-way crashes and safe Quit.

Independent reviews corrected schema bounds/availability parity, snapshot-key capture, generation/PID cache scoping,
original expiry through cached flags and sensor pixels, actual counter retention, exact-ID probe gates and source-lock
isolation. Worker/collector formatting and diff whitespace pass; broad formatting retains existing unrelated differences.

The quiet idle-safe update verified a new host and separate dock child, actual MCP0.7.0/engine4.19.0, dock connected and
cooler Live connected/no holds/errors, with the exact pre-update selected profile/page/all fourteen keys preserved.
Initial real collector status reports1Hz cooler-only demand,14 CPU/14 RAM/14 GPU reads across14 samples, zero source
failures and GPU active. Both AI providers verified initially and again339seconds later; collector generation was unchanged and each source advanced from14 to352 reads across352 samples with zero failures. Dock/cooler Live remained connected without holds/errors and the selected fourteen-key layout stayed identical. This bounded sample passes the five-minute freshness cutoff; it does not establish uninterrupted reliability. This is installed source/counter evidence, not physical
pixel/readability or long-run reliability acceptance. No standalone CoolerScreen app or rollback data was replaced.
The installed proxy matches the built binary after only the expected three-byte UNK→NSS bundle marker substitution.
Installer SHA256 `6BEBB727410BE366B5E2593A48CBD6F8A6517F78BE500DE7FA7E32AE0A0DD192`; proxy SHA256
`49BFEB2AEB5F3519ECB806764EBF74EA62344F98802DC2CC16FD584D922F2360`.

## 2026-11-04 dock-worker software gate

Version0.6.0 implements plan2.4.165 core/11 host tests pass (three pre-existing ignored checks), plus62 reader tests.
Quiet actual-process fixtures verify distinct host/dock/cooler processes, both-way crash isolation, cooler Live continuity,
fresh latest-page recovery without action replay, host-only profile files, active-action Quit/disable refusal and stdin-EOF
draining. Native profile tests retain delivered Folder/Back following, imports, A/B persistence and queued-save/import
tray Quit. Final native MCP, cooler and background-Live/lifetime suites pass. Browser interface/editor/settings/status/
search/entry checks pass. Independent source review found no remaining actionable issue in the reviewed scope.

Validation exposed and corrected preview deadlines across IPC, launch stdio inheritance, stale/uncertain/generation
handling, profile retry/application order and cached render-busy refusal of tray Quit. Test fixtures now wait for completed
saves and retry only explicit transient idle-admission refusals; active-action refusal assertions remain unchanged.
Whole-repository formatting still reports pre-existing unrelated differences; worker modules and diff whitespace pass.
These are simulated device/account checks, not new physical-action or live-account acceptance.
The idle-safe update installed clean source0117a5f at07:47:04UTC (18:47:04AEDT), with startup/new host PID10412 and
separate child PID51196/parent10412. Actual MCP reports0.6.0, dock connected and cooler Live connected/no holds/errors.
The selected profile/page/all fourteen keys exactly match the immediate pre-install snapshot. The installed request proxy
matches the built executable after only Tauri's expected three-byte `UNK`→`NSS` marker substitution. Installer SHA256
`F7145D51190FEAD496782B5F268AE0A206FB0522EDBFAFA03B697F939689B3E1`; proxy SHA256
`28F27E91FBCBE2609D339A16073171AF7E47BABBDC686934F8CFD1974E7D1491`.
The cooler workspace entry delay is separately tracked in [its evidence note](cooler-entry-delay.md).

## 2026-11-04 cooler data migration

Installed0.5.1 adds V2 source-definition ownership and a host-side cancellable comparison. Original settings, library,
gallery and lyrics offsets are retained; newer UC edits are never overwritten. General shows a historical verification
summary and explicit button; MCP exposes only cached counts/flags. Pending receipts invalidate stale success on cancelled,
failed or interrupted checks. [Contract and rollback limits](device-app/cooler-migration.md).

Software checks:155 core/11 host tests pass (three pre-existing ignored checks), focused streaming cancellation/receipt/
lyrics/migration cases, headless interface/cooler/settings suites and quiet native migration/cooler/lifetime/MCP checks.
Native migration exercises the actual General command permission and receipt; the fake worker deliberately uses NoSources,
so that fixture verifies copied definitions but does not claim actual catalog discovery. A worker launch-contract test
verifies the explicit owned Sources environment; the pinned engine's real catalog consumes that override.
Independent review found cancellation and evicted-job status problems; both were corrected and reviewed again.
Touched Rust files pass rustfmt; whole-repo fmt remains blocked by existing unrelated formatting differences.
0.5.0/sourcebeb3888 installed through the quiet idle-safe updater; startup/newPID39464 and actualMCP0.5 verified.
The app-owned comparison found286 library files,22 byte-identical gallery files, no missing files/old-root/missing
references, unchanged settings/companion and owned empty Sources. It flagged two generated preview images alongside
one edited UC theme. Engine writes confirm theme-card.png/theme-preview.png are derived.0.5.1 counts exactly these
three direct theme preview filenames separately, preserving strict Gallery/nested/original media checks. Focused tests,
independent review and native General recheck with a changed preview pass. No real data is overwritten for a pass.
Selected profile/page/all14keys and inventory remain unchanged (revision changes on restart). Cooler Live reconnected
with normal USB/no holds/errors; dock correctly remains blocked while Studio runs. Final0.5.1/sourceb67d928 installed through the idle-safe updater; startup/newPID, actualMCP0.5.1 and expected-only
three-byte NSIS marker difference in the installed request proxy verified. App-owned comparison at2026-11-04T00:50:41.939Z
passes:286 library files,22 originals,6 themes,27 versions;22 byte-identical gallery files;23 active references; zero
missing files, changed original media, old-root dependencies, missing references, unreadable records or unstable files.
It preserves one UC theme record change and two generated preview changes. Settings/companion match, no lyrics offsets
exist in either copy, and the owned Sources folder has zero imported definitions. The user answered “Yes, everything
looks right” when asked about existing UC themes/settings and physical Live updating. This is current-setup continuity,
not every original phase2 release gate. Live reports connected/no holds/no errors and normal USB; selected profile/page/
all14keys and inventory remain unchanged. Standalone installation/data remain as rollback; installer-lineage replacement,
broader architecture and release checks remain open. Installer SHA256
`92BCF727195BDDF66B9D74046CE6F8EB3B94015C1D64A5557B616ADAD466A803`; request proxy SHA256
`6B8195722D191352FC55A7175C2BB7A10966F4D608D9B5E30D7DA7E289C9A690`.

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

## Historical installed checkpoint:0.3.5

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

## Historical cooler integration preview:0.2.x

The following incident describes2026-11-01. Later manual recovery, physical Live confirmation and0.5.1 real-data
verification are recorded above; this is not the current USB or migration state.

Current recurring-failure incident: initial normal USB nodes had problem0 while writes timed out before any successful Live connection in the retained UC session. One exact user-approved USB port cycle was accepted but failed recovery; post-reset Windows reports matched descriptor failure43. User cannot access the screen USB cable now, so physical reconnect is deferred. 0.2.1 background diagnostics are installed and verified producing non-simulated local records. No initiating cause or recovery pass is claimed. [Evidence and monitoring](cooler-connection-health.md), [issue9](https://github.com/oldbugo/unified-control/issues/9).

Software checks pass for existing embedded themes UI, chunked imports, copied-data migration, saved Live settings, busy-safe Quit and worker crash/restart while dock stays connected. Corrected awaited IPC polling and actual native iframe routing are included in the checks. Physical Live/Embedded, imported user themes/settings and combined-device recovery remain pending. Sleep/resume stays deferred. Original CoolerScreen app/data are retained; it must be closed by the user before enabling the cooler engine in UC. No full phase2 acceptance or old-app replacement is claimed. See [architecture](device-app/architecture.md).

0.2.0 installer SHA-256: `917B7626FE2CE93B646F097A9DF983BCD15C57699644AE88CCFF51BBE635E693`. Running source `9b9ffca`; engine pin `58cb2da`. Startup 05:17:43 UTC, dock Connected and initial full page with zero errors; no cooler worker starts while disabled.

0.2.1 installer SHA-256: `2FB3B8C38011334D3643E0BF8FCD5E238FB34BD4CE7F578172FDE88DA22AFAB5`. Running source `f079a96`; engine pin `88cd041`. Startup 10:50:10 UTC/new PID verified; dock Connected with fresh full page and zero errors. First health transition 10:50:20 UTC reports descriptor_failure, normalNodes0, exactMatchedDescriptorCount1 and problemCodes[43], simulatedfalse. The first installer attempt refused busy work; a later idle-safe attempt succeeded without force quit.

## Nonblocking UI bug

[Preview flicker — issue #3](https://github.com/oldbugo/unified-control/issues/3): user saw a light flicker every few seconds in UC's software preview on 0.1.4. Physical edit refresh is confirmed working. Source inspection found every preview image was replaced before decoding on every polling tick, even for byte-identical frames. Version 0.1.5 retains unchanged images and decodes changed frames before replacement; stale/detached results are discarded. Browser and quiet native checks pass; installed user-visible confirmation is pending.

The 0.1.5 build includes `KeyPreviewRenderer`: unchanged image nodes are retained, replacements decode first and stale/detached results are rejected while temporary URLs are released. The headless suite passes unchanged-image/URL/decode-count checks, a delayed replacement retaining all fourteen old images, and late results after navigation. Issue remains open until installed user-visible verification.

Additional user feedback: typing display text can flash the preview at an apparently smaller size. The native input handler rebuilds HTML fallback content before requesting PNGs; issue #3 now includes that path. Navigation performance and virtual-page following are tracked separately in the [backlog](backlog.md).

The 0.1.5 typing fix keeps decoded key nodes, requests updated PNGs and aligns the preview face. The headless regression delays decoding during rapid typing, checks latest text persistence/render requests, preserves old image nodes and confirms the latest decoded frame wins. Virtual page following is included in the same build; physical confirmation is pending.
