# Local agent control (MCP)

The 0.3.0 checkpoint lets compatible MCP clients inspect UC and configure dock layouts through the running app.
The same server works with modern MCP discovery and older initialize clients. No model or provider API key is embedded.

## Connection and lifecycle

Enable **General > Local agent access** in UC. It is off by default and can be disabled at any time.
UC must already be running in the tray. A client never starts its GUI or another hardware engine.
Quitting a client disconnects its adapter and leaves UC running; quitting UC makes tool calls report app unavailable.
There is no Windows service or network listener.

The included stdio adapter uses the bundled Node runtime and official MCP SDK 2.4.0. It invokes only UC's
request-only `--agent-request` mode through a dedicated `UC-AgentRequest-v1.exe` identity in the install folder. The installer creates
this copy of the reviewed app binary, so a packaged client's stale virtualized main executable cannot become a GUI launch.
There is no fallback to the main executable. That mode runs before GUI/single-instance setup and forwards a bounded request to
the existing host. A Windows named pipe has a protected current-user/SYSTEM ACL, rejects remote clients and checks the
server user's identity. Installed, debug and isolated test scopes are separate. Neither client reads UC's AppData files;
this also avoids packaged-shell AppData redirection.

## Tools

| Tool | Result |
| --- | --- |
| `uc_get_status` | App version, available operations and access state; enabled access also provides bounded device/usage status and profile metadata |
| `uc_get_layout` | One page's keys, profile/page IDs and current revision |
| `uc_list_monitors` | Read-only choices for configuring a monitor action |
| `uc_preview_changes` | Validated proposed layout and operation summary, without saving or executing actions |
| `uc_apply_changes` | One durable revision-checked batch, accepted state and undo token |
| `uc_undo_changes` | Restore the last agent batch, provided no later layout edit has occurred |
| `uc_get_job` | Job state/progress, without exposing raw job errors or payloads |

Read the latest layout, preview a patch and apply it with its `expectedRevision`. At most 64 typed operations are accepted:
`set_face`, `set_action`, `rename_page` and `create_page`. Keys are numbered **1–14**, with 14 the wide display; faces are
exactly `a` and `b`. Existing IDs select profiles/pages; UC generates IDs for new pages. The host validates the entire
result before saving once. An invalid operation saves none of the batch. No generic JSON patch or full-layout replacement
is exposed to MCP. Static Face A rendering and complete-key patches also exist in the host API, outside these seven tools.

Example operation (obtain IDs/revision from `uc_get_layout`):

```json
{"op":"set_face","profileId":"<profile ID>","pageId":"<page ID>","key":10,"face":"a","value":{"kind":"text","text":"Monitor"}}
```

## Editor coordination and results

Every UI save and agent write checks the current revision. A stale revision returns `revision_conflict`; read again and
recompute the change. Before an agent write, a live editor refuses active text entry or a failed save. Otherwise it freezes
editing, waits for imports and queued writes to finish, then acknowledges the lease. The host commits against the expected
revision. The editor adopts the accepted layout, clears stale undo history and reloads authoritative state before unlocking.
A lease timeout during a slow import cannot unlock the editor before that import finishes. Failed drafts are kept.

Quit/update respect active agent writes. Cancellation or a request timeout does not prove a write was cancelled: read back
before retrying. Application writes return saved state; physical delivery/recovery remains a separate device status/result.
Configuring a monitor/media/launch action never executes it. This checkpoint exposes no physical action, reset, firmware,
shell execution, account-authentication or application-start tool.

Installed 0.3.2 includes bounded numerical dock timing in enabled `get_status`: stage durations/counters and
one latest successful navigation sample. Face time includes provider/helper waits; ZIP and transport time are combined.
Host receipt-to-delivery excludes the earlier physical press-to-host interval. No layout names/action values/account
readings are added to timing. Disabled access still exposes only version/capabilities. These diagnostics do not establish
a measured physical bottleneck or speedup. [Capture guide](navigation-timing.md).

Installed 0.3.3 adds `cooler.diagnosticsEnabled` and `cooler.lastHealthSample`. The existing `cooler.state` describes the
supervised worker, while the new object is a strict projection of already cached diagnostics. `lastSampleAt` timestamps
the worker-health result; `pnpSampledAt` timestamps the separately cached Windows USB result. Neither is renewed by an
MCP read. Finite state/error codes, booleans and bounded counts are included; PIDs, device/account identifiers, paths,
raw messages and payloads are omitted. Disabled agent access exposes none of these fields. A cached connection result
does not establish current physical operation, especially across worker restart or delayed sampling.

## Client setup

Use absolute installed paths, for example:

```text
command: C:/Users/<user>/AppData/Local/Unified Control/node/node.exe
args: ["C:/Users/<user>/AppData/Local/Unified Control/agent-mcp/server.mjs"]
```

Codex registration uses `codex mcp add unified-control -- <command> <server path>`. Codex's desktop/CLI/IDE configuration
is shared. Claude Desktop uses the same command/arguments under `mcpServers.unified-control` in its desktop configuration;
the Windows packaged app uses its own package's Roaming/Claude configuration folder. Preserve other entries. Reload the
client's MCP configuration or start a fresh client session; Claude Desktop normally needs a full quit/reopen by the user.
Client registration does not enable UC's access switch.

Claude's Desktop Code tab uses the user-level `mcpServers` entry in `~/.claude.json`, separately from the chat app's
`claude_desktop_config.json`. Register the same command in both if using both interfaces. Existing entries/settings must be
preserved. [Claude Desktop configuration](https://code.claude.com/docs/en/desktop).

Official references: [MCP server architecture](https://modelcontextprotocol.io/docs/learn/architecture),
[official SDK](https://github.com/modelcontextprotocol/typescript-sdk),
[Codex MCP setup](https://learn.chatgpt.com/docs/extend/mcp?surface=cli),
[Claude Desktop setup](https://py.sdk.modelcontextprotocol.io/get-started/real-host/).

## Verification

Software checks include the full Rust suite, actual Windows pipe ACL/identity/limits/shutdown, official-SDK stdio clients
(modern and legacy protocol), pure editor/revision tests and headless editor fixtures. The quiet native fake test connects
SDK -> request proxy -> host -> real UI IPC/storage, including access opt-in, preview/apply/undo, two clients, stale revisions,
focused text, an expired lease during a held import, busy-safe Quit, hidden editor and client disconnect. No automated physical
device, display, audio or account action is used. Client-host acceptance and installation are recorded separately.

Run `scripts/check-agent-mcp.cjs`, `scripts/check-agent-editor.cjs` (with `UC_PLAYWRIGHT_MODULE`) and
`scripts/check-native-agent.cjs` (with `UC_TEST_EXE` pointing to a debug build). Native tests refuse release/installed executables
and use their own temporary home plus quiet/multi/fake flags.

Installed 0.3.0 / source `0909429` through the idle-safe updater; startup/version/new process verified. The dedicated proxy
has the same SHA-256 as the installed app and reports 0.3.0 in both the packaged shell and normal filesystem view. The
installed bundled Node/production adapter connected through the official SDK, listed seven tools, returned 0.3.0 and
correctly rejected layout access with `agent-disabled` while the opt-in was off. No GUI automation was used for this check.

The `unified-control` connection is registered for Codex, Claude Code (including the Desktop Code tab) and Claude Desktop
chat. Existing configuration is preserved; the Claude Code file has a local backup. The user enabled Local agent access;
the installed production MCP connection then returned access enabled, seven tools, a readable 14-key layout and dock Connected.
This was read-only. Clients still need their configuration reloaded/fresh sessions; actual client-model tool use is not
claimed verified by that earlier check. No installed-user layout was modified during that read-only check.

On 2026-11-02, a fresh Codex desktop chat discovered and called the actual registered tools. Status, scoped layout,
read-only monitor discovery and preview passed. A temporary text face was saved in an inactive empty profile, read back,
then restored through the returned undo token; all fourteen keys matched the baseline afterward. Reusing the old revision
was correctly rejected with `revision_conflict`. The active Everyday page remained selected and the dock stayed connected.
This establishes Codex model-client read/configuration/undo acceptance. Claude model-client acceptance remains open.

Installer SHA-256: `278B4392AF581077D2EFCE4F9404730F71F39B9F247F9DB818E0C6F42928297F`.
