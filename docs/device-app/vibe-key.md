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
- On a real key, UC sends only the sets that were checked on the real key: standby time and what a key
  types. The other sets (power-off time, light mode, brightness, microphone noise reduction) work on the
  fake key only. The page shows them, read-only, with a note.
- Every write has a 2 s limit. One request is in flight at a time. Nothing is sent until the key has sent
  a message in the last 15 s, because a write to a sleeping key blocks. A missing reply gets one retry.
  A write that does not finish is not retried. After a stall, sending pauses for 30 s. After three stalls
  in a row, sending stops until the user presses Reconnect.
- The noise-reduction set writes back the two unknown values exactly as read.
- While sending is paused, a new change is refused at once with "Sending is paused".
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
| Start dictation in Claude | Find the visible window of `claude.exe` whose title starts with "Claude", restore it, tap Alt, `SetForegroundWindow`, wait until it is in front (0.6 s), then Ctrl + D | Prototype `au05_k3_hotkey.py`, user-confirmed |
| Start dictation in ChatGPT | The same with `chatgpt.exe`, title "ChatGPT", then Ctrl + Shift + D | Not checked (exe name, title and shortcut) |
| Send (Enter) | Enter in the focused app | Not checked |
| Custom key combination | That combination in the focused app (the dock's reviewed hotkey sender) | Not checked |
| Nothing (harmless) | The key types its own code; nothing happens | K3 types F22, user-reported |

Everything that touches the desktop is behind the `Desktop` trait. Unit tests and fake instances
(`UC_FAKE_DEVICE`) use `FakeDesktop`: it claims no hotkey, moves no focus and sends no input. The fake key
delivers a press to it only when the fake key types that key's own code, like Windows would.

## What the page offers

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

## Tests

- Driver: `cargo test -p ulanzi_au05` in uc-ulanzi (20 tests, fake key and pure functions).
- Worker and hotkey layer: `cargo test --lib au05` in `src-tauri` (11 tests, fake key and fake desktop).
- Page: `node scripts/check-vibe-key.cjs` (headless fixture; set `UC_PLAYWRIGHT_MODULE`).
- Native: `node scripts/check-native-vibe-key.cjs` (quiet debug app with the fake key; also set `UC_TEST_EXE`).

## Open checks on the real key (orchestrator, user present)

1. Enable in General. Confirm that UC opens only the private channel and shows "Connected" after a key
   press, with the battery value.
2. Confirm all reads in one session: standby 600 s, power-off 3600 s, light, mic, K1..K3 typing.
   Watch for the 15 s key-typing read seen once on 2026-11-16.
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
