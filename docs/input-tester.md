# Input tester
*Document type: reference. It describes how the part works. For the current state, read [status](status.md).*

The Input tester is a page at the bottom of the sidebar. It is always there; it is not a device. Use it to check keys, mouse buttons, wheels, knobs, microphones and speakers without a website. It was made for the Vibe Key (AU05), but it works with any keyboard, mouse or HID control.

## What the page shows

- **Keyboard.** A full keyboard drawing (ANSI or ISO), with F13–F24, the arrow and edit keys, and a number pad you can hide. Mouse buttons, wheel directions and media keys have their own rows. A key is lit while it is held. After you release it, it keeps an outline until you press Clear. Keys that the drawing does not show appear under "Other".
- **Last combination.** For example `Ctrl + Shift + D`. Modifiers alone also show (`Ctrl + Shift`).
- **Vibe Key.** The six codes the Vibe Key types when a control is set to a Unified Control action: K1 F20, K2 F21, K3 F22, knob press F19, knob right F23, knob left F24. Each shows if Unified Control claims it as a hotkey now.
- **Devices.** Every keyboard, mouse and HID control (media keys, system keys, game controllers) with its product name and VID:PID. A dot lights when the device sends something. Counts show keys, repeats, clicks, wheel turns, HID changes and mouse moves. Select a device to show only its events.
- **Event log.** Newest first: time (ms since the page opened), gap to the event before, device, type (key down, key up, repeat, button, wheel, HID press, HID release, HID report, connect, disconnect), key, virtual-key code, scan code, extended (E0) flag, modifiers and a note. Pause stops the log only (the drawing and counts go on). Copy puts the shown events on the clipboard as tab-separated text. Mouse moves are counted always and logged only when "Mouse moves" is on.
- **Microphone.** Choose a microphone (the Windows default first; the Vibe Key shows as "Microphone (AU05)"). Start shows a live level, a peak hold, the highest peak and clipping. It also shows the format Windows gives to the app: sample rate, channels and bits. "Echo, noise and gain processing" is off by default, so you hear the raw microphone. Record takes 5 seconds; Play back plays it through the chosen speaker.
- **Speakers.** Choose a speaker and play a soft 440 Hz tone for one second: left, both or right.

## How it works

- Events come from Windows Raw Input (`src-tauri/src/core/input_tester.rs`). UC makes a hidden window that is never shown or focused, and registers it with `RIDEV_INPUTSINK` for keyboards, mice, media keys, system keys, joysticks and game controllers. Raw Input only reads. It does not block or change input for other apps.
- UC listens only while the page is open and the window is visible. It stops when you leave the page, close the window to the tray, minimise it, or when the page stops asking for 10 seconds.
- Each Raw Input event carries a device handle, so each event names its device. Product names come from Windows (the USB product string or the Bluetooth device name).
- A key that UC holds as a global hotkey (Vibe Key actions) never reaches other apps. Raw Input still sees it. The log marks such a key press "Claimed by Unified Control".
- Input sent by software (for example a hotkey sender) has no device. It shows as "Software (no device)".
- Audio runs in the page (Web Audio). No sound leaves the PC. A recording stays in memory and is gone when you leave the page. It is written to disk only when you press Save: a WAV file in your Downloads folder.
- In a browser preview (no app) the page sees only its own window's keys and has no device names.

## Tests

- Rust unit tests in `input_tester.rs`: key decoding (positions, names, F13–F24, left and right modifiers, E0 and E1 keys, the fake Shift around Print Screen), mouse buttons and wheels, HID usage presses and releases, repeats, combinations, claimed hotkeys, the bounded log, device changes, and WAV saving. All use fake records.
- `scripts/check-input-tester.cjs` (headless): the pure page logic, then the page against a fake host, then a browser preview. It checks the drawing, combination, log, filter, pause, copy, clear, the Vibe Key card, simulated audio (no microphone, no sound), leave and return, the hidden-window pause, and a narrow window.
- `scripts/check-native-input-tester.cjs` (quiet native): the real debug app with fake devices. Fake raw records go through the real decoder. It checks that a quiet fake instance never registers Raw Input.
- `cargo test --lib list_real_input_devices -- --ignored --nocapture` lists this PC's input devices by name. It is read-only: it opens no device and registers nothing.

Test instances (`UC_FAKE_DEVICE` or `UC_TEST_QUIET`) never register Raw Input and use simulated devices and simulated audio.

## Check on the real PC

1. Open Input tester. The chip says "Listening to all devices". The Devices list shows the AU05, the main keyboard and the mouse by name.
2. Type on the main keyboard with another app in front: events still arrive, each with the keyboard's name.
3. Press each Vibe Key control. With a Unified Control action set, the key press shows "Claimed by Unified Control". Check which codes the knob sends (keys or HID media usages).
4. Close the window to the tray, then open it again. The chip says "Paused while the window is hidden" while it is hidden (check `device.log` for "raw input off"), and listening starts again when it is shown.
5. Microphone: choose "Microphone (AU05)", allow the microphone when asked, and look at the level and format. Record, play back, and compare with processing on and off.
6. Speakers: play the left and right tones on each output.
7. While the page is open, check that the Vibe Key actions and other hotkeys still work, and that the rest of UC behaves normally.

Known limits: Raw Input registration is one per process for each device type. While the page is open, UC's hidden window owns keyboard and mouse Raw Input for the UC process. Nothing else in UC uses it now. Windows may ask once for microphone permission.
