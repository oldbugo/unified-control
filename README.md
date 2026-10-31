# Unified Control

One Windows tray app for the user's devices: the Ulanzi D200H stream dock first, later the CoolerScreen (Lovingcool SC70) and
the Ulanzi Dial. Private. Dock MVP preview with native profiles, rendering, actions and explicitly enabled dock control.
Hardware acceptance is pending. Rules for working on it: `CLAUDE.md`.
Current user-confirmed results and remaining checks: [dock acceptance](docs/acceptance.md).

## Current direction (2026-10-30)

Build the combined interface from the start, with Home, Cooler screen, Stream dock, Dial, General and About pages.
Implement working dock support first, run normal functional and real-device acceptance tests, then integrate CoolerScreen,
then add the dial. A week-long daily-use trial is not required. The existing CoolerScreen installation stays separate
until its integration is built and tested.

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
pipeline is tested through the simulated dock; native HID opens only after the user enables the dock and Studio is absent. CoolerScreen engine integration is still planned. Reviewed work is committed and pushed
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

Checked: 95 UC library tests pass (one helper ignored), five app lifecycle tests, 61 reader tests and the original 105
Toolkit tests pass. The separate driver has 30 passing fake-transport tests from the foundation checkpoint. Browser checks
cover undo, picture persistence, all usage choices, literal launch arguments and three window widths. A quiet native WebView
check covers real IPC/files, image import, A/B persistence, physical-size PNGs, fake Studio block/reconnect, duplicate press
suppression and disable. Fake mode never starts real account readers, sensors or Windows actions. Native Quit also waits
for delayed image import and queued saves. Automated tests never open the physical dock or send input/audio/display actions.
Live Codex refreshed autonomously beyond five minutes in a six-minute check; Claude's existing source remained stale and
must show unknown. Preview 0.1.4/source `b212fe2` is installed; its new running PID and startup version were verified after
cooperative update from 0.1.3. The request-only executable override avoids the packaged shell's stale installed-file view.
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
verify clock/date/icon PNG delivery through the simulated dock. Physical confirmation follows installation.
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
