# Navigation timing checks

Prepared for 0.3.2. Read enabled MCP `uc_get_status` and inspect `dock.timing`. Status reads do not navigate or execute
actions. Keep the installed app running; use its normal Folder/Back keys for a physical navigation sample.

The fixed-size state retains counters and one latest successful navigation. `receiptToDeliveryUs` starts when the host's
poll returns a button event and ends after successful full-page transfer. It excludes physical press-to-host time and
does not establish when the screen displayed the pixels. `render.faceRenderUs` includes any usage-helper wait.
`zipAndTransferUs` combines ZIP construction and transport writes. Preceding non-button events are a count, not queue depth.
All durations are microseconds. Divide by 1,000 for milliseconds.

Compare Folder into a static page and Back into a usage page, with the editor open and closed. Do not change action
assignments for the test. Keep physical feel separate from host timings. Scheduling is unchanged in this release; no
physical speedup or bottleneck is established by the fixture tests.

## Bounded local capture

From the source checkout, use the installed production adapter and runtime:

```powershell
$env:UC_MCP_NODE = Join-Path $env:LOCALAPPDATA 'Unified Control/node/node.exe'
$env:UC_MCP_SERVER = Join-Path $env:LOCALAPPDATA 'Unified Control/agent-mcp/server.mjs'
node scripts/capture-navigation.cjs 300
```

The script samples status every two seconds for at most ten minutes. It writes elapsed time and numerical timing fields
to a temporary local JSONL file, prints new completed-navigation summaries and closes its client when finished.
It leaves UC running. Unsupported builds/access-off/missing timing fail closed. It never runs key actions, reads profile
files or saves account readings. Keep captures local; they are diagnostics, not source artifacts.

Fake tests cover held rendering, FIFO ACK/heartbeat events, aborted navigation, access gating and numerical-only timing.
Physical Folder/Back measurements remain separate from these software checks.
