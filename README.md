# Unified Control

One Windows tray app for the user's devices: the Ulanzi D200H stream dock first, later the CoolerScreen (Lovingcool SC70) and
the Ulanzi Dial. Private. Combined editor, native profile storage and dock foundations; no real device control enabled yet. Rules for working on it: `CLAUDE.md`.

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

The app now consumes `ulanzi_dock` through a Cargo Git dependency pinned to reviewed source commit `a467475` in
`uc-ulanzi`. This development pin is immutable; tagged dependencies follow when releases are made. The profile/render
pipeline is tested through the simulated dock; the app does not automatically open native HID. CoolerScreen engine integration is still planned. Reviewed work is committed and pushed
to the relevant private repository as work proceeds. A feature branch on GitHub does not imply a merged or installed release.

## Current implementation

The native editor saves validated profiles and pages in the UC app-data folder, with stable IDs, a startup backup and
journaled recovery. Changes save as made; errors remain visible and retryable. Browser-only previews keep a separate
local draft. They are not silently imported into native profiles. [Storage contract](docs/profiles.md).

Built: combined six-page frame; native profile editing; PNG text/clock/unknown-data faces at 196x196 and 458x196;
transport-independent A/B routing, duplicate-press suppression, folder/back navigation and changed-key updates. A
saved profile is rendered and sent through the real ZIP/packet pipeline to the simulated dock in integration tests.
The separate hardware crate implements native Windows HID, serialized writes and reconnect, tested with fake transports
only. Cached reconnect-image replay is off by default so stale usage pictures cannot be silently replayed.

Still to build/check: app-level Studio coexistence and native-device enablement, real action execution, account readers
and the full 16 usage faces, sensors, icon/picture rendering, physical dock acceptance and the later cooler/dial engines.
Initial native icon faces use their text label; allowance/reset/trend and unavailable sensors show unknown values.
No accounts are read and no launch, hotkey, media or monitor action is executed by this stage. No new installation is included.

Checked: 52 UC library tests pass (one helper ignored); 30 driver tests pass. Headless browser checks cover the combined
interface, stable ID conversion, single-flight saves/retry, draft recovery and three window widths. A separate quiet
native WebView test exercises actual IPC, isolated profile files, A/B persistence and physical-size PNG previews.
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
