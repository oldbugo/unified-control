# Dock acceptance status

Updated 2026-11-01. Installed/running version: 0.2.1 (`f079a96`), new startup/PID verified after cooperative update. User authorized proceeding with CoolerScreen integration before UI polish; remaining dock tests stay open. Cooler engine/interface are integrated in preview; current enabled cooler cannot enumerate, so physical acceptance has not passed. Real-data import/content continuity remains unverified. Dial remains unintegrated. These results distinguish user observation from software/transport evidence.

| Check | Status and evidence |
| --- | --- |
| Immediate physical face edits | User confirmed on 0.1.4: edited content appears automatically without an A/B round trip. |
| A/B presses | User tentatively reported the two-press check worked. Broader per-key checks remain pending. |
| Idle display and host liveness | User saw ten idle minutes without fallback on 0.1.2, but logs exposed hidden reconnects. 0.1.3 response probes then ran with zero host errors for hours. This is not a full physical/performance soak pass. |
| USB unplug/replug | User confirmed on 0.1.4: layout returned automatically without toggling Enabled or reopening UC. Logs show disconnect at 12:09:03 UTC, retry backoff and Connected plus a fresh full-page send at 12:09:34. Exact replug time was not captured; do not infer recovery latency from the disconnect timestamp. Retry errors during the intentional unplug are expected. |
| Folder and Back | User confirmed both actions work on 0.1.4. Approximately one-second navigation delay is a separate performance follow-up; the virtual editor currently does not follow physical page changes. [Follow-up backlog](backlog.md). |
| All physical keys and wide area | Remaining per-key coverage and firmware wide-area behavior pending. |
| Hotkey/text/launch/media/Monitor Toggle | Software/fake checks pass; user reported functions mostly work, but individual physical action results remain incomplete. Monitor/audio actions require a suitable hands-on check. |
| Profiles, autosave and restart persistence | User confirmed on 0.1.4: profile, Test page and assignments remained after tray Quit/Start-menu reopen, and UC reconnected. Automated IPC/storage/quit tests also pass. |
| Usage and account states | Codex idle source refresh verified; source freshness/unknown/account gates tested. Claude fresh-source/account-panel comparison and account-switch/logout acceptance remain pending. |
| Reported AI Usage and Monitor Toggle failures | User reports both nonworking. 0.2.0 fixes zero/one-monitor discovery and adds usage status messages; actual causes/physical resolution remain open in [functional follow-ups](function-failures.md). Installed startup logged finite `reader-unavailable`, not successful live usage. |
| Sleep/resume | Deferred at the user's request; cannot do this now. Remains pending and will be revisited later. |
| Editor usability/scaling | Browser checks at three widths pass; full user review pending. Minor preview flicker tracked below. |

## Cooler integration preview

Current recurring-failure incident: initial normal USB nodes had problem0 while writes timed out before any successful Live connection in the retained UC session. One exact user-approved USB port cycle was accepted but failed recovery; post-reset Windows reports matched descriptor failure43. User cannot access the screen USB cable now, so physical reconnect is deferred. 0.2.1 background diagnostics are installed and verified producing non-simulated local records. No initiating cause or recovery pass is claimed. [Evidence and monitoring](cooler-connection-health.md), [issue9](https://github.com/oldbugo/unified-control/issues/9).

Software checks pass for existing embedded themes UI, chunked imports, copied-data migration, saved Live settings, busy-safe Quit and worker crash/restart while dock stays connected. Corrected awaited IPC polling and actual native iframe routing are included in the checks. Physical Live/Embedded, imported user themes/settings and combined-device recovery remain pending. Sleep/resume stays deferred. Original CoolerScreen app/data are retained; it must be closed by the user before enabling the cooler engine in UC. No full phase2 acceptance or old-app replacement is claimed. See [architecture](device-app/architecture.md).

0.2.0 installer SHA-256: `917B7626FE2CE93B646F097A9DF983BCD15C57699644AE88CCFF51BBE635E693`. Running source `9b9ffca`; engine pin `58cb2da`. Startup 05:17:43 UTC, dock Connected and initial full page with zero errors; no cooler worker starts while disabled.

0.2.1 installer SHA-256: `2FB3B8C38011334D3643E0BF8FCD5E238FB34BD4CE7F578172FDE88DA22AFAB5`. Running source `f079a96`; engine pin `88cd041`. Startup 10:50:10 UTC/new PID verified; dock Connected with fresh full page and zero errors. First health transition 10:50:20 UTC reports descriptor_failure, normalNodes0, exactMatchedDescriptorCount1 and problemCodes[43], simulatedfalse. The first installer attempt refused busy work; a later idle-safe attempt succeeded without force quit.

## Nonblocking UI bug

[Preview flicker — issue #3](https://github.com/oldbugo/unified-control/issues/3): user saw a light flicker every few seconds in UC's software preview on 0.1.4. Physical edit refresh is confirmed working. Source inspection found every preview image was replaced before decoding on every polling tick, even for byte-identical frames. Version 0.1.5 retains unchanged images and decodes changed frames before replacement; stale/detached results are discarded. Browser and quiet native checks pass; installed user-visible confirmation is pending.

The 0.1.5 build includes `KeyPreviewRenderer`: unchanged image nodes are retained, replacements decode first and stale/detached results are rejected while temporary URLs are released. The headless suite passes unchanged-image/URL/decode-count checks, a delayed replacement retaining all fourteen old images, and late results after navigation. Issue remains open until installed user-visible verification.

Additional user feedback: typing display text can flash the preview at an apparently smaller size. The native input handler rebuilds HTML fallback content before requesting PNGs; issue #3 now includes that path. Navigation performance and virtual-page following are tracked separately in the [backlog](backlog.md).

The 0.1.5 typing fix keeps decoded key nodes, requests updated PNGs and aligns the preview face. The headless regression delays decoding during rapid typing, checks latest text persistence/render requests, preserves old image nodes and confirms the latest decoded frame wins. Virtual page following is included in the same build; physical confirmation is pending.
