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
- A UC action runs on its own thread, never on the device thread. A crash in an action does not stop the
  worker.

## What the page offers

- Connection state, battery and a Reconnect button.
- For K1, K2 and K3: what the key types. Choices: factory default, Harmless (the key types its own
  F20/F21/F22 code, which common apps ignore), or a key combination such as Ctrl + D. This is stored on the
  key and works in every app, also when UC is closed.
- For each control (three keys, knob press, knob left, knob right): a UC action when it is pressed. Kinds:
  hotkey, media key, type text. These use the same reviewed executor as the dock keys. They are stored in
  `settings.json` (`au05.actions`).
- Standby time and power-off time, light mode and all-on brightness, microphone noise reduction.
- A live list of key and knob events.
- "Light follows Claude Code" is shown as planned. The code has a stub only (`agent_light_requests`
  returns nothing).

## Tests

- Driver: `cargo test -p ulanzi_au05` in uc-ulanzi (20 tests, fake key and pure functions).
- Worker: `cargo test --lib au05` in `src-tauri` (7 tests, fake key).
- Page: `node scripts/check-vibe-key.cjs` (headless fixture; set `UC_PLAYWRIGHT_MODULE`).
- Native: `node scripts/check-native-vibe-key.cjs` (quiet debug app with the fake key; also set `UC_TEST_EXE`).

## Open checks on the real key (orchestrator, user present)

1. Enable in General. Confirm that UC opens only the private channel and shows "Connected" after a key
   press, with the battery value.
2. Confirm all reads in one session: standby 600 s, power-off 3600 s, light, mic, K1..K3 typing.
   Watch for the 15 s key-typing read seen once on 2026-11-16.
3. Set K1 to Ctrl + D and K3 back to Harmless from the page. Read back. Confirm that the keyboard part
   types what the page says, and that no modifier stays down.
4. Confirm that a key's private event still arrives when it types F22, and that a UC action runs once.
5. Let the key sleep. Confirm that a change waits ("asleep") and sends nothing until a key press.
6. Unplug and plug in the receiver. Confirm the reconnect.
7. One by one, check the unverified sets (power-off time, light mode and brightness, mic noise reduction).
   After each check, add the set to `Request::verified_on_hardware` in the driver.
8. Decide what K3's UC action "send now" means in Claude (a hotkey), and which key starts voice input
   in Claude and ChatGPT (Ctrl + D and Ctrl + Shift + D are reported, not confirmed).
