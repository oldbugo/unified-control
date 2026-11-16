# Vibe Key (Ulanzi AU05)
*Document type: reference. It describes how the part works. For the current state, read [status](../status.md).*

Built on 2026-11-16 against the fake key only. Nothing here is checked on the real key yet.

## What it is

The Ulanzi AU05 "Vibe Key" has three keys (K1, K2, K3), a knob and a microphone. It connects through its
own 2.4 GHz USB-C receiver. Windows sees the receiver as a keyboard, a mouse, a microphone and one private
channel. UC uses only the private channel. The protocol facts are on the vault page "Ulanzi AU05 Vibe Key
facts".

## Where the parts are

| Part | Where |
| --- | --- |
| Driver: encryption, messages, command allowlist, safe session, fake key, Windows transport | `oldbugo/uc-ulanzi`, crate `crates/ulanzi_au05` |
| Worker: one background thread that owns the key | `src-tauri/src/core/au05.rs` |
| Hotkeys, focus and synthetic keys (behind a trait, with a fake) | `src-tauri/src/core/au05_hotkey.rs` |
| Quick menu: model, profiles, state machine | `src-tauri/src/core/au05_menu.rs` (wiring: `MenuCtl` in `au05.rs`) |
| Claude sessions (hooks, session list, Claude settings merge) | `src-tauri/src/core/claude_sessions.rs`, `src-tauri/src/claude_hook.rs` (`--claude-hook`) |
| Quick menu window | `src-tauri/src/quick_menu.rs`, `public/quick-menu.html`, `public/js/quick-menu.js`, `public/css/quick-menu.css` |
| Settings and commands | `src-tauri/src/core/app.rs` (`au05_*`), `commands.rs`, `settings.rs` (`au05`) |
| Page, General switch and Home card | `public/js/au05-panel.js`, wired in `public/app.js` |
| Page checks | `scripts/check-vibe-key.cjs` (headless fixtures), `scripts/check-native-vibe-key.cjs` (quiet debug app, fake key) |

This follows the dock: the device driver lives in the uc-ulanzi repository, and UC owns the worker and
the page. Unlike the dock, the key runs in a thread of the UC host, not in a child process. The thread is
restarted after a crash (`sync::restarting`), and every device call has a time limit, so a fault or a
stall in the key cannot stop the window, the dock or the cooler.

## Safety rules in the code

- Off by default. Nothing opens the receiver until the user turns on Vibe Key in General.
- With `UC_FAKE_DEVICE` set, the real key is never opened. The fake key is encrypted in both directions.
- Only commands in the allowlist (`Request`) can be encoded. Reboot, serial number, flash ID, UUID,
  receiver command 0x05, firmware and the keep-alive 0x23 have no encoder.
- The allowlist is enforced three times on a real key: the page parser, the session, and the Windows
  transport itself, which decrypts every report and sends only one exact, in-range, known command
  (`protocol::check_outgoing`). Value ranges: light mode and record type 0..3, record time and breathe
  level 0..15, brightness values 0..10, hooks mode 0..1, noise reduction 0..3.
- Checked on the real key: all reads except the knob-press typing, and the sets standby time and key
  typing for K1..K3. Since 2026-11-16 (user request: "expose all the parameters in the app so I can test it
  out all at once") every other known setting can also be sent to the real key, labelled "Untested" on the
  page: power-off time, light mode, all-on brightness, the five fields of each of the four light mode
  records (masks 0x04 to 0x40), hooks mode, microphone noise reduction and knob-press typing. Each set is
  read back and the page shows the raw value next to the friendly one.
- Every write has a 2 s limit. One request at a time, and never a second write while a reply is
  outstanding. No retry: a reply that does not come in time (20 s for key typing, cmd 0x50; 2 s for the
  rest) or a write that does not finish pauses sending for 30 s. After three stalls in a row, sending
  stops until the user presses Reconnect. While paused, a change is refused at once.
- Nothing is sent unless the key has sent a message in the last 11 s. A set waits until the key is quiet:
  1.5 s after it woke and 1.5 s after the last key or knob event.
- Key typing (cmd 0x50) is never read at connect. A key's list is read only when the user presses "Read
  from key" on the page, one key at a time. A change to a key's typing learns the new list from the set's
  echo, so no extra read is sent. The other reads at connect are capped at 10 s in all.
- A key may be set to type only the keys the page offers: letters, digits, Enter, Esc, Backspace, Tab,
  Space, Minus, Plus, F1..F24 and the navigation keys. No Power, Application or lock keys; at most one
  modifier entry, first, and at least one key.
- The noise-reduction set writes back the two unknown values exactly as read.
- Start, stop and hotkey claims are serialised (`au05_config_lock` in the app and a lock in the worker),
  so overlapping commands cannot start two sessions.
- A UC action runs on its own thread, never on the device or hotkey thread. A crash in an action does not
  stop the worker.

## How a key runs an action (design change, 2026-11-16)

Not every control sends a private event. On the real key only K1 and the knob turns did; K2, K3 and the
knob press never did. So UC does not use private events for actions. Instead:

1. The key is set to type its own unused code: K1 F20, K2 F21, K3 F22, knob press F19. This is stored on
   the key (key typing command).
2. UC claims that code as a global hotkey: `RegisterHotKey(NULL, id, MOD_NOREPEAT, vk)` on one thread with
   a message loop. Windows swallows the key press and sends WM_HOTKEY to UC, whatever app has focus.
3. UC runs the chosen action on its own thread.

Hotkeys are claimed only while the Vibe Key is enabled and a key has an action. They are released when the
action is set to Nothing, when the key is switched to Types keys or Factory default, on disable and on quit.
If another app already holds the code, the page shows that the key is not available.

Actions:

| Action | What UC does | Evidence |
| --- | --- | --- |
| Start dictation in Claude | Find the visible window of `claude.exe` whose title starts with "Claude", restore it, `SetForegroundWindow` (allowed after WM_HOTKEY); only if that fails, tap Alt (skipped when Alt is down; `SendInput` result checked) and try again; then Ctrl + D | Prototype `au05_k3_hotkey.py`, user-confirmed |
| Start dictation in ChatGPT | The same with `chatgpt.exe`, title "ChatGPT", then Ctrl + Shift + D | Not checked (exe name, title and shortcut) |
| Send (Enter) | Enter in the focused app | Not checked |
| Custom key combination | That combination in the focused app (the dock's reviewed hotkey sender) | Not checked |
| Nothing (harmless) | The key types its own code; nothing happens | K3 types F22, user-reported |

Everything that touches the desktop is behind the `Desktop` trait. Unit tests and fake instances
(`UC_FAKE_DEVICE`) use `FakeDesktop`: it claims no hotkey, moves no focus and sends no input. The fake key
delivers a press to it only when the fake key types that key's own code, like Windows would.

## What the page offers

- The main element is a drawing of the key, in the real layout (Ulanzi's product image in the AU05 user
  guide, docs.ulanzistudio.com, and the Studio editor): logo and microphone grille at the top, a large round
  knob with a light ring, then K1 (microphone icon), K2 (tick) and K3 (cross) in one column. Five zones:
  knob turn (the outer ring), knob press (the centre), K1, K2, K3. Each shows its current assignment
  beside it ("Claude dictation", "Types Ctrl + D", "Factory: Esc", "Nothing", "Factory scroll").
- Clicking a zone (or Enter or Space on it) opens its settings in a panel beside the drawing; changes apply
  at once. A zone flashes when its control sends a message (K1 and the knob turns) or when UC runs its
  action on a hotkey. K2, K3 and the knob press send no message of their own, so they flash only for a UC
  action.
- The knob turn keeps the factory scroll. UC actions for the turns are not built yet (the factory scroll
  would still happen at the same time).
- Below the drawing, "Device settings" holds power, microphone, light, the control codes, "Read all
  settings" and "Reconnect".

- "Read all settings": every read except key typing, one at a time with the usual pacing. Key typing is
  still read one key at a time ("Read from key" / "Read again").

- Connection state, battery and a Reconnect button.
- For K1, K2, K3 and the knob press, "What it does": Unified Control action (then the action and a "Ready"
  or "not available" note), Types keys (such as Ctrl + D, stored on the key) or Factory default. The knob
  press typing is shown read-only on a real key until it is checked.
- The knob turns keep their factory scroll. Their private events (index 4 and 5) do arrive and are shown;
  they can drive UC actions later.
- Standby time and power-off time, light mode and all-on brightness, microphone noise reduction.
- A live list of private events and of the actions UC ran.
- "Light follows Claude Code" is shown as planned. The code has a stub only (`agent_light_requests`
  returns nothing).

## Actions by application (2026-11-17)

The action list is a data table (`CATALOGUE` and `APPS` in `src-tauri/src/core/au05_hotkey.rs`), grouped as
Claude, ChatGPT, Windows / media, Custom shortcut and Nothing. An action brings the app forward when needed,
then presses a key combination or a media key. To add an app or an action, add a row there.

Verified means vendor documentation or checked on this PC; the page marks every other action "unverified".

- Claude: Start dictation Ctrl + D (checked on this PC), Send Enter and Stop/interrupt Esc (Claude Code desktop
  docs). New session, next/previous session, model menu, permission menu, side chat, terminal, diff pane and
  shortcut list are inferred from the macOS list (unverified). "Open Claude from anywhere" Ctrl + Alt + Space
  is a third-party report (unverified). Stop/interrupt is never a default.
- ChatGPT: every shortcut is unverified (OpenAI's help pages could not be read): dictation Ctrl + Shift + D,
  new chat Ctrl + Shift + O, sidebar Ctrl + Shift + S, focus input Shift + Esc, shortcut list Ctrl + /,
  companion window Alt + Space. The app's window (`chatgpt.exe`, title "ChatGPT") is also unverified.
- Windows / media: play/pause, next, previous, volume up/down, mute (media keys), show desktop Win + D, task view
  Win + Tab, screenshot Win + Shift + S (verified Windows shortcuts), microphone mute in calls Win + Alt + K
  (unverified: only in call apps that support it).
- Custom shortcut: any key combination, sent to the focused app or after bringing Claude or ChatGPT forward.

## Choosing an action never waits for the key (fix, 2026-11-17)

Before, choosing "Unified Control action" waited for the key (up to 45 s) with the whole panel disabled, and
an asleep key made it fail after 20 s with the message far above the panel. Now the action is saved at once
(no device write) and the hotkey is claimed at once, even before the key's list is read. Setting the key to
type its own code is a background job shown under the choice: "Waiting for the key: press any key to wake it",
"Sending to the key", "Key updated", or the error with "Try again". The job is sent only when the key is awake,
one key-typing command at a time, with the usual quiet time, time limits and no retry. If the key falls asleep
before the job is written, the job waits again; any other failure ends the job with the error.

## Quick menu (2026-11-17)

User request: "a bit similar to what Logitech have" (the MX Master quick menu). Three keys are not enough, so
one key opens a menu.

- **Open and close.** Give any key (K1-K3 or the knob press) the action "Open quick menu". Pressing it opens
  the menu; pressing it again, Esc, clicking another window, or about 8 seconds without input closes it.
- **Move and choose.** Knob turns move the highlight (right = down). The knob press chooses. The knob press
  sends no private event, so it chooses through its own code F19: the knob press must be set to a Unified
  Control action (Nothing is fine). UC claims F19 while any key opens the menu. Arrow keys, Enter, Backspace
  and mouse clicks work in the window too. Another key with an action closes the menu and runs its action.
- **Knob turn** (Knob turn in the drawing; 2026-11-17). "Factory scroll" (stored on the key) or a Unified
  Control choice: the knob types its own codes F23 (right) and F24 (left), which UC claims as hotkeys. While
  the menu is closed UC then scrolls itself (one notch per click on the window under the mouse, SendInput),
  changes the volume, or does nothing; while it is open the turns move the menu and nothing scrolls behind
  it. Knob right typing F23 was proven on the real key (user-run, 2026-11-17); knob left (F24) is the same
  command and is marked Untested. With the factory scroll, the knob's private events move the menu (the
  window behind may scroll too). A turn moves the menu once: if the knob is known (read or set) to type F23,
  or was set to by the user, only the hotkeys move it.
- **Place.** Not at the mouse: near the top-right corner of the primary monitor's work area. Drag it by its
  title; the monitor (device name and rectangle) and the position are saved in `quick-menu.json` in the app
  folder and used next time. If that monitor is gone, it goes back to the default place. Sizes are logical
  pixels, so each monitor's scale applies. The window is topmost, out of the taskbar, and takes focus. After a
  choice it closes first, then the action runs (and may bring Claude or ChatGPT forward).
- **Menu model** (`au05.menu` in settings.json, data only): entries of type `action` (any catalogue action; the
  editor does not offer custom shortcuts yet, though a saved one is kept), `profile` (switch profile), `target` (change the target app), `submenu` (up to three
  levels, 64 entries), `claudeSessions` (the live list of Claude sessions, with an optional follow-up action
  such as dictation) and `claudeSession` (one saved session). The page's editor adds, removes,
  moves, renames and changes entries; "Reset to the default menu" restores the shipped one.
- **Default menu:** Claude dictation, Claude new session, Claude sessions, ChatGPT dictation, Switch profile (Claude / ChatGPT /
  Media), Target app (Claude / ChatGPT), Play / pause, Screenshot.
- **Profiles** (`au05.profiles`, `au05.profile`): a profile is a named set of actions for K1, K2, K3 and the
  knob press. `au05.actions` are always the current profile's actions. Switching applies at once and writes
  nothing to the key. A profile without a menu key keeps the current menu key, so the menu can always switch
  back. On the first start after the update the "Claude" profile takes the user's current actions, so nothing
  changes; "ChatGPT" (dictation, send) and "Media" (play/pause, next track) apply only when chosen.
- **Target app** (`au05.target`, Claude or ChatGPT): the "Target app" actions (start dictation, send, new
  session/chat) do the Claude or the ChatGPT action.
- The page shows the current profile and target above the drawing; the menu shows them in its header.
- Quiet and fake instances never create the window (`quick_menu::enabled`); the menu state is still there, so
  tests drive it through the fake key and `au05_menu`.
- Later, not built: an always-on small assistant that shows the current profile and target.

## Claude sessions (2026-11-17)

The user chose Claude Code hooks. Opt-in: "Connect Claude sessions" on the Vibe Key page.

- **The hook program** is `UC-ClaudeHook-v1.exe`, a copy of UnifiedControl.exe that the installer makes next to
  it (like `UC-AgentRequest-v1.exe`). A program with that name only ever runs the hook, whatever its
  arguments, so a hook can never start or raise the app. Hooks never point at UnifiedControl.exe: an older or
  virtualized copy without the hook code would raise the window on every prompt.
- **Connect** adds four hook entries (SessionStart, UserPromptSubmit, Stop, SessionEnd) to
  `%USERPROFILE%\.claude\settings.json`. Each runs the proxy directly (exec form, no shell):
  `{"type": "command", "command": "<install folder>\UC-ClaudeHook-v1.exe", "args": ["--claude-hook", "<event>",
  "--uc-hook-v1"], "timeout": 5}`. UC's entries are exactly those with that program name and the
  `--uc-hook-v1` marker. Connect needs the installed proxy; debug builds, fake and quiet instances only ever use
  `UC_CLAUDE_HOME` (never the real folder).
- **How the file is changed** (`claude_sessions::update_file`): read, change, and only if the content
  changed: back up the old bytes (`settings.json.uc-backup-<ms>`, UC keeps its last three), write a temp
  file, check the file was not changed meanwhile (else abort with a message), then replace it. Connect when
  already connected writes nothing. A symlinked settings file is written at its target. Invalid JSON is
  refused and the file left untouched. The file is rewritten with standard formatting (content kept); the
  page says so.
- **Disconnect** removes exactly UC's entries and forgets the session list. If Claude's file cannot be read
  or changed, UC still stops listening and tells the user which entries to remove by hand. **Uninstall:** the
  uninstaller runs `UnifiedControl.exe --claude-disconnect` (headless, same path) before it deletes the
  proxy, so Claude shows no hook errors afterwards; the page also says to disconnect before uninstalling.
- **Blocked hooks:** if the organisation's managed settings (`C:\Program Files\ClaudeCode\managed-settings.json`,
  `managed-settings.d`, or the `ClaudeCode` policy in the registry) block user hooks (`allowManagedHooksOnly`,
  `disableAllHooks`, `strictPluginOnlyCustomization`), or the user's own `disableAllHooks` is on, the page
  says so and Connect is not offered.
- **The hook run** reads the hook's stdin and only `CLAUDE_CODE_HOST_SESSION_ID` (the desktop session id,
  `local_<uuid>`) and `CLAUDE_CODE_SESSION_ID`; no other environment variable (the environment holds tokens
  and the user's email). It sends {event, desktop id, Claude Code session id, folder (control characters
  removed), time, and for UserPromptSubmit the first 80 characters of the prompt as a label} over UC's local
  pipe, prints nothing (its panic hook is silent), writes nothing and exits 0 (24-31 ms in the quiet test,
  debug build). Without the marker, or if UC is not running or not connected, nothing happens. Sessions
  without a desktop id (terminal sessions) are not sent.
- **Session list** (`claude-sessions.json` in the app folder): desktop id, the Claude Code session it is
  bound to (the first one that reported it; child sessions can share the desktop id, and their events change
  nothing), project (the folder's name, at most 60 characters), label, state (working after a prompt, idle
  after Stop), last activity. A session leaves at its own SessionEnd or after 24 h without activity.
- **Quick menu:** "Claude sessions" lists them, grouped by project (one project: the sessions directly), most
  recent first. Choosing one opens `claude://claude.ai/epitaxy/<desktop id>` (ShellExecute), then, after
  about 1.2 s, runs the entry's follow-up action if one is set (for example Claude: Start dictation).
- The log records connect and disconnect and "menu chose a Claude session"; never prompt text or labels.
- ChatGPT: nothing similar yet.

## Log

`%APPDATA%\UnifiedControl\devices\au05\device.log` (rotated at about 1 MB). It records: worker start and
stop; receiver opened or not (error kind only); state changes (waiting, connected, asleep, paused, not found);
every command queued and its result (for example "done set key typing K3 (1 entries): ok" or ": NoReply");
page changes and action choices; key jobs; hotkey claimed, not claimed (with the reason) or released; each
action run with its result; the quick menu opened and closed (and why), each choice, profile and target
changes, and menu edits (entry count only); knob turn changes; Claude sessions connected and disconnected. It never records report bytes, the key, window titles or typed text.

## Tests

- Driver: `cargo test -p ulanzi_au05` in uc-ulanzi (25 tests, fake key and pure functions).
- Worker and hotkey layer: `cargo test --lib au05` in `src-tauri` (14 tests, fake key and fake desktop), and
  `cargo test --lib au05_menu` (the quick menu's state machine).
- Page: `node scripts/check-vibe-key.cjs` (headless fixture; set `UC_PLAYWRIGHT_MODULE`).
- Native: `node scripts/check-native-vibe-key.cjs` (quiet debug app with the fake key; also set `UC_TEST_EXE`).

## Open checks on the real key (orchestrator, user present)

1. Enable in General. Confirm that UC opens only the private channel and shows "Connected" after a key
   press, with the battery value.
2. Confirm the connect reads: standby 600 s, power-off 3600 s, light, mic (no cmd 0x50). Then press
   "Read from key" for K1, K2 and K3 one at a time. Watch for the 15 s key-typing read seen once on
   2026-11-16 (UC now waits up to 20 s and sends nothing else meanwhile).
3. Set K3 to Unified Control action, Start dictation in Claude, from the page. From another app press K3:
   Claude comes forward and dictation starts, once. Check that F22 is not also typed into the other app.
4. Set K1 to Types keys, Ctrl + D, and back to Factory default. Read back. Confirm that no modifier stays
   down afterwards.
5. Set K2 to Send (Enter). Confirm one Enter in the focused app.
6. Disable the Vibe Key and quit UC: confirm F21/F22 are released (another test app can claim them).
7. Let the key sleep. Confirm that a change waits ("asleep") and sends nothing until a key press.
8. Unplug and plug in the receiver. Confirm the reconnect.
9. Check the knob-press typing get and set (button 3), then the other unverified sets one by one (power-off
   time, light mode and brightness, mic noise reduction). After each check, update
   `Request::verified_on_hardware` in the driver.
10. ChatGPT dictation: confirm the exe name, the window title and Ctrl + Shift + D.
11. Later: "send now" for queued messages in Claude (needs the right shortcut).
12. Quick menu: set K3 to "Open quick menu" and the knob press to a Unified Control action (Nothing). Press
    K3 in another app: the menu appears near the top-right of the main screen and takes focus. Turn the knob:
    the highlight moves (one step per click; note whether the window behind also scrolls). Press the knob:
    the action runs once, and no F19 or Backspace reaches the app. Drag the menu to another monitor, close it,
    open it again: it comes back there. Check a monitor with a different scale. Wait 8 s: it closes.
13. Quick menu profiles: switch to ChatGPT and Media and back; confirm K3 still opens the menu.
14. Knob turn: choose Unified Control, Scroll. Both directions are set (knob left is the first real F24 set).
    Check one notch per click in both directions with the menu closed, one menu step per click with it open,
    no scrolling behind the menu, then Volume. Then Factory scroll: confirm the n 0 write restores the
    knob's own scrolling.
15. Claude sessions: Connect, and check the backup and the four entries in `.claude\settings.json`. Send a
    prompt in a Claude desktop session: "Last event received" updates, and the session appears with its
    project. Confirm Claude is not slowed and shows nothing from the hook. From the quick menu, open the
    session (with and without the follow-up dictation). Confirm Claude Code accepts the exec form
    (`command` plus `args`); if not, the hooks do nothing and "No event received yet" stays. Disconnect: the
    file is back as before. Uninstall with Claude sessions connected (on a test install): the hooks are
    removed and Claude shows no hook error.
