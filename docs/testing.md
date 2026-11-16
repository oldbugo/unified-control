# Testing index
*Document type: reference. It describes how the part works. For the current state, read [status](status.md).*

This page lists every check script in `scripts/`, what it proves and what it needs. Run all commands from the repository root.

## Rules for every test run

Tests must never disturb the user's desktop. They must not show a window, take focus, play sound or touch real devices. The full rules are in `CLAUDE.md`. In short:

- Use a debug build only. A release build hands its launch to the user's running app.
- Set `UC_HOME` to a new folder in your scratchpad. It holds settings, `device.log` and `window.json`.
- Set `UC_TEST_QUIET=1`. The window opens off-screen, without focus, and no tray icon is added.
- Set `UC_MULTI=1`. No single-instance lock, so the test never joins the installed app.
- Set `UC_FAKE_DEVICE=1`. Fake devices only.
- Stop a test instance with `UnifiedControl.exe --quit`, using the same build kind and `UC_HOME`.
- Never launch the installed app from a Claude shell.

The native scripts set these four variables themselves. Still keep your own shell clean of the user's real `UC_HOME`.

## Rust tests

```
cargo test --manifest-path src-tauri/Cargo.toml
```

Use your own `CARGO_TARGET_DIR` under `%TEMP%`. Run long builds in the background. Some core tests are ignored on purpose. This is expected.

## Companion tests (usage reader)

```
cd companions/usage-reader
node --test tests/*.test.mjs
```

This is the same command as `npm test` in that folder. The tests use fixtures only. They read no real account.

## Check scripts

Run each script with `node scripts/<name>`. "Headless" means a fixture web page in Playwright Chromium. It starts no UC app. "Native" means it starts a disposable debug UC app with an invisible window and fake devices.

Variables used below:

- `UC_PLAYWRIGHT_MODULE`: path to the Playwright module. If not set, the script loads `playwright` by name.
- `UC_TEST_EXE`: full path to the debug `unified-control.exe`. The path must contain a `debug` folder.
- `UC_COOLER_WORKER_EXE`: full path to the debug `uc-cooler-worker.exe`.

| Script | What it proves | Env vars | Debug exe | Kind |
| --- | --- | --- | --- | --- |
| `check-agent-editor.cjs` | Pure editor modules and the production UI follow agent layout changes, using fixture IPC only. | `UC_PLAYWRIGHT_MODULE` | No | Headless |
| `check-agent-mcp.cjs` | The official MCP client talks to the stdio adapter and a request-only fixture. Needs `npm install` in `companions/agent-mcp`. | `UC_MCP_FIXTURE_CALLS`, `UC_MCP_FIXTURE_MODE` (set by the script) | No | Node only |
| `check-bundled-reader.cjs` | The installed or staged usage-reader resources work, without starting UC or any account source. | `UC_RESOURCE_DIR` (required) | No | Node only |
| `check-cooler-entry.cjs` | The embedded cooler UI behaves correctly when you enter the cooler page. | `UC_PLAYWRIGHT_MODULE` | No | Headless |
| `check-cooler-interface.cjs` | The cooler page interface works against browser fixtures. | `UC_PLAYWRIGHT_MODULE` | No | Headless |
| `check-device-settings.cjs` | Device visibility and device settings work. No real hardware or account. | `UC_PLAYWRIGHT_MODULE` | No | Headless |
| `check-device-status.cjs` | Pure status summaries and the status display, with fixture IPC. | `UC_PLAYWRIGHT_MODULE` | No | Headless |
| `check-interface.cjs` | The main interface renders and navigates. It can write preview images. | `UC_PLAYWRIGHT_MODULE`, `UC_PREVIEW_OUTPUT` (optional) | No | Headless |
| `check-monitor-discovery.cjs` | Monitor discovery results and failures reach the UI through fixture IPC. It never calls the display or audio APIs. | `UC_PLAYWRIGHT_MODULE` | No | Headless |
| `check-vibe-key.cjs` | The Vibe Key (AU05) switch, Home card and page: off by default, only checked settings can be sent to a real key, changes and UC actions reach the host. | `UC_PLAYWRIGHT_MODULE` | No | Headless |
| `check-navigation-search.cjs` | Navigation and search drafts in the browser UI. No native app or OS action. | `UC_PLAYWRIGHT_MODULE` | No | Headless |
| `check-native-agent.cjs` | End to end: MCP, request-only native proxy, same-user host pipe, editor. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE` | Yes | Native |
| `check-native-vibe-key.cjs` | The Vibe Key worker in the real debug app with the encrypted fake key: opt-in switch, first read, key typing and standby changes, a UC action through the claimed F22 hotkey (fake desktop, nothing real claimed), release on disable, allowlist only, stall pause and Reconnect. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE` | Yes | Native |
| `check-native-cooler.cjs` | The debug WebView works with the real cooler engine over inherited pipes. Fake devices. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE`, `UC_COOLER_WORKER_EXE` | Yes | Native |
| `check-native-cooler-lifetime.cjs` | The cooler worker lifetime follows the host (start and shutdown). | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE`, `UC_COOLER_WORKER_EXE` | Yes | Native |
| `check-native-cooler-migration.cjs` | Cooler data migration and comparison work with the real worker IPC. Source plugins are off. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE`, `UC_COOLER_WORKER_EXE` | Yes | Native |
| `check-native-cross-device.cjs` | Cross-device controls work across the dock and cooler workers with synthetic sources. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE`, `UC_COOLER_WORKER_EXE` | Yes | Native |
| `check-native-dock-worker.cjs` | Host and dock-child IPC work, and a crash in one worker does not stop the other. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE`, `UC_COOLER_WORKER_EXE` | Yes | Native |
| `check-native-profiles.cjs` | Profile, page and key editing in the real debug app. Never touches installed data. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE`, `UC_PREVIEW_OUTPUT` (optional) | Yes | Native |
| `check-native-shared-hardware.cjs` | Shared hardware readings reach both workers, using synthetic sources. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE`, `UC_COOLER_WORKER_EXE` | Yes | Native |
| `check-native-shared-services.cjs` | Shared services (AI usage, music, weather) feed both workers, using synthetic sources. | `UC_PLAYWRIGHT_MODULE`, `UC_TEST_EXE`, `UC_COOLER_WORKER_EXE` | Yes | Native |
| `capture-navigation.cjs` | Not a test. A read-only capture of key navigation from the installed app over MCP. It sends no key presses and changes no layout. | `UC_MCP_NODE`, `UC_MCP_SERVER` (both required), `UC_NAV_CAPTURE_OUTPUT` (optional) | No (uses the installed app) | Production read-only |

## Notes

- `capture-navigation.cjs` talks to the user's installed app. Run it only when the user asks. Pass the duration in seconds (10 to 600) as the first argument. See [navigation timing](navigation-timing.md).
- A passing script prints no failure and exits with code 0. A failed `assert` stops the script with a non-zero code.
