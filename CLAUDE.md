# Unified Control (UC): always-read guide

One visible Windows tray app that runs the user's devices: the Ulanzi D200H stream dock first (replacing Ulanzi Studio),
later the CoolerScreen (Lovingcool SC70) page and the Ulanzi Dial. The build plan lives in the private knowledge vault,
`Projects/Device app build plan.md`; read it (and the vault's `Start here.md`) at the start of every session.

The user is not a programmer: report in plain language.

## Standing user instructions (read in full at every session start)

- 2026-11-03: Install tested UC builds as needed without asking again. Use the idle-safe updater; defer while busy and
  never force-close. Install/restart quietly to the tray, then verify startup, device reconnection and preserved
  configuration. This is standing permission for UC updates, not unrelated apps.
- 2026-11-03: Cooler Live must continue when switching device pages and closing UC to the tray. Device enable toggles belong
  in General; only enabled devices appear in the sidebar and Home. Remove the large per-device enable headers, let cooler
  controls fill the right pane without an outer frame, and polish the overall interface. The user reports manual Reconnect
  recovered physical Live; software checks must distinguish real Live ownership from temporary editor previews.
- 2026-11-02: Build an agent-agnostic MCP interface for direct status, diagnostics and personalized configuration from Codex or Claude. Reuse the running app's core, coordinate with the editor and test with simulated devices. Physical action tools follow separately.
- 2026-11-01: Repeated cooler freezes require targeted recovery and persistent local connection diagnostics in UC, including while its editor is closed. The requested diagnostics are on with a saved opt-out; no hidden service or automatic elevated/USB resets.
- 2026-11-01: Proceed with CoolerScreen integration, then refine the UI. AI Usage and Monitor Toggle were reported not working; keep these as functional failures to resolve, not missing planned features. Sleep/resume remains deferred.
- 2026-10-30: Build with the combined interface visible from the start, including a CoolerScreen page without engine integration. Establish working dock support and run normal acceptance tests; no week-long daily-use trial. Then integrate CoolerScreen, then add the dial. This supersedes the original hidden-cooler-page and daily-use-trial plan.
- 2026-10-30: Keep reviewed work on GitHub as development proceeds. Preserve separate private repositories for UC, Ulanzi hardware and CoolerScreen, grouped under the existing Unified Control arrangement. Verify privacy before uploading; distinguish pushed branches from merged or installed releases.

Carried over from CoolerScreen's standing list (user-stated, 2026-10-10 to 2026-10-19) and confirmed for UC on 2026-10-25:

- Nothing hidden in the background. A visible tray app; no Windows service. Anything that runs must be visible and easy to quit.
- Start with Windows is opt-in: off by default.
- No Save buttons. Every change is saved as it is made and applies at once; closing the window or quitting changes nothing.
- Every option is a setting the user can change. Nothing new turns on by itself.
- Testing must never intrude on the user's desktop (they may be in a fullscreen game): no test window may appear, take focus,
  go fullscreen or come to the front, and tests never play sound.
- Never close, restart or kill the user's running CoolerScreen or Ulanzi Studio. Ask the user to quit Studio themselves.
- Do not launch installed apps from a Claude shell. Claude shells see a private MSIX copy of %APPDATA%, so an app started from
  one saves into a hidden folder, and reads of %APPDATA% from a Claude shell may be stale. After an install, tell the user to
  open UC from the Start menu.
- Install builds only when the app is idle (no job running): `tools/install-when-idle.ps1`. `UnifiedControl.exe --quit` refuses
  while a job runs; never force-quit.

## Versions, tags, installs

- UC numbering starts at 0.x (0.1.0, 0.2.0, ... in landing order through phase 1); CoolerScreen keeps its own numbering.
- **Only the orchestrator sets versions, tags and installs.** Workers never edit the version in `src-tauri/Cargo.toml`,
  `src-tauri/tauri.conf.json` or `package.json`, never tag, never run the installer.
- Work on branches; the orchestrator merges into `main`.

## Quiet tests (every test instance)

- Debug builds only. A release build hands its launch to the user's running app and can raise its window.
- Always set all of:
  - `UC_HOME=<a folder in your scratchpad>`: its own app folder (settings.json, device.log, window.json).
  - `UC_TEST_QUIET=1`: the window is built off-screen, without focus, out of the taskbar and never activated; its position is not
    remembered; the tray keeps its logic but no icon is added to the notification area (`UC_TEST_TRAY_ICON=1` shows it).
  - `UC_MULTI=1`: no single-instance lock (debug builds only), so the test never joins or raises the installed app.
  - `UC_FAKE_DEVICE=1`: fake devices only. Phase 0 has no device code; from phase 1 the dock page must never open the real dock
    when this is set.
- Debug hooks: `UC_TEST_TRAY_QUIT_MS=<ms>` runs the tray Quit path after a delay; `UC_TEST_TRAY=open@500,close@1500,quit@3000`
  drives the tray menu on a timer and prints the state.
- Stop a test instance with `UnifiedControl.exe --quit` (same build kind and `UC_HOME`): it only reaches that instance.
- Builds: your own `CARGO_TARGET_DIR` under `%TEMP%`; long builds in the background.

## Hardware rules

- Only the orchestrator touches real devices, and only with the user present. Workers use the fake devices.
- Never send firmware, OTA, ADB or storage commands to the dock or dial, and never guess commands.
- For the cooler (phase 2): never 0x05, 0x0C or 0x23; never non-MJPEG to clip slots.

## Identity (must never collide with CoolerScreen)

App name "Unified Control"; exe `UnifiedControl.exe`; bundle identifier `com.oldbugo.unifiedcontrol`; app folder
`%APPDATA%\UnifiedControl`; quit events `Local\UnifiedControlQuit-<scope>`; start-up value `UnifiedControl`; installer
`UnifiedControl_<x.y.z>_x64-setup.exe`; tray tooltip "Unified Control".

## Copied shell

The app shell was copied from CoolerScreen (commit in `README.md`). Phase 2 merges back, so keep the copied code close to the
original: no renames or rewrites for taste.
