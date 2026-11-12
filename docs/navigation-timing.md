# Navigation timing checks

Available in installed 0.3.2. Read enabled MCP `uc_get_status` and inspect `dock.timing`. Status reads do not navigate or execute
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

## Physical check on 0.3.2

The user completed three Test-folder/Back cycles and reported responsive behavior. Live MCP then reported six started,
six completed and zero aborted navigations. The latest host-receipt-to-completed-transfer sample was 72.44 ms: handling
7.733 ms, wait before render 20.050 ms, input preparation 0.005 ms, face rendering 18.207 ms, delivery preparation
0.007 ms and combined ZIP/transport 12.491 ms. These instrumented stages do not include every overhead and need not sum
to the total. Earlier navigation durations were not retained: the bounded capture file contained no navigation samples.
This is one latest timing plus the user's three-cycle feel report, not an average, before/after speedup or physical pixel
latency measurement. No scheduling optimization was made in 0.3.2.

## 0.3.2 package identity

Installed source: `29414fb8db44db15ae2b3f90bf64be8af7080b77`. Installer SHA-256:
`19ADF0291ED59ED978B2E2281B08BB60681A0085F187B0AFE28E206126BCDDE3`.
The installed request proxy hash is `054D8E1ED3B3FEA1D32042267208F29B3BF35F4080468F01756CDA65955A18D4`.
Its bytes match the built release exactly after Tauri's three-byte bundle marker changes from `UNK` to `NSS` for NSIS.
The packaged shell's old main-executable view is not an authoritative installed-binary check; use the proxy and live MCP.
