# Dock visuals and on-demand metrics

Built in the visuals branch; app integration and physical acceptance are separate checks.

`render_face(kind, text, wide, inputs)` keeps its original API. Added face kinds are
`picture` and `date`. `RenderInputs` adds `date` (`YYYY-MM-DD`), `icon` (one of
`ICON_NAMES`) and `picture` (`Option<Arc<PictureAsset>>`). Labels stay limited to
40 printable characters and must fit legibly. Clock input is validated as 24-hour
`HH:MM` or `HH:MM:SS`; calendar dates reject impossible days.

The 13 fixed icons use local vector geometry, with no downloaded assets or arbitrary
SVG parser. The set covers apps, folders, back, media transport, volume/mute,
monitor, globe, keyboard and clock. Missing pictures show a dash; account faces
always stay unknown through this API. The usage service owns verified quota faces.

## Picture import contract

The caller must import bytes from an explicit user file choice. `PictureAsset::decode`
accepts PNG/JPEG only, at most 4 MiB, 4096 pixels per side and 16 million total pixels.
Decoding uses strict dimension limits and a best-effort 128 MiB decoder allocation
budget. Images normalize to at most 1024 pixels on the long side; `.png()` returns
the normalized PNG (at most 4 MiB). `.dimensions()` returns its geometry.

The app stores that PNG under its own opaque asset ID. Profiles contain the asset
ID, never a source path, URL or base64 payload. Resolve the ID inside the app-owned
asset folder and call decode before supplying `RenderInputs.picture`. The renderer
does no file or network access. Alpha is composited onto the dock background;
the image fits without cropping, with space reserved for an optional label.

## Sensors

`Sampler::new()` does no I/O. The owning app calls
`sample(Needs { cpu, gpu, ram }, monotonic_elapsed: Duration)` only for visible,
enabled features. It returns `Readings { cpu, gpu, ram }` as optional percentages.
Sampling is throttled to once per second and shared by the caller. No thread,
process, service, startup entry, driver installation or admin access is added.

CPU/RAM logic follows CoolerScreen `b68d2e8b98f544ddd48a0a535109f3ee6b591ec7`,
`src-tauri/src/live/metrics.rs`; GPU loading follows its `live/sensors.rs`.
CPU uses `GetSystemTimes` deltas; the first sample, errors and counter resets stay
unknown. RAM uses `GlobalMemoryStatusEx`. NVIDIA GPU utilization uses the installed
driver's NVML through a System32-only DLL lookup, with checked initialization and
utilization results. Failed/unsupported GPU reads stay unknown and retry after
30 seconds while requested. Disabling GPU releases NVML. Other GPU vendors and
driver layouts without System32 NVML show a dash; no substitute readings are invented.

Checks are pure image/geometry tests and injected sensor-source tests. No native
GPU call, USB device, app window, monitor/audio setting or account was accessed
by these checks. Real sensor accuracy and physical readability require acceptance.
