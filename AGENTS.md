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

UC must remain running in its visible tray. Installed0.9.0/source7c3ee6d and engine4.21.0/sourceda2cb2c add cross-device controls and retain shared
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

The user resumed development on2026-11-06. Plan2.4/shared-services2.6 and software2.7 are complete. Cooler Live currently fails with write_timeout, reproduced on0.8.0 as well; physical2.7 acceptance remains open. No automatic reset is authorized. Installer replacement2.8 is the next implementation item. Completed work is consolidated on `main`; immutable release tags identify their original build sources.
Read the current branch/status before new edits and create a `codex/` branch for new work. Do not retag or reinstall an app
for documentation changes. Keep the repositories private and user profiles/authentication/local diagnostics out of Git.
Follow `CLAUDE.md` for quiet simulated testing, idle-safe installation and hardware limits.
