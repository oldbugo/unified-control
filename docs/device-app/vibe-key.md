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
  1.5 s after it woke and 1.5 s after the last key or knob event. UC applies the same wait to every
  key-typing command (cmd 0x50), reads included, and to "Read from key".
- Key typing (cmd 0x50) is not part of the reads at connect. A key's list is read once per control, after
  the settle time, before a write UC needs (see the guards under "Two layers"), or when the user presses
  "Read from key" on the page; one key at a time. A change to a key's typing learns the new list from the
  set's echo, so no extra read is sent. The other reads at connect are capped at 10 s in all.
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

Hotkeys are claimed only while the Vibe Key is enabled and a key has an action (the "hotkey ... claimed" log
lines appear only when the claimed set changes). They are released when the
action is set to Nothing, when the key is switched to Types keys or Factory default, on disable and on quit.

Real key, 2026-11-17: once a control types its own code, the key seems to stop reporting that control on the
private channel (K3 on F22 and the knob right on F23 sent no private events). The power button sends nothing
at all (three short presses: only the 10 s status message arrived).

## Message box first (user report 2026-11-17)

With Claude's terminal panel focused (Claude Code desktop), "Start dictation" sent Ctrl + D into the terminal
(which can close the shell), and "Dictate & send" would have sent Enter there (running a command). Now the
actions that type into a message box (Claude: Start dictation, Send message, Dictate & send; ChatGPT: Start
dictation, Send message; and the Target-app actions that become them) first bring the app forward, then find
its message box with Windows UI Automation (`core/au05_composer.rs`), focus it and confirm the focus moved
there, and only then press the keys. Dictate & send does this before each of its keys (Ctrl + D, and Enter).

- The message box: an enabled, focusable, on-screen Edit (or a Document other than the page) whose name or
  automation id names a prompt or message box (prompt, message, reply, ask, chat, composer), and never one
  named terminal, console, xterm, search, find, filter or rename. With no such name, a single editable field
  left counts. Chromium builds its accessibility tree when first asked, so UC looks a second time after 0.3 s.
- Not found or not focused: nothing is pressed, the key's event shows "Couldn't find Claude's message box"
  (or ChatGPT's), and the log says why, with up to eight editable elements seen (type, a 40-character label
  and automation id; never their text). The element found is logged once per run so the search can be
  tuned on the real app.
- The search is tested on a list of elements (terminal next to the prompt box, only a terminal, the page,
  two unnamed fields); the fake desktop models a missing message box (nothing pressed). The real Claude and
  ChatGPT windows are an open check.
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

## Page layout (user feedback 2026-11-17: "make it a little bit more visual ... like a card, group it well")

- Profiles are a row of cards above the drawing (colour dot, name; the active one highlighted, with rename and
  remove; "+" adds a copy). The target app (Claude / ChatGPT) is a two-way switch beside them.
- The drawing stays the anchor (it stays in view while the panel scrolls). The panel for a control has a
  three-way switch (UC action / Types keys / Factory), status pills (Ready, Waiting for key, Key updated,
  Failed with Try again, Not set up with Set up) and the actions as cards grouped by app (Claude, ChatGPT,
  Target app, Windows & media, Unified Control). Each card has an icon (`public/js/au05-icons.js`: simple line
  glyphs, no app logos; app sections use an initial and a colour token), a short name, the shortcut in small
  text and a "?" badge when unverified; details are in the tooltip.
- The knob turn panel: UC action / Factory scroll, then Scroll / Volume / Nothing cards and one line per
  direction.
- Quick menu entries are cards: drag handle (same level), icon, name, a type badge (Action, Profile, App,
  Sub-menu, Sessions) that opens the same card picker to change it, and small move / add-inside / remove
  buttons. "Add entry" opens the picker.
- Claude sessions is one card: status dot, Connect / Disconnect, last event, sessions as chips by project,
  and "What Connect adds" behind a disclosure.
- Device settings are cards (Power, Light, Microphone, Controls) with short labels; raw values in a small
  muted line. Recent events are folded away.
- Colours come from tokens only (`--hue-1..6`, `--hue-claude`, `--hue-chatgpt`, `--hue-system` added); UC is
  dark only, so a later light theme needs only new token values.

## Claude "Dictate & send" (user request 2026-11-17; marked New until confirmed)

A Claude action next to plain "Dictation". It never counts presses: each press decides from Claude's real
microphone state (`au05_hotkey::dictate_send`).

- **Dictating?** Windows Core Audio (`core/audio_capture.rs`): an Active capture session on any active
  recording device whose process is `claude.exe` (any of its processes). Read only. UI Automation was not
  added (the capture state proved enough in tests; it can be a later tiebreaker).
- **Not dictating:** bring Claude forward, Ctrl + D, then confirm within 1.5 s that the capture started. If
  not: "Dictation didn't start" (logged, the key flashes red, a short notice). Never a second Ctrl + D.
- **Dictating:** bring Claude forward, Ctrl + D, wait up to 1.5 s for the capture to stop, then 0.8 s for the
  transcription to settle, check Claude is still in front, then Enter. If the capture did not stop or
  Claude is no longer in front: no Enter, "Not sent".
- **Guards:** a second press within 400 ms is ignored; one run at a time (a press while it runs is ignored);
  if Claude cannot come forward, nothing is pressed. Each decision is logged (state seen, keys sent, result;
  no typed text).
- The fake desktop models Claude's capture (start, stop and send, missed start, stop that does not end,
  focus lost: one test).

## Two layers: with Unified Control and on its own (user request 2026-11-17)

The key stores one list per control (what it types). The page edits two layers, switched above the drawing:

- **With Unified Control** (live): each key has a UC action (palette) or "Same as on its own"; the knob turn
  has UC codes (scroll, volume, nothing) or "Same as on its own". Profiles belong to this layer.
- **On its own** (offline, `au05.offline` in settings): what each of the six controls types without UC
  (another PC, or after Quit): plain keys or the factory default (knob turns: factory scroll or keys).

UC keeps the key holding the right layer (`Worker::sync_layers`, every tick): while it runs, a control with a
UC action is set to its own code and every other control to its offline typing; only controls whose stored
list differs are written, through the usual key jobs (one key command at a time, read back from the set's
echo, time limits, settle after wake, no retry). Each panel shows "On the key now: …".

Guards on key-typing commands (cmd 0x50; independent review 2026-11-17):

- **Read before write.** A list not known in this run is read first (one at a time, 20 s slow wait) and
  written only if it differs. Only lists UC would write are read: a control with a UC action, one whose layer
  changed in this run, or one whose saved confirmed list differs. The first `configure` of a run marks
  nothing as changed, so a start sends no blind writes.
- **Saved confirmed lists** (`au05.keyEcho`): each list the key confirms (a read, or a set whose echo
  matched) is saved, so a later start neither reads nor writes a control that already holds what UC wants.
  (If another PC changes the key meanwhile, UC does not notice until that control's layer changes or the
  user presses Read from key.)
- **Echo check.** A set whose echo differs from what was written ("The key stored something else") fails
  and is never repeated automatically; one automatic attempt per control and wanted list, then only when
  the wanted list changes or the user presses Try again.
- **After a missed reply.** Any other key-typing failure (no reply, write timeout) pauses all automatic
  key-typing commands until Reconnect or Try again (the page shows "Key changes paused: use Reconnect"); a
  queued newer choice is dropped, and the set's result is treated as unknown.
- **Unchecked controls.** Knob press and knob left are not checked on the real key: UC never writes them by
  itself; they show "Needs a check" with "Set up" (also while key changes are paused; marked "!" on the
  drawing; logged once as "layer sync: knobPress needs Set up"), which allows them: one read, then one set.
  Real-key bug on 0.24.0: a check skipped while key changes were paused never came back, so the knob press
  with "Open quick menu" kept typing Backspace and the page showed nothing. Once a set-up write's echo matched, the
  control counts as checked on this key and is saved (`au05.setUp`).
- **Keeping the user's keys (decision).** Until a control's offline typing is set by the user (or is not the
  factory default), nothing is written for it. When its list becomes known (read at start, or "Read from
  key"), that list becomes its offline typing (saved, `offlineSet`); one of UC's own codes F19–F24 counts as
  the factory default. So the migration and "Read from key" never write the user's keys away. A seed never
  overwrites a choice the user made meanwhile.
- **Quick menu.** While any key opens the quick menu, the knob press is on its own code (F19) in the live
  layer, as the hotkeys need it to choose.

The offline layer goes back on the key at: Quit (up to 8 s), disabling the Vibe Key (up to 8 s), Windows
session end (a hidden window on the hotkey thread gets WM_ENDSESSION; up to 4 s), and "Store offline layer on
the key now" (until "Back to Unified Control" or the next start). Only an awake key is written; otherwise, or
when time runs out, settings keep `offlinePending` and the page shows "Offline layer not stored yet".
"Stored" is reported only when no key job waits or is being sent and no key-typing command is unanswered. A
command already sent is always waited for within the budget; when the budget runs out no new one starts, and
one still unanswered is saved as `slowAbandonedAt`: the next run waits 20 s after that time before its first
key-typing command (the page shows "Waiting for the key to finish an earlier change"), so two never overlap
on the key. The tray Quit stops the cooler first (if it refuses, the app stays open with the Vibe Key still
running), then the Vibe Key, and only then arms the 3 s exit watchdog. Any other exit stores the Vibe Key
before the watchdog, at most 3 s (so Windows does not report UC as blocking shutdown). Once exiting, a late
settings save never starts the worker again. Session end does nothing while the Vibe Key is disabled; if UC is
still running a minute after a session end, key changes may start again, and Reconnect also lifts that stop.
Stopping the worker forgets what the key holds. If the
PC loses power while UC runs, the key keeps F19–F24 (harmless) until UC runs again; the page says so in one
line.

Actions on its own (user direction 2026-11-17: no more key-side fixes; the offline layer is not a priority):
the "On its own" panel offers the catalogue actions the key can store as plain keystrokes
(`au05_hotkey::offline_keys`: key combinations the key accepts, such as Claude dictation Ctrl + D or Send
Enter; without UC they go to the app in front). The others (media keys, Dictate & send, Target-app actions,
combinations with keys the key cannot store, the quick menu, profiles, Claude sessions) are listed greyed
under "Needs Unified Control". In the live layer, a key with such an action (or a custom combination) shows
"Also use on its own (keys)", which copies the keystroke into that control's offline layer. Every control,
knob press and knob left included, is set up from the page (Set up); no scripts.

Settings from before: a key action "none" (no UC action) became "offline" once (`au05.layers: 1`); the
explicit UC "Nothing" stays "none" after that.

## Switched off (real key, 2026-11-17)

After its power-off time (3600 s by default) the key switches itself off; a key press does not wake it, only
holding the power button on its right side for 2–3 s does. The receiver stays connected and silent. UC shows
"The Vibe Key is switched off or asleep. Hold the power button on its right side for 2–3 s" (state `off`,
also on the Home card) when nothing has arrived since the connection opened for longer than the standby time
+ 2 minutes, or nothing for longer than the power-off time. Any message from the key clears it. The power-off
setting has a short note that "Never" avoids this at some battery cost; UC does not change it.

## Battery (user request 2026-11-17)

- Above the drawing: a battery icon with its fill level and the percentage; warning colour at 15 % or less;
  "Full" at 100 %; "Asleep — last known 30 %" while the key sleeps; on hover: volts, the time of the last
  report and the raw status flags. The Home card's status chip shows it too ("Connected · 30 %").
- The periodic status message (class 0x7B) carries millivolts, percent and a flags byte. Real key
  2026-11-17: 0x08 at 30 % while it was most likely charging; `0b 7b 1e 10 00 64` (4126 mV, 100 %, flags
  0x00) full and unplugged. So bit 0x08 is treated as "charging": the page shows the bolt and the tooltip
  says "probably charging (charging bit not confirmed yet)". Every change of the flags is still logged
  ("battery status flags 0x08 -> ...").

## Action palette (user request 2026-11-17: drag an action onto the key)

- The action cards are a persistent "Actions" palette beside the drawing (below it on narrow windows). It can
  be folded; the choice is remembered on this PC (browser storage, `uc.au05.palette`).
- **Drag** a card onto K1, K2, K3 or the knob press (the knob ring accepts only the "Knob turn" cards: Scroll,
  Volume, Nothing). While dragging, every part that accepts the card is outlined, the one under the pointer
  glows, and other places show a "not allowed" cursor. The drop gives that part the action at once, in the
  active profile, and flashes it. A key in "Types keys" or "Factory" becomes a UC action, with the usual key
  set-up and its status pill. Dragging uses pointer events (mouse or pen), not HTML drag and drop, which did
  not start from buttons in the app's WebView; Escape cancels. On touch, cards are tapped.
- **Click** (or Enter on a focused card) gives the card to the part selected on the drawing. The selected
  part's current action is marked in the palette; cards that do not fit the selected part are dimmed.
- **Undo:** each assignment shows "K2 · confirm: Send · Undo" for about 6 seconds; Undo restores the previous
  action (and the previous key mode, or the previous knob turn choice).
- A card dropped on a quick-menu entry replaces that entry; dropped on "Add entry" it adds one (catalogue
  actions only).
- The panel for the selected part keeps the mode switch, the status pills and a summary of the current
  action; it no longer has its own card grid.

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
- **Move and choose.** Knob turns move the highlight (right = down). Since 2026-11-17 K2 selects and the knob
  press closes (see "Menu controls" below); before, the knob press chose. The knob press
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

### Menus per profile, Close/Back and menu controls (user requests 2026-11-17)

- **A menu per profile.** Each profile has its own menu (`au05.profiles[].menu`; `au05.menu` is the current
  profile's). Settings from before had one menu: it was copied to every profile. The page edits the current
  profile's menu ("Quick menu for the profile …"); switching profile switches the menu; a new profile starts
  with a copy of the current menu.
- **Close and Back.** "Close" is always the first row at the top level and "Back" the first row inside a
  sub-menu (fixed, not stored, not removable). The menu opens on the first entry after Close.
- **Menu controls** (`au05.menuControls`, the page's "Menu controls"): while the menu is open the knob turn
  moves; K2 selects, K3 goes back (at the top: closes), the knob press closes without doing anything
  (defaults; each can be Select, Back, Close or "Its own action"). Their normal actions wait while the menu is
  open and work again once it closes. This replaces the earlier "knob press chooses".
- **Own codes for K2 and K3.** While any key opens the menu, K2 and K3 with a menu role type their own codes
  (F21, F22) in the live layer and UC claims them, even when their action is "Same as on its own" (otherwise
  K3's factory Esc would reach the focused app). While the menu is closed UC then types their usual keys for
  them (their offline keys, or the factory key: K2 Enter, K3 Esc). The page says this in bold under Menu
  controls.
- **The menu window.** The header shows the profile and the target app; rows are larger, the highlighted row
  is bold with a stronger outline, and the footer shows the hints ("Turn: move · K2 select · K3 back · Knob
  press close").

### Session tree and navigation (user decisions 2026-11-17)

- **Tree from folders.** Claude gives no parent-session id, so the "Claude sessions" sub-menu is built from
  folders (`claude_sessions::locate`): level 1 is the project, the repository root (a `…\.claude\worktrees\<name>`
  folder, or a worktree anywhere whose `.git` file points into `<repo>\.git\worktrees`, belongs to `<repo>`),
  shown by its folder name, most recent activity first. Under a project: the session in the main checkout first,
  then the worktree sessions, most recent first, each by its title (else its first prompt, else the worktree
  folder). A green dot marks sessions working now. Nodes are generic (`entries`), so a real parent link can nest
  sessions later (`au05_menu::session_tree`).
- **Navigation** for the whole menu: the knob moves up and down, K2 goes into the highlighted entry (on a session:
  opens it in Claude), K3 goes up one level (at the top: closes). No wrap-around: at the first or last row,
  further turns stay put.
- **K1 on a session** (or on a project: its main session): Claude opens the session; once Claude is in front (at
  most 1.5 s, else nothing is pressed) the dictation runs there through the message-box guard: Dictate & send if
  that is K1's action, else Start dictation. The menu closes. On other entries K1 keeps its own action.
- The Claude sessions card says that sessions appear once they start (or restart) after Connect, because Claude
  reads its hook settings when a session starts.

## Claude sessions (2026-11-17)

The user chose Claude Code hooks. Opt-in: "Connect Claude sessions" on the Vibe Key page.

Session titles (2026-11-17): a session named in Claude has `{"type":"custom-title","customTitle":"…"}` lines
in its transcript. On each event the hook proxy reads only the end of `transcript_path` (only a file named
`…\.claude\projects\<project>\<id>.jsonl`, not a link, its last 256 KB, at most 50 ms) and sends only the last
custom title (control characters removed, at most 120 characters). Nothing else in the transcript is read or
sent. UC keeps the latest title and shows it (else the first prompt, else the folder name), grouped by project,
in the menu and on the page. Titles and prompts are never logged.

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
16. Dictate & send on K1: press once (Claude comes forward, dictation starts), speak, press again (dictation
    stops, the text is sent once). Press quickly twice: nothing odd. With Claude closed: nothing is pressed.
    With the microphone muted in Windows: "Dictation didn't start". Watch the Vibe Key log lines.
17. Battery: plug in the charger, then unplug; note the "battery status flags" lines in the log and check
    the bolt follows bit 0x08 (then mark it confirmed).
18. Message box: in Claude, focus the terminal panel, then press Start dictation: dictation starts in the
    message box, nothing reaches the terminal. Note the "Claude message box: ..." log line (type, name). Do
    the same for Dictate & send (the Enter goes to the message box) and for ChatGPT's Start dictation. If the
    page says "Couldn't find Claude's message box", send the logged element list to tune the search.
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
