Archived 2026-11-13; superseded by [docs/status.md](../status.md).

# Stand-down handoff

Checkpoint **2026-11-13**, Australia/Sydney. The user asked to wrap up, push reviewed work to private GitHub, update the vault and hand over. Development is stood down. Leave the installed tray app running.

## Where to resume

Use `C:/Users/pc/Documents/UnifiedControl`, private `oldbugo/unified-control`. This chat began in the separate Ulanzi Toolkit folder. The heartbeat work belongs to Unified Control. Read AGENTS.md, CLAUDE.md, README.md and the vault Start here before work. Read current Git and MCP state before decisions.

## Built, installed and checked

- UC **0.9.2**, build source `8c194f280cf1ff9768ec3cebd036cec1d3593b58`. Installer `dist/UnifiedControl_0.9.2_x64-setup.exe` remains local. SHA256 `4a8b6dd3a5e7bb3fadaad3ae78fd0da1e368b21662d0aa97155e8310559f65f5`.
- Engine **4.21.1**, pinned source `77c90863bec8e761d34f4de4cadd67abd59c8a6a`.
- Installed on 2026-11-11 through the cooperative idle-safe updater. A new host PID and MCP version were verified. All fourteen keys on the selected page matched the pre-update layout. Dock reconnected.
- Historical checks for this code passed 206 core and 12 host tests (three existing core ignores), JavaScript syntax and diff checks. Quiet isolated fake tray hosts proved two minute-spaced records when enabled, no log when disabled, and normal Quit. A quiet native UI check proved the switch saves both states through normal IPC. This stand-down changes documentation only.

## Temporary freeze evidence

General has a **Freeze diagnostics** switch. It defaults off. On 2026-11-13 the saved setting was **off**, and the heartbeat file was absent. The installed app has therefore not collected a real freeze heartbeat trace.

When enabled, the host thread checks the setting and appends once per minute, independently of the webview and device workers. Each line is `sample_unix_seconds host_thread_start_unix_seconds pid`. Files are `%APPDATA%/UnifiedControl/freeze-heartbeat.log` and `freeze-heartbeat.previous.log`, each bounded to about 256 KiB. Enable/disable can take up to one sampling interval. The thread ends with the tray host. Disk errors are recorded in device.log.

These records show host-thread liveness. They do not directly detect frozen pixels, a GPU hang or a cooler freeze. Sleep, app exit, disk failure or a stalled thread can also leave gaps. Compare traces with Windows event and device-health times before assigning a cause.

**Start with Windows** already existed. 0.9.2 also keeps a second `--tray` launch from raising the window. The current HKCU Run entry was read back on 2026-11-13 and targets the installed executable with `--tray`. Actual sign-in startup was not exercised.

## Current device state and unresolved cause

Read-only MCP on 2026-11-13 reports UC 0.9.2, agent access enabled, dock connected, cooler worker running, connection diagnostics enabled, cooler health **write_timeout**, and Live connected **false**. The earlier 2026-11-09 user-confirmed recovery after cycling the PC is historical. The recurrence remains unresolved.

The user cannot identify the moment a freeze starts. They notice it later. Which display/window the latest report referred to was never confirmed. Do not assume a PC GPU freeze and cooler connection failure are the same event.

Correction to the earlier chat diagnosis. Windows reported LiveKernelEvent141 entries on 2026-11-10, but their referenced WATCHDOG dumps were dated September/October. Those report times do not establish new GPU timeouts that evening. Compare original event/dump timestamps, not report submission time. Protected WER report contents could not be read by this session.

## Next agent

1. Resume only on the user's next direction. Check current diagnostics switches and health. The heartbeat remains available for temporary use.
2. Preserve connection evidence for the recurring cooler write timeout. Keep existing connection diagnostics on unless the user changes it.
3. The wider feature queue remains installer transition 2.8, combined release gates 2.9, then Dial. Read project-status.md and the installer transition contract. Standalone CoolerScreen remains the fallback.
4. Physical cross-device controls, full device acceptance, real freeze diagnosis and sign-in startup acceptance remain open. Sleep/resume remains deferred.

No automatic resets, elevation, guessed USB/firmware commands or profile edits. Use the running app's MCP for supported changes. Test with isolated quiet debug/fake instances. Use the idle-safe updater for authorized app updates. No background agent watch or scheduled task was created.
