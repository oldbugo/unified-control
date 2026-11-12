# Dock foundations (2026-10-30)

Archived: this is a historical record. The `uc-ulanzi` pin below is outdated. The real pin is `9ea93c0` in `src-tauri/Cargo.toml`.

This stage implements profile storage, initial PNG rendering and a transport-independent runtime. It does not complete
the dock MVP. Production action adapters, verified account providers, native-device ownership/coexistence and physical
acceptance are still required before replacing Studio. CoolerScreen and dial integration follow dock acceptance.

The app depends on the separate private `uc-ulanzi` repository at exact commit `a46747589f9f8295541ed207ac71d31808cd0992`.
It constructs no native transport. `PageRuntime` accepts a `Dock<T: Transport>` supplied by its caller; automated checks
use `FakeDock`. Real hardware ownership and a worker loop belong to the later app-level integration.

`PageRuntime` validates its profile snapshot, starts each display at Face A, flips exactly A/B once per press and suppresses
duplicate press reports until release. This latch survives page transitions. Folder history is bounded to 32 entries.
External actions are returned as `PendingAction` data and are not executed. Current-page state is runtime data until a
caller explicitly saves a new editor profile state.

The renderer uses Windows system fonts through ab_glyph (the same approach as CoolerScreen's compositor, source
`b68d2e8`). Large text is fitted to the physical bounds with a minimum size; unsupported characters and missing fonts
return an error rather than a blank success. Images are opaque RGB PNGs, 196x196 or 458x196. The initial icon face uses
its text label. Image/icon artwork and complete usage presentations are later rendering work.

Account presentations have no numeric input in this initial rendering API. Claude, Codex, reset and trend render unknown
regardless of arbitrary face text. Sensors are explicit optional inputs; invalid or unavailable values remain unknown.
The renderer reads no accounts or sensors. A future usage adapter must enforce account provenance and freshness at both
provider and renderer boundaries before introducing current numerical gauges.

Only changed PNGs enter partial updates; successful transport writes advance the cache. Failed writes clear it so the
next transport receives a new full page. The driver disables cached reconnect replay by default. Applications must render
a new safe page after connection, rather than replay old usage images.

The editor converts numeric UI indices to stable profile/page IDs for native saves. One write is in flight; newer edits
coalesce, stale errors do not replace newer edits, and the latest failure remains retryable. Native load errors never fall
back to a new prototype layout. Close hides the existing WebView to preserve its queue. Quit/restart/update use an editor
flush handshake before exit or helper launch; failed writes keep the editor available. Keeping the WebView in the tray
currently trades memory for reliable recovery. Releasing an idle WebView is later work, after a verified flush protocol.

Native verification caught missing Tauri command permissions that library tests could not catch. Register every new
command in the Rust handler, build manifest and window capabilities together; run the quiet native IPC check before
claiming integration. Review also found the former destroy-on-close path could lose queued edits; the native regression
delays an earlier write, queues a newer one, closes the window and invokes the real timed tray-Quit path. The newest edit
must reach disk before the app exits.
