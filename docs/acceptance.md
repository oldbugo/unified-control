# Dock acceptance status

Updated 2026-10-31. Installed version: 0.1.4 (`b212fe2`). These results distinguish user observation from software/transport evidence. Phase 1 is not yet fully accepted; CoolerScreen and Dial engines remain unintegrated.

| Check | Status and evidence |
| --- | --- |
| Immediate physical face edits | User confirmed on 0.1.4: edited content appears automatically without an A/B round trip. |
| A/B presses | User tentatively reported the two-press check worked. Broader per-key checks remain pending. |
| Idle display and host liveness | User saw ten idle minutes without fallback on 0.1.2, but logs exposed hidden reconnects. 0.1.3 response probes then ran with zero host errors for hours. This is not a full physical/performance soak pass. |
| USB unplug/replug | User confirmed on 0.1.4: layout returned automatically without toggling Enabled or reopening UC. Logs show disconnect at 12:09:03 UTC, retry backoff and Connected plus a fresh full-page send at 12:09:34. Exact replug time was not captured; do not infer recovery latency from the disconnect timestamp. Retry errors during the intentional unplug are expected. |
| Folder and Back | Hands-on check requested; result pending. |
| All physical keys and wide area | Remaining per-key coverage and firmware wide-area behavior pending. |
| Hotkey/text/launch/media/Monitor Toggle | Software/fake checks pass; user reported functions mostly work, but individual physical action results remain incomplete. Monitor/audio actions require a suitable hands-on check. |
| Profiles, autosave and restart persistence | Automated IPC/storage/quit tests pass. Full user layout persistence check pending. |
| Usage and account states | Codex idle source refresh verified; source freshness/unknown/account gates tested. Claude fresh-source/account-panel comparison and account-switch/logout acceptance remain pending. |
| Sleep/resume | Pending user check. |
| Editor usability/scaling | Browser checks at three widths pass; full user review pending. Minor preview flicker tracked below. |

## Nonblocking UI bug

[Preview flicker — issue #3](https://github.com/oldbugo/unified-control/issues/3): user sees a light flicker every few seconds in UC's software preview on 0.1.4. Physical edit refresh is confirmed working. Source inspection found every preview image is replaced before decoding on every polling tick, even for byte-identical frames. Keep unchanged images and decode changed frames before replacement; stale/detached results must be discarded. A quiet browser regression and the next suitable release should verify the fix. Keep the installed 0.1.4 running during current hands-on acceptance; no restart is needed merely to record this issue.

Source fix prepared: `KeyPreviewRenderer` retains unchanged image nodes, decodes replacements first and rejects stale/detached results while releasing temporary URLs. The headless suite passes unchanged-image/URL/decode-count checks, a delayed replacement retaining all fourteen old images, and late results after navigation. Issue remains open until a new build and user-visible verification; this source change is not in installed 0.1.4.
