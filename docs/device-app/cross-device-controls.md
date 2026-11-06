# Dock-to-cooler controls

Plan2.7. Software implementation verified2026-11-06 and installed in0.9.0. Physical acceptance remains open: cooler Live reports write_timeout, reproduced on the prior0.8.0 as well. No actual device action was used in automated tests. See [installation evidence](../acceptance.md).

## Stored action

Existing profile storage remains `{action,value}`. The new action kind is `cooler`; value is a strict JSON object tagged
by `op`, with unknown/duplicate fields rejected at profile and execution boundaries:

- `theme_select` with `theme_id`: stable ASCII letters/digits/hyphen/underscore,1–128bytes; never a path or display name.
- `theme_next` / `theme_previous`: fixed case-insensitive theme-name order, ID tie-break, wrapping at either end.
- `live` with mode `on`, `off` or `toggle`.
- `brightness_set` with integer value0–100.
- `brightness_step` with nonzero integer delta−100–100; the result clamps to0–100 from a fresh device report.

The action editor does not execute effects. It provides explicit theme selection and preserves deleted/unavailable IDs
and prior valid choices during temporary disconnection. Existing A/B faces, navigation and unrelated actions remain.
No action enables a disabled cooler. Theme selection changes the Live theme and never stores or overwrites Embedded.
Relative operations resolve atomically inside the cooler worker, against its current state; a UI-side read is not a baseline.

## Action route and lifetime

The deduplicated physical press uses the dock's existing serial action executor. It emits a bounded typed request with
monotonic ID on the current private worker channel. The host validates/adopts it once on a separate bounded serial queue.
A captured session/generation prevents replay into a replacement worker. Host callback busy counts survive requester loss,
so Quit and device disable cannot truncate an admitted cross-device operation.

Completion is enqueued with a500ms bound through the original session writer and demultiplexed by the child's stdin reader before ordinary
command dispatch. This avoids Shutdown waiting on an action whose completion is queued behind Shutdown. EOF fails pending
waiters. Timeout, disconnect, target failure or a stale/duplicate ID never causes an automatic retry of a relative action.

The host holds a cooler-configuration admission guard, calls only named private cooler operations, and preserves target
uncertainty after a lost reply. Neither stored JSON nor editor/MCP configuration can supply arbitrary worker RPC, shell or
USB commands. Existing MCP layout edits remain inert; no physical-action MCP tool is added.

## Cooler execution

Target admission refuses active visible/device work atomically rather than queueing a physical press for later replay.
Read-only catalog/state queries do not open a device. Current theme/Live state is published through the existing settings
and library events, so the cooler interface reflects key actions.

Brightness uses a dedicated reviewed engine method for the documented brightness command, serialized under the existing
device scheduler while Live relinquishes the USB handle. It never broadens generic command allowlists, resets USB,
falls back to serial or sends firmware/storage operations. Steps require fresh reported brightness; unknown is not zero.
Requested and reported brightness remain distinct, and the existing Screen settings show the latest reported value.
No brightness write happens on startup, catalog inspection or profile editing.

## Verification boundary

All automated action tests use isolated quiet debug instances, fake devices and synthetic themes. Verify strict schema,
inert editing/preview, press deduplication, deterministic cycle/Live semantics, Embedded preservation, brightness bounds and
readback, iframe reflection, busy refusal, correlated completion, requester/target crashes, no replay and safe Quit.
Actual visual switching, brightness feel and physical key coverage require separate user acceptance. Cooler workspace
entry issue23, sleep/resume, installer replacement and longer combined soak remain separate work.

Software gate:202 UC core/11 host/62 reader tests pass, with three existing ignored checks. Cooler five dock-control,19 scheduler and nine private-worker cases pass. Full headless editor/cooler and native cross-device/profile/MCP/dock-worker/cooler/lifetime/migration/shared-provider/hardware checks pass. Native cross-device evidenceW3tNZb proves actual fake button routing, iframe reflection, Embedded preservation, busy after requester crash, no replay, correlation during Shutdown and EOF cancellation. Independent routing/device safety reviews found no remaining actionable issue. New/worker formatting and whitespace pass; broad formatting retains unrelated existing differences.
