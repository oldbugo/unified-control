# Unified Control MCP adapter

This is a local stdio MCP server for the **already running** Unified Control
app. The adapter starts only a request-only UC proxy (`--agent-request`), which
returns an error when the app is closed. It never starts a GUI, a second hardware
engine or a service. Agent access is a visible opt-in in UC's General settings.

The same tool definitions serve compatible MCP clients through the official SDK,
including modern discovery and older `initialize` clients. The client handles its
own model and permissions. No provider API key or model is built into UC.

## Installed entry

Use the bundled Node executable as the MCP command:

```text
command: <UC install folder>/node/node.exe
args:    ["<UC install folder>/agent-mcp/server.mjs"]
```

The installed adapter resolves `../UC-AgentRequest-v1.exe` relative to
`server.mjs`. UC's installer creates that dedicated proxy from the current
reviewed binary. This filename exists only for versions that implement the
request-only protocol. A packaged client can see an older virtualized
`UnifiedControl.exe`; the adapter never launches or falls back to that main
executable. A missing dedicated proxy returns a fixed unavailable error.
There is no command, executable, environment or path override in the production
entry. All requests go through the running app, which owns validation, durable
saves, editor coordination, revisions, device queues and diagnostics.

## Tools

- `uc_get_status`: version, capabilities, access state and bounded device status.
- `uc_get_layout`: one page, stable IDs and its current revision.
- `uc_list_monitors`: read-only monitor choices; no topology or audio change.
- `uc_preview_changes`: validate a proposed patch without saving it.
- `uc_apply_changes`: apply a validated batch against its expected revision.
- `uc_undo_changes`: revert the last agent change if its revision still matches.
- `uc_get_job`: inspect a job result without executing anything.

Layout operations are `set_face`, `set_action`, `rename_page` and `create_page`.
Key numbers are **1–14**; key 14 is the wide display. Each key has faces `a` and
`b`. `set_face.value` is a UC face object. The backend validates all changes again.
No action execution, firmware, USB reset, arbitrary command or direct app-data
access is exposed. An applied configuration does not establish physical delivery.

Read the layout and revision, preview changes, then apply against that revision.
If the editor is busy, wait. If the revision conflicts, read again and recompute
the patch. A timeout or cancellation can occur after the host admitted a write;
read back before retrying. Never silently replace the user's unsaved draft.

## Build and tests

The Node distribution is already staged by UC's release build. Copy `server.mjs`,
`package.json`, `package-lock.json` and `licenses/` into `runtime/agent-mcp`, then
run `npm ci --omit=dev --ignore-scripts` **in that isolated staging folder**.
The production dependency set is the pinned official MCP server/core plus Zod.
The MCP client is a dev-only fixture-test dependency.

Run `npm ci --ignore-scripts` here and then
`node scripts/check-agent-mcp.cjs` from the repository root. This test connects
the official SDK client over stdio, exercises all tools and validation, verifies
legacy MCP compatibility, and uses a disposable fake proxy. It never launches
the installed app or reads profiles/settings/accounts/hardware.

For isolated native integration, `createProcessRequester(command, args)` and
`createServer({request})` / `startStdio({request})` are exported. A test wrapper
can supply the quiet debug request-only binary. These exports do not add runtime
override flags to the installed entry.

## Third-party sources

- Official SDK source and compatibility: <https://github.com/modelcontextprotocol/typescript-sdk>
- MCP concepts: <https://modelcontextprotocol.io/docs/learn/architecture>
- `@modelcontextprotocol/server` and `@modelcontextprotocol/core` 2.4.0:
  Apache-2.0 / retained MIT contributions; full packaged notices in `licenses/`.
- Zod 4.6.7: MIT, full packaged notice in `licenses/`.

Exact package tarballs and integrity hashes are pinned in `package-lock.json`.
