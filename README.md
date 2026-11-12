# Unified Control

Unified Control (UC) is one visible Windows tray app for the user's devices. It runs the Ulanzi D200H stream dock, the Lovingcool SC70 cooler screen and, later, the Ulanzi Dial.
Devices are opt-in. Settings save as you change them. The repository is private.

Current state, next work and open checks: [docs/status.md](docs/status.md). Agent rules: `AGENTS.md` and `CLAUDE.md`.

## Repositories

UC is one product in four private repositories under the `oldbugo` account. They share the `unified-control` topic and a `UC:` description prefix.

| Repository | What it holds |
| --- | --- |
| [unified-control](https://github.com/oldbugo/unified-control) | Combined app, interface, profiles, actions, shared services, installer |
| [uc-ulanzi](https://github.com/oldbugo/uc-ulanzi) | Ulanzi dock hardware engine and fake dock; later the dial |
| [coolerscreen](https://github.com/oldbugo/coolerscreen) | CoolerScreen app and cooler engine. UC pins one engine commit in `companions/cooler-engine.json`. |
| [ulanzi-toolkit](https://github.com/oldbugo/ulanzi-toolkit) | Ulanzi Studio plugins, including the usage readers and Monitor Toggle |

UC uses `ulanzi_dock` through a Cargo Git dependency pinned to a reviewed commit in `uc-ulanzi`.

## Build and test

Needs Rust (stable, MSVC) and Node 18+.

```
npm install
npx tauri build
```

Debug build only: `cargo build --manifest-path src-tauri/Cargo.toml`. Rust tests: `cargo test --manifest-path src-tauri/Cargo.toml`.
Every check script, its settings and the quiet-test rules are in [docs/testing.md](docs/testing.md).
Test instances must follow the quiet-test rules in `CLAUDE.md`.

## Where the docs are

- Current state: [docs/status.md](docs/status.md). It also lists all other docs.
- Testing: [docs/testing.md](docs/testing.md).
- Reference: [agent control (MCP)](docs/agent-control.md), [actions](docs/actions.md), [profiles](docs/profiles.md), [usage](docs/usage.md), [visuals](docs/visuals.md), [dock service](docs/dock-service.md), and `docs/device-app/`.
- Evidence (dated): [acceptance](docs/acceptance.md), [cooler connection health](docs/cooler-connection-health.md), [function failures](docs/function-failures.md), [backlog](docs/backlog.md), [cooler entry delay](docs/cooler-entry-delay.md), [navigation timing](docs/navigation-timing.md).
- Version history: [CHANGELOG.md](CHANGELOG.md).
- Older records: docs/archive/ (do not read unless needed). The long README history to 0.9.2 is in [docs/archive/readme-history-to-0.9.2.md](docs/archive/readme-history-to-0.9.2.md).

## Identity

App name "Unified Control"; exe `UnifiedControl.exe`; bundle identifier `com.oldbugo.unifiedcontrol`; app folder
`%APPDATA%\UnifiedControl`; quit events `Local\UnifiedControlQuit-<scope>`; start-up value `UnifiedControl`; installer
`UnifiedControl_<x.y.z>_x64-setup.exe`; tray tooltip "Unified Control".

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

