# Dock controller

The controller is built for app integration. All automated checks use the software
dock and injected hooks; native HID, Studio enumeration and real key actions have
not been run by this work.

`Controller::new(Arc<ProfileStore>, Arc<dyn DockHooks>, fake)` starts disabled.
The constructor does no profile, process or device I/O. `set_enabled` is explicit;
the app must keep startup opt-in. Set the persisted brightness before enabling.
`set_brightness`, `notify_profiles_changed`, `status`, `busy`, `shutdown` form the
app API. Fake-only `simulate_button` and `set_fake_studio` reject native mode.

One owner thread holds the transport, native ownership mutex and `PageRuntime`.
It checks the exact process name `UlanziDeck.exe` before opening native HID, at
least once per 250 ms between operations while connected, and before handling
physical events. A separate serial action executor checks Studio again before
execution. A running Studio or failed probe pauses access and releases the dock;
the controller retries after Studio is closed. It never closes or changes Studio.
The ownership mutex is held/released on the same thread and prevents another UC
instance in that Windows session from driving the dock concurrently.

Control/event and action queues have fixed bounds (32 and 16). Profile changes
coalesce through an atomic revision; the latest durable profile state is loaded.
Physical page navigation stays inside `PageRuntime` and is reported through status
IDs. It never writes a whole editor snapshot back to disk. Native reads use 10 ms
waits. A missed 35-second input/heartbeat deadline or transport failure releases
the handle and retries with a 1-to-30-second backoff. A new transport clears input
latches and frame acceptance, then re-renders a fresh full page through the hooks;
it never replays cached account-gauge PNGs.

Hooks provide shared inputs, face rendering, active-provider notification and
reviewed action execution. Live redraws occur at most once a second, plus physical
press/config/error transitions; unchanged frames are suppressed by `PageRuntime`.
Action failures show a short ERROR face on the originating display, then restore
its normal face. Status messages never contain integration errors or action values.

An action already in progress finishes normally. Disable, disconnect, configuration
change and shutdown invalidate pending action admission; queued jobs from an old
epoch do not execute. Shutdown stops new admission immediately, then waits for
active device operations and action transactions. No process is killed, including
Monitor Toggle's recovery path. `busy()` remains true for active/queued actions
and rendering/writes. The app must call shutdown away from its UI event thread
and refuse installs/forced quit while busy. Driver page writes have their own
30-second logical deadline plus the last bounded report; the service does not
interrupt a partially sent message.

Remaining acceptance: native Studio coexistence/ownership, real unplug/replug,
sleep/resume, heartbeat behavior, physical input/readability, error flashes, the
one-hour live update run and safe shutdown around actual action transactions.
No physical acceptance is established by fake tests.
