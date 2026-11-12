# Unified Control: agent startup

This repository is the combined device app. The separate Ulanzi Toolkit repository has its own scope.

1. Read `C:/Users/pc/Documents/knowledge-vault/Start here.md` in full. Do its safe remote-refresh check.
2. Read `CLAUDE.md` and `docs/status.md` in full. The standing user instructions apply here.
3. Read Git status. Create a branch named for your agent (for example `claude/` or `codex/`) before you edit.

## Use the running app through MCP

The `unified-control` MCP server works for any agent. Start with `uc_get_status` and `uc_get_layout`.
Do not claim a tool that your session does not show. If the tools are missing, check the MCP registration and start a fresh client session.

- UC must stay running in the visible tray. Local agent access must be on in General. Read the state first. Do not ask the user to turn it on without evidence.
- To change a layout: read the revision, preview the change, apply it with `expectedRevision`, then read it back.
- On busy or conflict: wait, read again, then retry. After a timeout, read back before you retry. Keep the user's edits.
- Do not edit AppData profile files directly.
- No physical action, USB reset or firmware command is exposed. Do not look for a way around this.
- Do not start the installed GUI from a packaged shell. It can see a stale main executable.
- Follow `CLAUDE.md` for quiet fake-device tests, idle-safe installs and hardware limits.
- Keep the repositories private. Keep profiles, sign-in data and local logs out of Git.
