# Dock acceptance status

Updated 2026-10-31. Installed version: 0.1.4 (`b212fe2`). These results distinguish user observation from software/transport evidence. Phase 1 is not yet fully accepted; CoolerScreen and Dial engines remain unintegrated.

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
| Sleep/resume | Deferred at the user's request; cannot do this now. Remains pending and will be revisited later. |
| Editor usability/scaling | Browser checks at three widths pass; full user review pending. Minor preview flicker tracked below. |

## Nonblocking UI bug

[Preview flicker — issue #3](https://github.com/oldbugo/unified-control/issues/3): user sees a light flicker every few seconds in UC's software preview on 0.1.4. Physical edit refresh is confirmed working. Source inspection found every preview image is replaced before decoding on every polling tick, even for byte-identical frames. Keep unchanged images and decode changed frames before replacement; stale/detached results must be discarded. A quiet browser regression and the next suitable release should verify the fix. Keep the installed 0.1.4 running during current hands-on acceptance; no restart is needed merely to record this issue.

The 0.1.5 build includes `KeyPreviewRenderer`: unchanged image nodes are retained, replacements decode first and stale/detached results are rejected while temporary URLs are released. The headless suite passes unchanged-image/URL/decode-count checks, a delayed replacement retaining all fourteen old images, and late results after navigation. Issue remains open until installed user-visible verification.

Additional user feedback: typing display text can flash the preview at an apparently smaller size. The native input handler rebuilds HTML fallback content before requesting PNGs; issue #3 now includes that path. Navigation performance and virtual-page following are tracked separately in the [backlog](backlog.md).

The 0.1.5 typing fix keeps decoded key nodes, requests updated PNGs and aligns the preview face. The headless regression delays decoding during rapid typing, checks latest text persistence/render requests, preserves old image nodes and confirms the latest decoded frame wins. Virtual page following is included in the same build; physical confirmation is pending.
