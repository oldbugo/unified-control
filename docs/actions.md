# Dock actions

`core::actions::ActionSink` is the dock service's execution boundary:

```rust,ignore
fn execute(&mut self, action: &PendingAction) -> Result<(), String>;
```

`NativeActions::new(resource_dir, app_home, fake)` constructs the Windows adapter
without doing any I/O. The resource directory must contain `monitor/` with the
two helper files described below. A simulated service must use a fake sink;
`NativeActions` additionally refuses all effects when its `fake` flag is true.

`list_monitors(&resource_dir)` returns `Vec<MonitorInfo>` with serialized
`{key,label}` fields. It invokes the trusted helper in read-only `list` mode;
the simulated UI must return its fixture without invoking this native helper.

Only an enabled real dock key may call the native sink. Editing, importing,
loading or previewing a key must never execute it. The editor gets no generic
execute-action command. The caller runs actions serially on its worker, reports
an error with a key flash and a redacted log line, and treats execution as busy
work for disable/quit/update. Do not record action values in logs.

## Supported configuration

Every value is at most 4096 UTF-8 bytes with no null character. Unknown actions,
invalid configuration and unsupported navigation calls fail before execution.
Page, folder and back actions are handled by the page runtime, not this sink.

| Action | Value |
| --- | --- |
| `none` | Ignored |
| `hotkey` | `Ctrl + Shift + M`, `Win + D`, `Alt + F4`, or another supported chord |
| `media` | `Play / pause`, `Next track`, `Previous track`, `Volume up`, `Volume down`, `Mute` |
| `text` | Nonempty literal Unicode text |
| `launch` | Absolute Windows path, or JSON `{"path":"C:\\Apps\\App.exe","args":["argument one","argument two"]}` |
| `url` | An `http://` or `https://` URL, without credentials or whitespace |
| `monitor` | JSON `{"targets":["target-id:1"],"mode":"toggle"}`; targets are explicitly selected display keys, not imported settings |

Hotkeys support Ctrl/Control, Shift, Alt and Win/Windows/Super modifiers with one
main key. Main keys include letters, digits, F1–F24, Numpad0–Numpad9, navigation,
editing, space/tab/enter, common punctuation and keypad operations. Use `Plus`
for the plus key. The runtime does not synthesize the protected Ctrl+Alt+Delete
sequence. Other Windows-reserved combinations and elevated foreground apps may
reject injected input; no elevation is requested to bypass this.

An already-held requested modifier is not injected or released. An unrelated
held modifier causes a hotkey to fail, so an ordinary letter does not become an
unexpected shortcut. Text typing also requires modifiers to be released. Input
is submitted as one ordered batch. Partial delivery triggers a release batch
for only the injected keys known to remain down. Failure to release every key
is reported explicitly. Unicode text is sent as UTF-16 events, including
surrogate pairs. No clipboard contents are read or changed.

Launch paths must be absolute; environment expansion, command-line parsing,
wildcards and script-file launch are not supported. Executable paths use direct
process creation with an argument array, without a command interpreter or UAC
elevation verb. Documents, folders and URLs use the Windows `open` association
with error dialogs suppressed. Security prompts and the launched application's
own UI remain controlled by Windows/the application. Arguments are supported
only for `.exe`/`.com` apps and are passed as literal arguments. Launched user
apps remain independent of UC and are not put in UC's kill-on-close child job.

## Monitor Toggle

The two helper files in `src-tauri/resources/monitor/` are copied byte-for-byte
from reviewed Toolkit source `bb2825d4a68883656560a8c063706f4709f8f1d5`:

- `plugins/monitor-control/com.ulanzi.monitortoggle.ulanziPlugin/scripts/WindowsDisplayControl.ps1`
- `plugins/monitor-control/com.ulanzi.monitortoggle.ulanziPlugin/scripts/WindowsAudioDefaults.cs`

SHA-256:

```text
WindowsDisplayControl.ps1 a01a381fb0829ceea243c61cc26d4fe5662d77cbd49dabab6831ff116ba5977f
WindowsAudioDefaults.cs    3322fec4be2ec3d857ccc35f571ccffdf2d34dfe09b29240bc23b555df03b5ed
```

No SDK, personal settings, target IDs, installed plugin files or saved display
snapshots are copied. The built-in action has its own snapshots under
`<app_home>/monitor/`, keyed by the normalized target group. Allowed modes are
`toggle`, `enable` and `disable`. Up to eight distinct targets are supported.
Unconfigured keys fail with a setup message. Enabling needs a snapshot previously
created by disabling that same target group; UC does not borrow Toolkit's private
snapshots or silently search another app's state.

The original helper refuses disabling every active display. Display operations
preserve the separately captured playback, multimedia and communications audio
defaults through its existing bounded observation and one-pass repair. No
hard-coded DAC, microphone, volume, driver or background audio policy is added.
Audio warnings cause a visible action error rather than silent success.

Monitor operations are serialized globally within UC. Windows PowerShell is
started hidden with `-File` and explicit arguments for these trusted bundled
helpers. No user-provided PowerShell or shell command is executed. The action
waits for the helper, including audio cleanup, and never kills it midway through
the transaction. A stuck driver/COM call can therefore take longer than the
audio observation window. The caller must leave the app responsive and keep
quit/update waiting for the running action. Hardware reliability must be checked
with the user; a fake test does not establish physical behavior.

## Validation

Unit tests use `ActionPlatform` fakes only. They check key/media dispatch,
modifier preservation, partial-input cleanup and cleanup failure, Unicode,
literal arguments, launch error propagation, path/URL bounds, explicit monitor
configuration and the native fake-device guard. The copied audio helper also
passes the original 16 fake-backend scenarios without Windows audio or display
changes. No real keyboard input, launch, monitor toggle or audio setting is
used as an automated check.

Checked: 12 targeted Rust unit tests pass, and all 16 original fake-backend
audio scenarios pass against the copied C# helper. The Windows native code
compiles; real action execution and physical monitor/audio behavior are pending
manual acceptance. Helper copies were hash-checked against their source.

Primary interface references: [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput),
[KEYBDINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput),
[ShellExecuteEx flags](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shellexecuteinfow).
