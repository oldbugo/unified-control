# Unified Control: Codex startup

This repository is the combined device app. The separate Ulanzi Toolkit repository has its own usage-plugin scope.

1. Read `C:/Users/pc/Documents/knowledge-vault/Start here.md` in full and perform its safe remote-refresh check.
2. Read `CLAUDE.md` and `README.md` in full. Their standing user instructions apply to this repository.
3. Read `docs/handoff-0.3.0.md` for the current checkpoint and unresolved work.

## Use the running app through MCP

The user requested agent-agnostic direct control. The `unified-control` MCP connection is registered for Codex and Claude.
Start with its advertised `uc_get_status` and `uc_get_layout` tools. Use MCP for supported status/configuration work; use
computer use for visual review or a capability gap. Do not claim a tool connection that is not exposed in the current session.
If the tools are absent, check the registered connection and reload MCP/start a fresh client session as needed.

UC 0.3.0 must remain running in its visible tray. Local agent access was enabled by the user and read-only layout access was
verified. Read current state rather than asking the user to enable it again without evidence. Do not start the installed GUI
from a packaged shell: it can see a stale main executable. The bundled MCP adapter uses the dedicated request-only proxy.

For layout changes: read the revision, preview the typed changes, apply with `expectedRevision`, then read back. Preserve
focused edits and failed saves. On busy/conflict, wait/read again; after a timeout, read back before retrying. Do not edit
AppData profile files directly. No physical action, USB reset or firmware command is exposed by this MCP checkpoint.

## Resume safely

The completed 0.3.0 work is consolidated on `main`; the immutable `v0.3.0` tag identifies the installed build source.
Read the current branch/status before new edits and create a `codex/` branch for new work. Do not retag or reinstall 0.3.0
for documentation changes. Keep the repositories private and user profiles/authentication/local diagnostics out of Git.
Follow `CLAUDE.md` for quiet simulated testing, idle-safe installation and hardware limits.
