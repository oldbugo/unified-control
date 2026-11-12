# Unified Control: Codex startup

This repository is the combined device app. The separate Ulanzi Toolkit repository has its own usage-plugin scope.

1. Read `C:/Users/pc/Documents/knowledge-vault/Start here.md` in full and perform its safe remote-refresh check.
2. Read `CLAUDE.md` and `README.md` in full. Their standing user instructions apply to this repository.
3. Read `docs/handoff-current.md` and `docs/project-status.md` for the current pause checkpoint and ordered next work.

## Use the running app through MCP

The user requested agent-agnostic direct control. The `unified-control` MCP connection is registered for Codex and Claude.
Start with its advertised `uc_get_status` and `uc_get_layout` tools. Use MCP for supported status/configuration work; use
computer use for visual review or a capability gap. Do not claim a tool connection that is not exposed in the current session.
If the tools are absent, check the registered connection and reload MCP/start a fresh client session as needed.

UC must remain running in its visible tray. Installed0.9.2/source8c194f2 and engine4.21.1/source77c9086 retain cross-device controls and shared
hardware/AI/music/weather ownership. UC owns one hardware collector and one account reader; a host-supervised device-free
provider supplies music/weather. Worker caches/pixels retain original expiry, replaced local readers stay off, and an idle
provider can retain bounded weather cache without native/network polling. Host-only profiles, independent supervised
workers, safe recovery, verified cooler migration and background Live remain. The user granted standing permission on
2026-11-03 for tested idle-safe updates; defer while busy and verify after a quiet restart.
Local agent access was enabled by the user and actual Codex model-client read/configuration/undo access was
verified. Read current state rather than asking the user to enable it again without evidence. Do not start the installed GUI
from a packaged shell: it can see a stale main executable. The bundled MCP adapter uses the dedicated request-only proxy.

For layout changes: read the revision, preview the typed changes, apply with `expectedRevision`, then read back. Preserve
focused edits and failed saves. On busy/conflict, wait/read again; after a timeout, read back before retrying. Do not edit
AppData profile files directly. No physical action, USB reset or firmware command is exposed by this MCP checkpoint.

## Resume safely

The user requested stand-down and handoff on2026-11-13. Read docs/handoff-current.md.
Current MCP reports dock connected but cooler Live disconnected with write_timeout.
The2026-11-09 physical recovery is historical. Connection diagnostics remain on.
Temporary host heartbeat is installed in0.9.2 but its saved switch is off and no
real heartbeat trace exists. Windows startup Run entry targets the installed app
with --tray; actual sign-in startup is untested. Plan2.4/shared-services2.6/software2.7
are complete; physical2.7 acceptance remains open. Installer transition2.8 remains
the wider next feature step. Preserve the standalone fallback and existing
rollback gates. No automatic reset is authorized.
Read the current branch/status before new edits and create a `codex/` branch for new work. Do not retag or reinstall an app
for documentation changes. Keep the repositories private and user profiles/authentication/local diagnostics out of Git.
Follow `CLAUDE.md` for quiet simulated testing, idle-safe installation and hardware limits.
