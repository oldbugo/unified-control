# Changelog

## 1.1.0 — 2026-11-18

Several quick menus. K2 now shows the next menu and K3 closes the menu
(both still changeable under "In the menu"; if you had the old defaults,
K2 select and K3 back, they move to the new ones once). The shipped menus
are your profile's menu as a dial and "Recent sessions", a plain drop list
of every open Claude session with the one you last spoke to first and its
project next to it. The "Menus" list on the Vibe Key page adds, renames,
reorders and removes menus and sets each one's style (Dial or List) and
what it shows. The menu opens on the menu you last used. Tab does the same
in the menu window.

The dial is now one helix. The highlighted entry sits at the top; the
entries after it wind anticlockwise and deeper, the ones before it grow
outward and fade. Every turn turns and grows the whole spiral one step, so
the next entry comes up from the depth. Going into a sub-menu the spiral
spins and sinks into the middle while the new level spins in from outside;
Back does the reverse. There is no end marker and no eight-entry window any
more: a long list simply goes deeper. Labels are fitted where each segment
rests, upright.

A thin bar under the caption runs down to the automatic close (8 s without
input). The hint line is gone.

The 1.0.0 folding into "More" is now off unless you pick a time ("Fold
sessions after", default Never).

## 1.0.0 — 2026-11-18

The quick menu's Claude sessions put what you use first. At every level
(projects, sessions, sub-sessions) the one you last spoke to comes first, so
going back and forth between two projects stays one or two turns away. "Spoke
to" means you sent the session a message (Claude's hook event; with Live working
status off, Claude's own last-activity time is used). Projects now follow this
order instead of the Claude sidebar order.

Sessions you have not used for a while fold into a "More (n)" entry at the end
of their list; open it and they are all there, in the same order. At least
three stay shown, a working session is never folded, and a single old entry is
never folded on its own. "Fold sessions after" (In the menu) picks 1 hour, 4
hours, 8 hours (default), 1 day, 3 days or Never.

Fixed: lists on the Vibe Key page lost your choice when the page refreshed
while the list was open (the battery and Claude session counts refresh it every
few seconds). This is why noise reduction levels 0, 2 and 3 never reached the
key: only one change, to level 1, was ever sent. The page now waits while a list
is in use.

## 0.38.0 — 2026-11-17

The quick menu dial is one spiral now, like a nautilus shell. The outer edge
falls smoothly clockwise from Close/Back, the largest, to the last entry, the
smallest, so every segment lines up with the next. The step back up to
Close/Back at the top marks the end of the list. The gaps between segments are
the same width from the centre to the rim. The highlighted entry grows outward
along the spiral instead of moving sideways, so it stays lined up with its
neighbours.

## 0.37.0 — 2026-11-17

Changes on the Vibe Key page now reach an awake key. Before, Unified Control
only sent a change if the key had said something in the last 11 seconds, but
the key is silent when idle, so a change made a little later was refused even
though the key's lights were on. Now a change is sent while the key's own
timers say it is awake. If a send ever stalls that way, Unified Control goes
back to the careful rule for five minutes. If the key really is asleep, the
change waits with "Touch the Vibe Key to apply" next to the setting and is sent
when you touch it; after a minute it says "Not applied: the key was asleep".

After each change the key's value is read back: the setting shows "Applied",
or what the key kept if it differs, and the value is written to the log.

The light mode is now four small pictures of the key (Off, All, Dial, Keys)
with the lit parts glowing, as checked on the real key. Brightness is a slider
that sends when you let go. The mode records are folded under "Advanced", the
Controls card appears once the codes are read, and Recent events shows only
when there are some. The note under the title no longer repeats in the key
card.

The quick menu dial uses the whole circle now: the entries share the ring
equally (four entries are a quarter each), the ring is about half as thick
again and the dial is bigger, so names have room. Names are always level and
readable, never turned or upside down, on up to three lines; a name that still
doesn't fit shows its most telling words, and a very long single word is split
with a hyphen instead of cut off. The stick that marked the end of the list is
gone: Close or Back is now the largest segment, each next one is a little
smaller and the last one a notch smaller, so the step back up to Close or Back
shows where the list starts and ends. The segments also get slightly darker
toward the end of the list.

## 0.36.0 — 2026-11-17

The Vibe Key no longer shows "asleep" while you are using it, for example
during a long dictation. The page now follows the key's own timers: it counts
as awake until its standby time has passed since your last press, turn or
message from it, then asleep, and "probably off" once its power-off time has
passed. Any press wakes it on the page at once. The key's standby and power-off
times are remembered, so this is right straight after a restart too.

The Vibe Key page is tidier. The receiver, the key and the battery now show
together in one small status line above the drawing, with a coloured dot each,
instead of in three places. The long explanations are gone (a few are tooltips
now). The quick menu's controls sit in one row, "In the menu", next to
"Reopen where I left off". The Claude sessions card now shows what Unified
Control reads, live: how many sessions are open, in how many projects, how many
are working, and the latest few with their project. Connect is now a "Live
working status" switch.

## 0.35.0 — 2026-11-17

The quick menu dial is bigger and its names are easier to read. Names now curve
along their segment, on up to two lines, broken between whole words, and the
text shrinks a little before anything is left out. A name that still doesn't
fit shows its most telling words rather than being cut off, and words that
every entry shares (such as the project name) are left out of the ring. The
full name is always under the dial. A ring shows at most 8 segments, and the
highlighted one now has dark text on the blue.

The dial now shows where the list ends: there is a gap with a small wall just
to the left of Close or Back, and turning past either end gives a little bump
against it. In a long list the wall appears only at the real end; elsewhere
the edge segments fade to show there is more.

Dragging the menu by its middle no longer closes it. Windows takes focus away
from the menu while it is being dragged, and the menu took that as a click on
another window. The menu also no longer closes itself while you drag it.

## 0.34.0 — 2026-11-17

The quick menu is now a dial: a thick ring of pie segments instead of a list.
Close (or Back, in a sub-menu) is always the top segment, and the other entries
go round clockwise; turning the knob right moves clockwise, left goes back, and
it stops at the ends instead of wrapping round. The highlighted segment lights
up and moves out a little, and its full name (with the project and the working
dot for Claude sessions) shows under the ring. When you go into an entry, it
turns to the top and the next level appears as a bigger ring around it; up to
three rings show at once. A level with more than 12 entries shows 12 at a time
and moves along as you turn, with "8 / 31" under the ring. The middle of the
ring is a handle for dragging the menu, and the dial keeps its place on each
monitor. The window around it is see-through. Clicking a segment chooses it
and the mouse wheel turns the dial.

## 0.33.0 — 2026-11-17

The Vibe Key's settings no longer stay "Not read" after you wake the key. Unified
Control used to ask for them the moment the key woke, while it was still busy
waking; the first question went unanswered, sending paused for 30 seconds, and
the questions were never asked again. Now it waits until the key has been awake
for 3 seconds and quiet for 1.5 seconds. If a question still goes unanswered, it
asks once more after the pause, one question at a time. If that fails too, the
page says "Couldn't read the settings" with a Read again button, and nothing
more is sent by itself. When the key wakes after being switched off, the
settings are read again, because they may have changed while it was off.

## 0.32.0 — 2026-11-17

The quick menu now reopens where you left it: the same sub-menu and the same
entry, for example a sub-session deep in Claude sessions, so you can go back
or on from there. It follows the entries themselves, not their position, so
it still finds your session when the list has changed; if that entry is gone,
it opens one level up near where it was. Each profile's menu remembers its own
place until Unified Control restarts. A new setting under the menu controls,
"Reopen where I left off" (on by default), turns this off.

The Vibe Key page no longer says "asleep" while you are using the key: key
presses now count, and it waits about 25 seconds without any sign from the
key before saying asleep. After the key's power-off time it says the key is
probably switched off and how to turn it on. The status now reads "Receiver
connected · Vibe Key awake / asleep / probably off", so it is clear which part
is connected.

## 0.31.0 — 2026-11-17

The quick menu's Claude sessions now use your own projects from the Claude
sidebar ("Unified Control", "Project Nocturne" and so on), in sidebar order,
instead of folder names. Each session sits under its project with its
sub-sessions below it; a sub-session you have not moved goes with the session
that started it. Sessions in no project are under "Other sessions", by folder.
Unified Control reads only the project names and which session is in which
project from the Claude app's settings file, and never writes there. If that
changes in a Claude update, the menu groups by folder again and the Claude
sessions card says so. Pressing the knob in the menu now selects for everyone:
a "Close" saved from the old default is changed to "Select" once; you can still
choose Close on the Vibe Key page and it stays.

## 0.30.0 — 2026-11-17

The quick menu's Claude sessions now list every session you have not archived,
as a real tree: sub-sessions sit under the session that started them, at any
depth, grouped by project and newest first. Unified Control reads only a few
fields (title, parent, folder, archived, last activity) from the Claude app's
own session files and never writes there; it works without Connect (Connect
adds the live "working" dot). If those files change in a Claude update, it
falls back to the hook list. In the menu, pressing the knob now selects (like
K2), and the highlight stops at the first and last entries on every input.

## 0.29.0 — 2026-11-17

Claude sessions in the quick menu are now a tree: each project first, then its
sessions (the main folder's session, then the build sessions working in the
project's worktrees), by title, most recent first, with a dot for sessions
working now. In every menu, K2 goes into an entry (or opens a session in
Claude when there is nothing deeper) and K3 goes back up. K1 on a highlighted
session opens it and starts dictation there. The highlight stops at the first
and last entries instead of jumping around. The Claude sessions card explains
that sessions appear once they start or restart after Connect.

## 0.28.0 — 2026-11-17

Quick menu: each profile has its own menu, and switching profile switches the
menu. "Close" is always the first entry ("Back" inside sub-menus). While the
menu is open, K2 selects, K3 goes back (or closes at the top), the knob press
closes without doing anything, and the knob turns move; K2 and K3 return to
their normal jobs when the menu closes (changeable under Menu controls). The
menu shows the profile and target app at the top and a short hint line.
Claude sessions now show their real titles: the hook reads only the title
line from the end of each session's file and nothing else.

## 0.27.0 — 2026-11-17

Every Vibe Key control can now be fully set up from the Vibe Key page: when a
control (for example the knob press for the quick menu) needs its one-time
check, the page always shows "Needs a check · Set up". The "On its own" setup
offers only actions the key can store by itself (keystrokes such as Ctrl+D,
Enter, media keys, your own combinations); Unified Control-only actions such as
the quick menu or Dictate & send are marked "Needs Unified Control". A small
"Also use on its own" button copies a compatible action across.

## 0.26.0 — 2026-11-17

Cooler engine 4.24.0: the screen's keep-display-on setting (0x15) is sent only
once, at the first Live start after it is installed; every recorded screen hang
began right after that setting was sent again at an update hand-over (one even
after the 30 s wait). Approved by the user. Not yet verified on the screen:
power-cycle the PC after installing, then leave Live on.

## 0.25.0 — 2026-11-17

Claude and ChatGPT key actions now put the cursor in the app's message box
first, using Windows' accessibility interface. If the message box cannot be
found (for example Claude's terminal panel has focus), nothing is pressed and
a short notice says so, so Ctrl+D or Enter never lands in a terminal. The
Vibe Key battery shows a charging bolt (flag 0x08, "probably charging" until
confirmed).

## 0.24.0 — 2026-11-17

Two setups for the Vibe Key: "With Unified Control" (actions while the app
runs) and "On its own" (what the key types by itself on any computer, or with
Unified Control closed). Unified Control puts the "On its own" setup back on the
key when you quit, switch the Vibe Key off, or sign out, and there is a button
to store it now. The first time a key is read, its current keys become its
"On its own" setup, so nothing you set before is lost. Writes to the key stay
careful: one at a time, after a short pause following a key press, checked
against what the key reports back, never repeated automatically after a
problem, and never started while the app is closing. The knob press and the
left knob turn need one "Set up" click before they are changed automatically.

## 0.23.0 — 2026-11-17

New Input tester page (bottom of the sidebar). A keyboard drawing with F13 to
F24 lights up keys as you press them and shows the last combination; an event
log names the device each key, button or wheel event came from (the Vibe Key,
your keyboard, your mouse, any USB input), with counts per device and keys
held by Unified Control marked. A microphone test shows a live level, records
and plays back 5 seconds and shows the format; a speaker test plays a tone left,
right or both. Nothing is saved or sent anywhere unless you press Save.

## 0.22.0 — 2026-11-17

The Vibe Key page now says when the key has switched itself off: "The Vibe
Key is switched off or asleep. Hold the power button on its right side for
2–3 s" (also on the Home card), instead of waiting silently. A short note under
the power-off setting explains that "Never" stops this at some battery cost.
A flaky Vibe Key test was made reliable.

## 0.21.0 — 2026-11-17

Fix: the real Vibe Key's messages never reached Unified Control (key presses,
knob turns and battery), so the key stayed "waiting" and key set-ups never
ran. Unified Control now keeps one read always waiting on the receiver instead
of starting and cancelling a read 40 times a second, and the Vibe Key log now
shows what the receiver delivers (a summary at connect and a heartbeat).
New: "Dictate & send" for Claude. The first press starts dictation; the next
press stops it and sends, but only after checking that Claude really is
recording and that Claude is in front, so a missed press never sends by
mistake. New: a clear battery indicator above the drawing (percentage, low
warning, last known level while the key sleeps); the Home card shows it too.

## 0.20.0 — 2026-11-17

Drag actions onto the Vibe Key. The action cards are now an always-visible
"Actions" palette beside the drawing (fold it away if you like). Drag a card
onto K1, K2, K3 or the knob press and that key takes the action at once; the
knob ring takes the knob-turn cards (Scroll, Volume, Nothing). Keys that can
take the card light up while you drag. Clicking still works: select a key on
the drawing, then click a card. Every change shows a short Undo. Cards can
also be dropped onto the quick menu editor.

## 0.19.0 — 2026-11-17

A clearer Vibe Key page. Profiles are cards along the top, with the Claude or
ChatGPT target beside them. Choosing what a key does is a grid of icon cards
grouped by app (Claude, ChatGPT, target app, Windows & media, Unified Control);
the current choice is highlighted, shortcuts are small text and unconfirmed
ones carry a small "?". Each key has a simple switch (Unified Control action,
types keys, factory) and a small status label. The quick menu editor uses
draggable cards, Claude sessions is one card with a Connect button, and the
device settings are tidy cards. Most help text is gone. No change in behaviour.

## 0.18.0 — 2026-11-17

Open Claude sessions from the Vibe Key quick menu. On the Vibe Key page,
"Connect Claude sessions" (off until you click it) adds four small hook
entries to your Claude settings (~/.claude/settings.json, backed up first;
the page shows exactly what is added). Each Claude desktop session then tells
Unified Control its session, project folder and a short label from its first
message; nothing else leaves Claude. The quick menu lists the sessions by
project and opens the one you pick, optionally followed by an action such as
dictation. Disconnect removes exactly Unified Control's entries; uninstalling
Unified Control disconnects first. No administrator rights are needed; if your
organisation's Claude policy blocks user hooks, the page says so.

## 0.17.0 — 2026-11-17

Vibe Key quick menu. Any key (K1, K2, K3 or the knob press) can open a small
menu window: turn the knob to move, press the knob to choose, press the key
again or wait to close. The menu appears near the top-right of the main screen
by default; drag it anywhere, on any monitor, and it opens there next time.
Menu entries can run an action, switch a profile (a named set of key
assignments), change the target app (Claude or ChatGPT), or open a sub-menu;
edit the menu on the Vibe Key page. The knob turn can now be a Unified Control
control: the knob types its own codes, the menu uses each click, and when the
menu is closed Unified Control scrolls under the mouse or changes the volume.
Factory scroll stays available.

## 0.16.0 — 2026-11-17

Cooler engine 4.23.0. The cooler screen hung three times (6, 11 and 17 Nov)
when Live started again about 8 s after Unified Control stopped it for an
update. Now, after a Live stop, the first new connect waits until 30 s have
passed. This also covers the new cooler worker after an update. The wait is a
Live setting: "Wait before Live starts again after a stop" (no wait, 10, 20,
30, 60 or 120 seconds; 30 by default). The step that switches the screen into
Live may now take up to 2 s (was 0.5 s). The USB commands and their order are
unchanged. Not yet verified on hardware: the next update with Live on is the
test.

## 0.15.0 — 2026-11-17

Choosing a Unified Control action for a Vibe Key key no longer gets stuck.
The action is saved at once; setting the key to type its own code runs in the
background and the panel shows each step ("Waiting for the key: press any key
to wake it", "Sending to the key", "Key updated", or the error with Try again).
Actions are now grouped by app: Claude, ChatGPT, Windows / media, and a custom
shortcut. Actions nobody has confirmed yet are marked unverified. The Vibe Key
has its own log, devices\au05\device.log (no raw device data or typed text).

## 0.14.0 — 2026-11-17

The Vibe Key page now opens on a drawing of the key: knob (press and turn)
and K1, K2, K3, each labelled with what it does now. Click a part to open its
settings in a panel beside the drawing; changes apply at once. A part lights up
briefly when it is used (K2, K3 and the knob press only when they run a Unified
Control action, because they send no message of their own). The other settings
are in a compact Device settings section below.

## 0.13.0 — 2026-11-16

The Vibe Key page now lets you change every known setting on the real key,
for one round of hands-on testing: power-off time, light mode, brightness and
the four light mode records, the Claude status switch (hooks mode), microphone
noise reduction and what the knob press types. Settings not yet confirmed on
the key are labelled Untested, show their raw value, and are read back after
each change. A Read all settings button reads everything again. The safety
rules are unchanged: one change at a time, time limits, and commands that
could harm the key cannot be sent.

## 0.12.0 — 2026-11-16

Add the Ulanzi AU05 Vibe Key. It is off until you switch it on in General.
The Vibe Key page shows the connection and battery, live key and knob
presses, the standby and power-off timers, the light mode and brightness,
and microphone noise reduction. Each key (K1, K2, K3, knob press) can run a
Unified Control action from any app (start dictation in Claude, start
dictation in ChatGPT, Send, or nothing), type a key combination stored on the
key, or use the factory setting. All writes to the key are time-limited and
limited to checked commands; nothing is written at start-up.

## 0.11.1 — 2026-11-13

Fix a half update. The updater waited only for the main UC process. The dock
worker runs from the same exe and can still be closing, so the installer could
not replace UnifiedControl.exe while the new cooler engine was installed. The
updater now waits up to 30 s until no other process runs from the installed
exe or its agent-request copy. It never ends a process. If the files are still
in use, it installs nothing and starts the current version again.

## 0.11.0 — 2026-11-13

Cooler engine 4.22.0 adds the same crash recovery as UC 0.10.0. A fault in one
part of the cooler engine no longer stops it. Locks recover, background
samplers and fetchers restart with a back-off, and an unguarded crash ends only
the worker, which UC restarts. The USB command order and timings are
unchanged. A new slow record, devices/cooler/connection-changes.jsonl, keeps
each change of cooler connection health, every Live stop, changed connect
outcomes and the app version at each start, for weeks. The full health log
keeps about two days, so the start of a long screen hang was lost (issue 9).

## 0.10.0 — 2026-11-13

Crash recovery and cleanup after the agent handover. A fault in one part now
gets recovered instead of stopping the whole program. Release builds unwind on a
panic. Locks recover after a crash. The scheduler reports a crashed job as an
error and keeps running. Background loops restart with a back-off. An unguarded
crash in the dock worker ends only that worker, and UC restarts it. Each crash
writes one short line to device.log. The dock owner's large step function is
split into named parts with no change in behaviour. Shared helpers replace
copied process flags and pipe framing. The documents now have one current-state
page, docs/status.md. The cooler engine stays 4.21.1 and still stops on a
panic; UC restarts it.

## 0.9.2 — 2026-11-11

Add an optional General freeze-diagnostics switch. The tray host writes one
timestamp per minute into bounded local rotating logs while enabled. Quiet fake
tray and native UI checks verify recording, disabled behavior and saving both
switch states. The records establish host-thread progress. Freeze causes remain
unresolved. Keep repeated --tray launches quiet through the existing Windows
startup function.

## 0.9.1 — 2026-11-06

Add finite per-step connection and shutdown results/timings from cooler engine 4.21.1. Traces are buffered until a transaction finishes, delivered through a bounded nonblocking private event queue and validated again by UC. The existing diagnostics opt-out and migration guard control persistence in the rotated local health log. No device identities, reply payloads or arbitrary error text are recorded.

Preserve commands, delays, timeouts, original errors and best-effort shutdown continuation. No new device probe, recovery operation or public RPC is added. Stop messages describe requested Embedded playback/screen-off, rather than claiming a verified physical result. This is diagnostic instrumentation, not a cooler recovery fix.

## 0.9.0 — 2026-11-06

Add typed dock actions for cooler Live-theme selection/cycling, Live on/off/toggle and brightness set/step. The editor
preserves missing theme IDs and valid prior choices through temporary unavailability; editing/preview never executes.
A generation-bound child-to-host route delivers each admitted physical action once, with correlated completion handled
before ordinary worker dispatch. Bounded completion delivery and host busy counts survive requester loss, preventing
shutdown deadlock or replay after restart. Disabled/busy targets refuse without auto-enablement or queued work.

Cooler engine 4.21.0 resolves relative actions atomically, preserves Embedded and uses a dedicated reviewed serialized
brightness operation. Fresh readback supplies relative baselines; missing/mismatched confirmation remains explicit and
never automatically retries. Existing cooler UI events reflect theme/Live changes; Screen settings displays brightness.
No physical-action MCP method, arbitrary worker command, generic raw device access, reset or firmware operation is added.


Installed 0.9.0 / source 7c3ee6d, engine 4.21.0 / source da2cb2c, passed quiet startup/new-process, actual MCP version, connected dock, both verified AI providers and unchanged selected fourteen-key layout checks. Cooler Live remains disconnected with write_timeout. A quiet comparison install of 0.8.0 / engine 4.20.0 reproduced the same timeout;0.9.0 was restored. The initiating cause is unknown. Windows reports two normal USB nodes/problem 0; no resets, elevation, firmware or actual cooler actions were performed. Physical switching/brightness acceptance remains open.

## 0.8.0 — 2026-11-05

Move AI reading/rasterization into one host-owned service, preserving existing account, provenance, freshness, history
and activation policies. Workers receive bounded typed frames with original numerical and transport expiry; shared dock
mode never starts a local account reader. Repeated helper failure uses bounded restart backoff.

Supervise a separate source-only cooler worker for music/weather, with no device or media-server startup. Cooler widgets
consume validated observations and disable replaced local readers. Preserve artwork, position, pause age and original
weather times; forecasts/retries are keyed by place. An already-started idle provider keeps the bounded in-memory weather
cache while stopping source interests, avoiding extra requests on quick widget changes. No cold empty-demand process starts.

Failure, expiry, replay, worker replacement and configuration changes cannot relabel old values as current. This completes
shared-services slice 2.6b; cross-device controls remain next and cooler workspace issue 23 remains separate. No standalone
CoolerScreen installation/data, device opt-ins or authentication settings are replaced.
Installed source 5dfec54 passed quiet startup, actual MCP 0.8.0 / engine 4.20.0, one account-reader/one provider child, device reconnection and unchanged fourteen-key layout checks.


## 0.7.0 — 2026-11-05

Collect CPU, RAM and full NVIDIA GPU readings once for interested dock/cooler displays. Expiring per-consumer demand
selects the required source families and fastest 1/2/4 Hz rate; unused sources stop. Cooler engine 4.19.0 consumes injected
readings through both legacy stats and modern widgets, with replaced local aggregate readers disabled.

Preserve original observation/expiry through worker exchanges, cached status and dock pixel delivery. Missing, failed
or expired readings remain unknown; validated generation/sequence checks prevent replay. Independent coalescing delivery
keeps a stalled consumer from blocking its peer. Ephemeral probes cannot create durable-action uncertainty.

This is shared-services slice 2.6a. AI usage, music, weather and other data sources retain existing ownership for now.
Cooler workspace entry delay remains separately tracked in issue 23.
Installed clean source b5b0140 through the idle-safe updater; startup/newPID, actual MCP 0.7.0 / engine 4.19.0, connected
devices and unchanged selected fourteen-key page verified. Real cooler demand drives one 1 Hz collector, with equal
CPU/RAM/GPU read counts and no source failures. Source/tag and private PR 25 identify the release.

## 0.6.0 — 2026-11-04

Move the dock driver, renderer, profile runtime, native actions and account reader into a supervised process.
UC remains the only durable profile writer; the child uses validated memory-only snapshots. A replacement starts
with the latest saved layout and never replays button actions. Editor previews share the child reader and preserve
freshness deadlines through IPC. Quiesce/acknowledgement coordinates busy-safe disable, Quit and updates.
The cooler worker remains independent. Shared providers and cross-device actions follow in later milestones.
Installed clean source 0117a5f through the idle-safe updater. Startup/newPID, separate dock child, actual MCP 0.6.0,
device reconnection, expected-only proxy marker substitution and unchanged selected fourteen-key page verified.
165 core/11 host/62 reader tests and relevant headless/quiet native checks pass. PR 22 is merged; v0.6.0 pins the build source.

Record the separately reported cooler workspace entry delay. The engine already stays running; repeated interface
hydration is a possible cost to measure. No warm timer or cooler-entry performance fix is claimed in this release.

## 0.5.1 — 2026-11-04

Complete cooler data migration verification while preserving UC edits and original rollback data. UC-owned custom source
definitions replace the old default-folder dependency. General offers a cancellable app-owned comparison and historical
summary; MCP reads cached finite counts without scanning. Atomic bounded receipts invalidate older success during interruption.
Generated theme previews are counted separately from original media; missing media and stale active paths still fail.
Installed source b67d928, startup/MCP/proxy/reconnection/layout verified. Actual 286 library/22 gallery files and 23 references
pass; user confirms themes/settings and physical Live.155 core/11 host and relevant headless/native checks pass.

## 0.5.0 — 2026-11-04

Introduce copy-once V2 source-definition migration and host-side data comparison. The real-data check conservatively
flags two changed generated previews, corrected in 0.5.1. Original files and UC edits are preserved throughout.


## 0.4.1 — 2026-11-03

Opening the cooler workspace no longer interrupts an active Live stream with another automatic firmware check.
After editor hydration, use a fresh worker snapshot: Live owns its startup/retry/held connection, and physical previews
retain their connection too. An actually-off screen still gets its initial check. Explicit Check/Reconnect remain.

The user grants standing permission to install tested UC updates as needed without repeated approval questions;
the idle-safe updater, quiet restart and verification remain mandatory.
Installed from clean source bc9a864 through the idle-safe updater; startup/newPID, actual MCP 0.4.1, packaged proxy and
unchanged selected profile/page/all 14 keys verified. Real reentry smoothness remains separate from the automated check.

## 0.4.0 — 2026-11-03

Move device enable switches into General. Only enabled devices appear in Home, the sidebar and search; enabling a device
does not leave settings, and failed changes preserve the previous state. Hide the unimplemented Dial entry. Remove large
device enable headers and let the cooler workspace use the full right pane without an outer frame or duplicate app name.
Refine navigation, spacing, typography, cards, settings and switches, with compact layouts and reduced-motion support.

Preserve saved cooler Live across editor page changes and close-to-tray. Claim temporary editor streams when Live is
enabled, clear stale temporary ownership after explicit off, and detach only the temporary stream being cleaned up.
Explicit Live off/device disable/Quit retain their behavior; autosave/import guards and local diagnostics remain.

Installed from clean source b18a4a1 with startup/newPID, actual MCP 0.4 / engine 4.18.2, packaged proxy and unchanged 14-key
layout verified. User confirms physical Live continues across page changes and close-to-tray; repeated entry is fixed above.

## 0.3.5 — 2026-11-02

Fix cooler section navigation waiting for unavailable-device preview cleanup. Real edits and imports still flush before
the frame is removed; transient preview/watch cleanup continues through the parent without blocking other sections.
Loading frames can be left before their bridge is ready. Idle visits do not enqueue redundant cleanup requests.
The embedded view is removed on exit, stopping its visual loops; enabled worker Live/diagnostics remain independent.
Disable, reset and Quit retain their full cleanup checks. No USB recovery or hardware fix is claimed.

Installed with user approval through the idle-safe updater. NewPID/startup version, actual MCP 0.3.5, packaged proxy,
dock/provider reconnection and unchanged active 14-key layout verified. Hardware recovery and user navigation feel remain open.

## 0.3.4 — 2026-11-02

Add the planned sidebar page search. Match page names and related terms such as usage, themes and MCP. Filtering does not
save or navigate; Enter opens the first result through the existing guarded navigation path. Escape clears the query.
Keyboard focus skips hidden results. Compact sidebars expose a search button and can expand/collapse without losing access.
Closing compact search clears its filter; resizing with an active query keeps search visible instead of hiding pages
behind an invisible filter.

Installed corrected source 95a68ee through the idle-safe updater, with newPID/startup version, actual MCP version,
packaged proxy identity, unchanged active layout and dock/provider recovery verified. The earlier draft package was not installed.

## 0.3.3 — 2026-11-02

Refresh Home device cards in place when accepted status polls arrive, preserving keyboard focus. Distinguish the cooler
engine's state from its last connection check on Home and the cooler page. Missing, disabled, failed or out-of-date
checks remain distinct; cached connection reports are not a guarantee of current physical operation.

Expose a strict cached cooler-health summary through enabled MCP status, without triggering collection or recovery.
Worker and Windows USB sample timestamps are separate. Identifiers, paths, PIDs and arbitrary error messages are excluded.

Installed with startup/new PID, actual MCP version, packaged proxy identity and unchanged active layout verified. Dock
and both usage providers recovered. The last cooler sample still reports descriptor failure 43; no hardware recovery.

Physical 0.3.2 retest: the user reported three Test/Back cycles responsive; six navigations completed without aborts.
The latest host-receipt-to-transfer duration was 72.44 ms. No before/after speedup or physical pixel timing is claimed.

## 0.3.2 — 2026-11-02

Keep keyboard focus on the matching dock editor control after display/face/presentation/action changes. Tab continues
through the editor. Undo keeps the selected display on the same stable profile/page, with a safe fallback if that page
is removed. Explicit navigation and agent/editor coordination retain their existing behavior.

Add bounded numerical navigation timings to dock state and enabled MCP status. Separate input preparation, face rendering
(including usage-helper waits), delivery preparation and combined ZIP/transport work. Retain one latest completed
host-receipt-to-delivery sample and navigation completion/abort counts. No scheduling change or physical speedup is claimed.

Preserve guarded Codex error/stale states while continuing to hide all unverified numbers. Missing account identity still
suppresses otherwise valid readings. Simulated regressions cover throttling, recovery and history/token clearing.

Installed through the idle-safe updater during the user's requested continuation. New process/startup version, MCP 0.3.2,
dock reconnection, unchanged active-page keys and both providers verified. Numerical timings are readable; physical
navigation capture/feel remains separate from automated checks.

## 0.3.1 — 2026-11-02

Fix AI Usage reader startup when Windows supplies canonical extended resource paths. Normalize trusted disk/UNC executable,
script and usage-home paths before starting Node, preserving Unicode and other device namespaces. The canonical-path
fixture reproduces `reader-unavailable` before the fix and passes afterward without account access. Live account refresh
and physical usage acceptance are checked separately. Installed through the user-approved idle-safe updater;
new process/startup version, dedicated proxy hash and dock reconnection verified. Both providers reached verified status.

Codex desktop MCP acceptance now covers actual model-driven status/layout/monitor reads, preview, inactive-profile
apply/readback, stale revision refusal and undo. The temporary face was restored and active dock selection preserved.
Claude model-client acceptance remains open.

## 0.3.0 — 2026-11-02

Add agent-neutral local MCP control. A bounded same-user RPC channel and a request-only command line let Codex or Claude read status and layout, preview changes and apply them with a revision check.

Editor leases and revision-checked writes keep agent edits and the open editor in step. Native checks cover agent editing and delayed editor recovery.

The installer adds a dedicated request identity for packaged MCP clients. This identity has no legacy GUI fallback.

## 0.2.3 — 2026-11-02

Fix the Monitor Toggle path. Verbatim monitor helper paths are now changed to canonical Windows paths before PowerShell runs them.

Monitor discovery failures now show in the interface, and the user can retry. A native check confirms that discovery passes through normal IPC. The 0.2.2 recovery checkpoint was not tagged.

## 0.2.1 — 2026-11-01

Add the requested background Connection diagnostics in UC, on with a saved opt-out. Read-only worker health runs every ten seconds; passive Windows USB status every thirty seconds. State transitions and minute snapshots go to bounded local rotating logs. Worker timeouts, USB write failures, matched descriptor failures and intentional holds remain distinct; raw messages, device/account IDs and media paths are excluded. Logging continues with the editor closed and stops when UC exits. No automatic USB resets or elevation.

Add explicit Reset cooler USB for a normally enumerated but unresponsive screen. It saves edits, uses the exclusive device job, requests Windows approval, selects one exact cooler parent/port/interface, resets once and checks for a genuine firmware reply. Failed enumeration keeps the existing exact remembered-port recovery path. Successful reset request alone is never reported as recovered hardware.

Current incident remains unresolved: initial PnP nodes were OK while writes timed out before the session's first successful Live connection. One user-approved targeted reset was accepted, but normal enumeration did not return; Windows then reported code 43. Physical USB reconnect is the next recovery step. This does not establish development work, firmware, power or a cable as the initiating cause.

## 0.2.0 — 2026-11-01

Integrate CoolerScreen themes, library, Live, Embedded and settings into the cooler page using the existing interface and a separate supervised engine from the private CoolerScreen repository. Devices remain separately enabled; cooler starts off. First enable imports a separate copy of the old settings/library/gallery and preserves the fallback. Existing CoolerScreen must be closed by the user before UC takes ownership.

Use private inherited pipes with validated commands/events, bounded chunked file imports, retained busy-work guards, explicit saved-Live activation and independent worker restart. Autosaved cooler edits/imports flush before leaving or quitting. Bundle the pinned engine, reviewed ffmpeg and existing setup/reconnect scripts; apply a UI content security policy.

Fix Monitor Toggle discovery for PowerShell zero/one/many display output. Add safe AI Usage reader/provider status messages without weakening freshness/account gates. Both user-reported function failures remain open for verification. UI polish and physical cooler/migration acceptance are pending; sleep testing remains deferred.

Correct native async test waits to explicitly poll resolved IPC conditions. Native tests cover real host/worker IPC and iframe routing, imports, crash isolation, busy refusal and clean shutdown using fake devices only.

## 0.1.5 — 2026-11-01

The virtual dock follows completed physical profile/page navigation, including Folder and Back. Follow physical page is enabled by default for the user's requested behavior and can be turned off. Observational page following never sends navigation back to the dock and waits for pending edits/imports/saves; manual inspection and disconnected states do not cause polling to pull the editor back. The preference persists in app settings. Physical page status is published after full-page transfer succeeds.

Fix light flicker in UC's software dock previews: retain identical decoded images across polling, decode changed frames before swapping them in, and discard stale/detached results. Browser regressions cover unchanged-node retention, delayed decoding and navigation. Tracks issue #3.

Native text input also retains decoded preview nodes instead of rebuilding an HTML fallback on each keystroke. Rapid-typing regression verifies latest text persistence, render requests and the final decoded frame without a size flash.

## 0.1.4 — 2026-10-31

Editing one face of a physical key now shows that edited face as soon as its saved change reaches the dock, even if the key was displaying its other face. Clock, date, icon and other face edits no longer need an A/B round trip. Unrelated key selections, held-press deduplication and folder history stay intact. Name/action-only edits do not flip keys; a bulk change to both faces retains the current selection. Saving an edit never executes the key action.

## 0.1.3 — 2026-10-31

Fix the passive-only liveness check exposed by the 0.1.2 physical idle trial. The user saw a stable layout for about ten minutes, but diagnostics showed continuing keepalive writes, no passive heartbeats and repeated receive-timeout reconnects. After 25 seconds without input, send one probe using the already-selected dock brightness and wait for its real reply; retain the 35-second receive deadline if no response arrives. A successful write alone is never proof of liveness. Background image keepalive remains enabled.

The installer can use an explicitly supplied reviewed request-only UC executable when its installed-file view is shadowed by a packaged copy; it verifies the new process's logged startup version/PID rather than file metadata alone. All busy and unreachable-host guards remain enforced.

## 0.1.2 — 2026-10-31

Physical-test update: send the documented background small-window keepalive every five seconds after the first full page, only while the dock is enabled, connected and Studio is absent. Preserves the wide PNG and avoids resending unchanged images. Diagnostics count keepalives. This tests a host-idle fallback hypothesis: on 0.1.1 the user saw the dock's startup screen within 10–15 seconds while inbound heartbeats and clock partial updates continued with zero errors/reconnects. Physical efficacy is not yet established.

Installed quit/update requests now share the same default identity as the app's single-instance lock, independent of packaged APPDATA paths; debug/explicit-home channels remain isolated. The installer refuses to proceed if an existing app cannot be reached through its update guard.

## 0.1.1 — 2026-10-31

Compatibility and diagnostic update for a user-reported return to the physical dock's startup/default screen while UC still reports Connected. Tolerates nonstandard `0x010b` acknowledgement lengths, as the reference driver does; other input validation remains strict. Adds sanitized connection transitions, read-error kinds and heartbeat/ack/frame counters. Heartbeats and acknowledgements no longer force extra rendering. The cause of the physical fallback is not yet established; this is not a claim that it is fixed.

## 0.1.0 — 2026-10-31

Dock MVP acceptance preview. Adds native profiles/pages, A/B key editor, image/icon/date/live faces, all usage presentations, reviewed Windows actions, explicit dock control, Studio coexistence, reconnect, brightness and tray controls. Includes the managed reader runtime and monitor helpers in the installer. Autosave and Quit preserve pending imports and queued edits.

Software checks pass against isolated files, fixture providers and a simulated dock. Physical dock acceptance is pending; CoolerScreen and Dial engines are not integrated.
