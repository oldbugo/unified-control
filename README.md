# Unified Control

One Windows tray app for the user's devices: the Ulanzi D200H stream dock first, later the CoolerScreen (Lovingcool SC70) and
the Ulanzi Dial. Private. Combined dock/cooler integration preview with independent enable controls.
Physical cooler acceptance and remaining dock checks are tracked separately from data migration. Rules: `AGENTS.md` and `CLAUDE.md`.
Development resumed by the user on2026-11-04. [Current handoff](docs/handoff-current.md) · [Scope, goals and ordered next work](docs/project-status.md).
Current user-confirmed results and remaining checks: [dock acceptance](docs/acceptance.md).
UI and navigation follow-ups: [backlog](docs/backlog.md).

Version0.7.0 implements shared hardware readings (plan2.6a). One demand-driven CPU/RAM/NVIDIA collector serves dock and
cooler displays, including the cooler's temperature, clock, memory, power and fan details. Demand expires when unused;
both former cooler aggregate readers and the dock's local reader stay off in shared mode. Readings retain their original
expiry through worker delivery and rendering. [Contract and software checks](docs/device-app/shared-hardware-protocol.md).
Installed0.7.0/sourceb5b0140 passed the quiet idle-safe update, startup/newPID, actual MCP, engine4.19.0, device reconnect
and unchanged fourteen-key layout checks. The real shared collector reports1Hz cooler demand, equal CPU/RAM/GPU reads
and no source failures.179 core/11 host/62 reader cases and relevant headless/quiet native suites pass.
Other shared providers remain the next slice.

Version0.6.0 separates the dock into a supervised worker. UC retains durable profile storage and editor/MCP coordination;
the child owns transport, rendering, A/B/folder runtime, actions and usage readers. Editor previews share that reader.
Crash recovery uses the latest saved layout without replaying actions. Expiry guards cross IPC, and quiescence preserves
active action recovery during disable/Quit. [Worker contract](docs/device-app/dock-worker.md).
The separately reported [cooler workspace entry delay](docs/cooler-entry-delay.md) remains open; no retention timer was added.
Installed0.6.0/source0117a5f passed the quiet idle-safe update, startup/newPID, actual MCP, expected-only packaged proxy
marker change and device reconnection checks. The selected page/all fourteen keys are preserved.165 core/11 host,
62 reader tests, relevant browser checks and quiet native profile/MCP/dock-worker/cooler/lifetime suites pass.

Version0.5.1/sourceb67d928 completes the independent cooler data migration, including UC-owned custom source definitions.
UC's own comparison verifies286 library files (22 originals,6 themes,27 versions),22 byte-identical gallery files and23
active references, with no missing media or old-folder dependencies. One UC theme edit and two regenerated previews are
preserved. Settings/companion match; optional lyrics offsets and custom source definitions are absent in this setup.
The user confirms existing themes/settings look right and physical Live updates. Quiet install/startup/newPID/MCP,
packaged proxy, cooler reconnection and unchanged selected dock page/all14keys verified. General offers a timestamped
verification result and explicit recheck; normal status reads never scan media.
[Migration behavior and remaining installer-lineage work](docs/device-app/cooler-migration.md).

## Earlier checkpoints (historical)

Installed0.4.1/sourcebc9a864 removes the automatic firmware check on cooler workspace entry while Live owns the connection, avoiding
the user-confirmed pause/reconnect on each return. Explicit Check/Reconnect and initial checks with Live off remain.
Quiet idle-safe update, startup/newPID, actual MCP0.4.1, packaged proxy and unchanged selected14-key layout verified.

Installed0.4.0 moves device enablement into General, shows enabled devices only in Home/sidebar/search and removes the
large workspace enable banners. Cooler controls fill the right pane, with coordinated toolbar styling. Refined navigation,
cards, switches, settings, typography and spacing retain the existing autosave/editor safeguards. Background lifetime
fixes claim saved Live independently of temporary editor streams and prevent delayed preview cleanup from stopping it.
All targeted browser/native integration checks and release packaging pass. Startup/newPID, actual MCP0.4/engine4.18.2,
packaged proxy identity and unchanged14-key layout verified. The user confirms physical background playback away from
the cooler page and in the tray. Standing permission covers future tested idle-safe UC updates without asking again.

Installed0.3.5/source89fbe5d fixes leaving Cooler screen while its device is unavailable or its embedded page is loading. Saved edits
and imports remain guarded; transient screen-preview cleanup no longer blocks section navigation, and the iframe is
removed rather than retained in the background. Disable/reset/Quit retain full cleanup. User-approved idle-safe update,
startup/newPID, actual MCP0.3.5, packaged proxy identity, dock/provider reconnection and unchanged14-key layout verified.
User feel remains open. [Background cost and evidence](docs/cooler-connection-health.md).

Installed0.3.4/source95a68ee adds the planned sidebar page search, including related terms, keyboard activation, Escape
to clear and compact-sidebar access. Type usage, themes or MCP in Find a page; Enter opens a result through normal guarded
navigation. Filtering does not change settings or layouts. Startup/newPID, MCP version, corrected packaged proxy,
dock/provider recovery and unchanged active-page keys were verified.

Installed0.3.3/source da70551 fixes stale Home device cards and separates the cooler engine from its last connection report. Enabled MCP
status gains a strict cached-health summary with separate worker/USB timestamps. 129 core/11 host tests, headless status
and interface suites, editor leases and quiet native dock/MCP/cooler checks pass. Startup/new PID, MCP version, packaged
proxy identity, dock/provider recovery and unchanged active-page keys were verified. Cached health confirms the current
cooler USB descriptor failure43 while its engine runs; no recovery is claimed.

Installed 0.3.2/source29414fb keeps keyboard focus during dock editing and retains the selected display on Undo when its
page survives. It includes bounded numerical navigation timings in MCP/dock status and guarded Codex error/stale reasons.
The idle-safe update verified startup/new PID, MCP version, dock reconnection, both providers verified and an unchanged
fourteen-key active page. The user reports three physical Test/Back cycles feel responsive; UC recorded six completed
navigations and a latest host-receipt-to-transfer time of72.44ms. No speedup or physical pixel latency is claimed.
[Navigation measurement guide](docs/navigation-timing.md).

Version 0.3.1/sourcee23be94 fixed a reproduced AI Usage startup fault: Node rejects canonical Windows script paths.
Trusted filesystem paths are normalized at launch. The failing canonical-path fixture now passes, along with 120 core
and 11 host tests. The user approved the idle-safe update on 2026-11-02; startup/new PID, MCP version0.3.1, dedicated proxy
hash and dock reconnection were verified. Both Codex and Claude subsequently reported verified observations through MCP.
Both providers remained fresh beyond the five-minute cutoff in a bounded MCP watch. Codex briefly became unavailable
and recovered automatically; its cause remains unknown. This is not an uninterrupted reliability pass. Physical usage,
exact account-panel comparison and account-switch acceptance remain open. Claude needs a supporting Desktop Code companion.

Included in 0.3.2: bounded numerical navigation timing in dock/MCP status, fake held-render and ACK-burst regressions,
and preservation of guarded Codex error/stale reasons. 123 core/11 host and 62 reader tests pass.
Scheduling is unchanged; physical latency and any speedup remain unmeasured.

Headless keyboard/Tab/context checks cover three widths; native profile and MCP integration checks pass with fake devices.

## Earlier direction and implementation history

User direction on 2026-11-02: add an agent-agnostic MCP interface. The 0.3.0 checkpoint provides seven tools for status,
scoped layouts, monitor discovery, preview, revision-checked apply, last-agent-write undo and job status. It uses the running
tray app through same-user Windows IPC; it never edits app-data files from a client or starts a second device engine.
Local agent access is visible in General and off by default. [Setup, tools and evidence](docs/agent-control.md).
Installed 0.3.0/source `0909429` is verified through startup and a direct installed-MCP connection. Codex and both Claude
client configurations are registered. The user enabled Local agent access; direct production MCP reads the 14-key layout and
connected dock without changes. Reload the client's MCP configuration/start a fresh session to discover its tools.
Fresh Codex desktop model-client acceptance passed on 2026-11-02: status/layout/monitor discovery, preview, a temporary
inactive-profile write/readback, stale-revision refusal and undo. The inactive profile's keys were restored exactly;
the active Everyday page stayed selected. Claude model-client acceptance remains open.
The dedicated request proxy avoids a packaged client's stale main executable. The completed stack is consolidated onto
private `main`; `v0.3.0` preserves the installed build source. See the handoff before new development.

Build the combined interface from the start, with Home, Cooler screen, Stream dock, Dial, General and About pages.
Implement working dock support first, run normal functional and real-device acceptance tests, then integrate CoolerScreen,
then add the dial. A week-long daily-use trial is not required. The existing CoolerScreen installation stays separate
until its integration is built and tested.

User direction on 2026-11-02: prioritize Monitor Toggle for immediate use. The live picker failure was traced to a canonical
Windows resource path rejected by PowerShell 5.1's path provider. 0.2.3 normalizes filesystem paths at the adapter boundary;
the original helper is unchanged. A temporary installed-helper correction restored discovery, and the user confirmed the
physical Dell toggle worked on Key 10. The permanent adapter passes the canonical-path read-only regression; 0.2.3 was
installed with startup and original helper hashes verified. The user subsequently confirmed Monitor Toggle works.
[Evidence and limits](docs/function-failures.md).

User direction on 2026-11-01: integrate CoolerScreen now, then refine the UI. Preview 0.2.0 reuses the existing cooler
interface and engine in a supervised worker. First enable copies settings/library/gallery into UC, preserving the original
fallback; close the separate CoolerScreen app yourself before enabling. No uninstall or automatic device enablement.
[Architecture, migration and limits](docs/device-app/architecture.md). AI Usage remains unresolved; Monitor Toggle is now
user-confirmed working. [Functional follow-ups](docs/function-failures.md). Sleep/resume stays deferred.

Installed 0.2.1 adds the requested [background connection diagnostics](docs/cooler-connection-health.md), with a saved
Connection diagnostics switch on the cooler page. It records bounded local evidence while UC stays in the tray; it never
resets devices automatically. One approved targeted reset did not recover this incident. Physical screen USB access is
unavailable now; Windows still reports matched descriptor failure43. The installed, non-simulated health log is recording it.

Software checked for 0.2.0: 103 core tests (one helper ignored), five host tests, three cooler-worker tests, full headless
interface and cooler bridge checks, plus quiet native fake IPC. The native integration verifies the embedded themes UI,
chunked imports, busy-safe Quit, saved Live settings, cooler worker crash/restart while the dock stays connected and clean
disable/Quit. Corrected native-profile polling replaces earlier async state waits that could return before their condition
was true. Physical behavior and real user-data migration still require confirmation. UI polish follows integration.

## GitHub layout

Unified Control is one product maintained across separate private repositories. The agreed grouping is the shared
`unified-control` GitHub topic and a `UC:` description prefix, under the existing `oldbugo` account.

| Repository | Responsibility |
| --- | --- |
| [unified-control](https://github.com/oldbugo/unified-control) | Combined app, interface, profiles, actions, shared services and installer |
| [uc-ulanzi](https://github.com/oldbugo/uc-ulanzi) | Ulanzi dock hardware engine and fake dock; later the dial |
| [coolerscreen](https://github.com/oldbugo/coolerscreen) | CoolerScreen app and reusable cooler engine; keeps its own version history |
| [ulanzi-toolkit](https://github.com/oldbugo/ulanzi-toolkit) | Existing Studio plugins, including usage readers and Monitor Toggle |

The app now consumes `ulanzi_dock` through a Cargo Git dependency pinned to reviewed source commit `9ea93c0` in
`uc-ulanzi`. This development pin is immutable; tagged dependencies follow when releases are made. The profile/render
pipeline is tested through the simulated dock; native HID opens only after the user enables the dock and Studio is absent. CoolerScreen worker integration is built in the 0.2.0 preview. Reviewed work is committed and pushed
to the relevant private repository as work proceeds. A feature branch on GitHub does not imply a merged or installed release.

## Current implementation

The native editor saves validated profiles and pages in the UC app-data folder, with stable IDs, a startup backup and
journaled recovery. Changes save as made; errors remain visible and retryable. Browser-only previews keep a separate
local draft. They are not silently imported into native profiles. [Storage contract](docs/profiles.md).

Built in the 0.1.0 preview: six-page editor, native profile storage, strict A/B routing, navigation, undo, searchable actions,
bounded PNG/JPEG imports, 13 fixed icons, text/clock/date and on-demand CPU/RAM/NVIDIA GPU faces. Hotkey, media, Unicode text,
direct launch/URL and preserved Monitor Toggle adapters are wired to physical presses. Usage readers render all 16 views
through a managed Node helper; account provenance/freshness gates run before publication, rendering, receipt and delivery.
The dock remains disabled until explicitly enabled. Studio detection, ownership, reconnect, brightness and tray controls
are implemented. A fresh render replaces cached-image replay after reconnect. [MVP evidence and acceptance](docs/dock-mvp.md).

Checked: 97 UC library tests pass (one helper ignored), five app lifecycle tests, 61 reader tests and the original 105
Toolkit tests pass. The separate driver has 30 passing fake-transport tests from the foundation checkpoint. Browser checks
cover undo, picture persistence, all usage choices, literal launch arguments and three window widths. A quiet native WebView
check covers real IPC/files, image import, A/B persistence, physical-size PNGs, fake Studio block/reconnect, duplicate press
suppression and disable. Fake mode never starts real account readers, sensors or Windows actions. Native Quit also waits
for delayed image import and queued saves. Automated tests never open the physical dock or send input/audio/display actions.
Live Codex refreshed autonomously beyond five minutes in a six-minute check; Claude's existing source remained stale and
must show unknown. Earlier preview 0.2.1/source `f079a96` was installed through cooperative idle-safe update; new PID and
startup version were verified at 10:50:10 UTC on 2026-11-01. Dock reconnected with a fresh full page and zero errors. Cooler
is enabled but fails USB enumeration; background health records code43 with one exact-matched descriptor failure and no
normal nodes. A USB port-cycle request did not recover it; physical reconnect is deferred because its cable is inaccessible.
The request-only executable override avoids the packaged shell's stale installed-file view.
Installed reader/resources pass an account-inactive PNG/clean-shutdown check. Physical acceptance is pending.
Initial hands-on feedback: actions mostly work, but the physical dock falls back to its built-in startup/default screen.
UC was observed still reporting Connected. The 0.1.1 update tolerates nonstandard acknowledgement lengths and adds
sanitized connection/heartbeat/frame diagnostics. The physical failure cause and recovery remain unverified.
The 0.1.1 idle retest failed within 10–15 seconds while heartbeats and minute clock partial updates continued with
zero errors/reconnects. Version 0.1.2 adds a known Background-mode keepalive after the first full page and every five
seconds. A deterministic fake test confirms the cadence, stop-on-disable, unchanged key/wide images and no page resends.
User-reported: the layout stayed visible for about ten idle minutes on 0.1.2, and enabling restored it from the default
screen. Diagnostics qualify that result: keepalives continued but passive heartbeats stopped, causing receive-timeout
reconnects roughly every minute. This is visible idle stability, not a clean connection pass. Version 0.1.3 adds a bounded
brightness-response probe before the receive timeout while retaining failure/reconnect when no response arrives.
See [idle-fallback evidence](docs/dock-mvp.md#idle-fallback-retest). Broader physical acceptance remains pending.
The first 100 seconds of 0.1.3 live diagnostics show three acknowledged response probes, one initial full page and no
errors/reconnects, with Background keepalive continuing. This is bounded connection evidence, not full hardware acceptance.
Version 0.1.4 fixes physical edit refresh: editing one face selects and redraws that edited face immediately, without an
A/B round trip. Other keys, held-press suppression and folder history are preserved; edits never execute actions. Tests
verify clock/date/icon PNG delivery through the simulated dock; the user confirmed this fix.
Version 0.1.5 adds virtual page following after successful physical page delivery, with a saved opt-out and guards for
active edits/imports/saves. It also bundles the tested polling/typing flicker fixes. Sleep/resume is deferred at the user's
request; remaining hardware acceptance and the navigation-latency investigation remain open.
Normal close keeps the WebView in the tray to preserve queued/failed edits. Tray Quit waits for the editor to flush;
failed saves keep the app open with retry available. This currently retains WebView memory while the window is closed.

Repeat the browser check with `node scripts/check-interface.cjs` where Playwright 1.58.2 and its Chromium are available.
For an external installation, set `UC_PLAYWRIGHT_MODULE` to its module folder. Screenshots go to the temporary
`uc-interface-preview` folder, or `UC_PREVIEW_OUTPUT` if set. The check runs headlessly and opens no native app.

For the native check, first build debug, set `UC_TEST_EXE` to its `debug/unified-control.exe`, then run
`node scripts/check-native-profiles.cjs`. It refuses release/installed binaries, creates its own temporary UC_HOME,
and sets quiet/multi/fake flags. It also verifies normal window close and timed tray Quit preserve a newer queued edit.
It does not open the real dock or alter the installed apps.

## Where the shell came from

Copied from CoolerScreen (`oldbugo/coolerscreen`) at commit `52f1eec79815fe33046279d6969f55ec52013d5f` (4.18.0), app shell only:

| UC file | CoolerScreen source | Changes |
|---|---|---|
| `src-tauri/src/main.rs` | `src-tauri/src/main.rs` | cooler commands, media server, live measure removed; tray always on |
| `src-tauri/src/live_app.rs` | `src-tauri/src/live_app.rs` | tray Open/Quit only; live engine, pause/fps, sleep/lock window removed; quiet runs add no icon |
| `src-tauri/src/instance_id.rs` | same | `UC_HOME`, own identifier |
| `src-tauri/src/quit_cmd.rs` | same | event names `UnifiedControlQuit-*` |
| `src-tauri/src/relaunch.rs` | same | installer and updater names `UnifiedControl_*` |
| `src-tauri/src/window/*` | same | `UC_TEST_QUIET`, own folder |
| `src-tauri/src/core/scheduler.rs`, `jobobject.rs`, `js.rs`, `autostart.rs` | same | unchanged except names (`UnifiedControl` start-up value) |
| `src-tauri/src/core/settings.rs` | same | `SettingsStore` unchanged; cooler fields replaced by UC's (`ui.page`) |
| `src-tauri/src/core/app.rs`, `commands.rs`, `error.rs`, `buildinfo.rs`, `paths.rs` | same | shell parts only |
| `src-tauri/src/device/log.rs` | same | own folder |
| `src-tauri/build.rs`, `installer-hooks.nsh`, `tauri.conf.json`, `Cargo.lock` | same | own commands, identity; no CoolerScreen 1.x clean-up |
| `tools/install-when-idle.ps1`, `scripts/make-release.ps1` | same | own exe, installer and folder names |
| `public/js/titlebar.js`, `public/js/dialog.js`, `public/css/tokens.css` | same | unchanged |

## Build

Needs Rust (stable, MSVC) and Node 18+.

```
npm install
npx tauri build
```

Debug build only: `cargo build --manifest-path src-tauri/Cargo.toml`. Tests: `cargo test --manifest-path src-tauri/Cargo.toml`.
Test instances must follow the quiet-test rules in `CLAUDE.md`.
