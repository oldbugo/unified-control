# Unified Control

One Windows tray app for the user's devices: the Ulanzi D200H stream dock first, later the CoolerScreen (Lovingcool SC70) and
the Ulanzi Dial. Private. App shell and combined-interface prototype; no real device control yet. Rules for working on it: `CLAUDE.md`.

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

The combined app will consume the device engines through versioned interfaces. These engine integrations are still
planned; the current interface prototype does not link or control either engine. Reviewed work is committed and pushed
to the relevant private repository as work proceeds. A feature branch on GitHub does not imply a merged or installed release.

## Interface prototype

The interface prototype edits a local sample dock layout. Its profiles, pages, actions and A/B face choices are preview
configuration, not the production profile store. It does not send key presses, launch apps, switch monitors, read AI
accounts or open device connections. CoolerScreen and Dial pages describe future integrations; they do not report live
connection status. Usage previews must display an explicit sample marker and never imply a live account reading.

Built: shared frame and interactive dock editor prototype, plus persisted navigation for all six pages. Still to build:
real HID transport, production profiles, rendering pipeline, action execution, usage-provider integration and device
coexistence. UI/browser checks do not establish native WebView or physical-device acceptance. No new installation is
part of this prototype milestone.

Verified for this prototype: 30 Rust library tests pass (one helper test ignored); headless Chromium checks cover all six
pages, 14 displays, A/B flips, draft persistence, profiles/pages, inert action edits, text escaping and corrupt-draft
recovery. Screenshots were inspected at 1440, 1100 and 800 pixel widths. Native WebView and physical dock checks remain.

Repeat the browser check with `node scripts/check-interface.cjs` where Playwright 1.58.2 and its Chromium are available.
For an external installation, set `UC_PLAYWRIGHT_MODULE` to its module folder. Screenshots go to the temporary
`uc-interface-preview` folder, or `UC_PREVIEW_OUTPUT` if set. The check runs headlessly and opens no native app.

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
