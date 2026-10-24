# Unified Control

One Windows tray app for the user's devices: the Ulanzi D200H stream dock first, later the CoolerScreen (Lovingcool SC70) and
the Ulanzi Dial. Private. Phase 0: the app shell only (no device code yet). Rules for working on it: `CLAUDE.md`.

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
