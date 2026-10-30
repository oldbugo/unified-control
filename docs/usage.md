# AI usage adapter

The app owns `core::usage::UsageService`. It starts no process at construction.
Only `set_active_providers` starts the managed Node helper. Empty configuration,
`shutdown` and Drop end that owned helper. An inactive `render` returns an error
for the caller to draw unknown; it never starts a reader or returns a cached PNG.
The app must keep all usage requests off its UI thread. Fake/quiet application
tests leave providers inactive; explicit standalone fixtures contain no accounts.

## App contract

```rust,ignore
let service = UsageService::new(node_executable, reader_script, uc_home);
service.set_active_providers(&[
    ActiveProvider { provider: Provider::Codex, token_history: false }
])?;
let rendered = service.render(Provider::Codex, "rings", 196, 196)?;
// rendered.png: validated PNG bytes; rendered.snapshot: sanitized current state
service.shutdown();
```

The caller derives active providers from enabled, configured keys, including
both A/B faces. `token_history` is true only when a configured Codex face needs
actual daily token history. Claude cannot request token history. Quota readings
and daily token totals are separate fields with separate timestamps.

Provider values serialize as `claude` / `codex`. Status values are `loading`,
`ok`, `stale`, `unavailable`, `error`, `inactive`. `UsageSnapshot` uses camelCase:
`observedAt` (milliseconds), `provenanceVerified`, `fiveHour`, `weekly`,
`tokenHistoryStatus`, `tokenHistoryObservedAt` (milliseconds), `tokenHistory`,
`message`. Each quota window is `{usedPercent, resetsAt}`; reset time is Unix
seconds. Daily entries are `{date: "YYYY-MM-DD", tokens}`. No account/device IDs,
credentials, authorization handles, transcripts or raw server errors cross pipes.

The 16 original Toolkit presentations are:

| View | Purpose |
| --- | --- |
| `rings`, `bars` | Both remaining allowances |
| `trend` | Both quota observation trends |
| `tokens` | Actual daily token totals, Codex only |
| `resets` | Both reset countdowns |
| `five-hour-ring`, `weekly-ring` | One allowance ring |
| `five-hour-number`, `weekly-number` | One large remaining percentage |
| `five-hour-reset`, `weekly-reset` | One reset countdown |
| `five-hour-time`, `weekly-time` | Sydney reset date/time |
| `five-hour-trend`, `weekly-trend` | One quota observation trend |
| `wide-dashboard` | Both allowances and resets |

Supported raster sizes are exactly 196×196 and 458×196. The reused SVG renderer
owns usage appearance; the general Rust renderer provides unknown when unavailable.
A key still has exactly A/B faces; the adapter does not cycle presentations.

## Freshness and lifetime

The provider publication gate and every render require verified source provenance,
an original observation timestamp and a reading no older than five minutes.
Future timestamps more than one second ahead are rejected. Invalid or expired
windows disappear independently. Logout/auth rejection removes current numbers
as soon as detected; an error never renews an observation timestamp. Detection is
bounded by the source schedule: Codex quotas at most once per minute; Claude's
sanitized logout/account files at most once per second, shared across keys.

The copied Claude reader enforces companion version/source/account binding and
the independent account-reader status/logout barrier. It never reads Desktop
history or authentication files. When Claude is active, the helper starts or
shares the existing Toolkit coordinator on `127.0.0.1:19374`, using the same
`LOCALAPPDATA/UlanziToolkit/AIUsageMonitor` files. This is the existing companion
protocol, not app↔helper IPC and not another account polling schedule. Credentials
and authorization remain inside Claude. A supporting Claude Desktop Code session
still must be loaded for account refresh; closing all such sessions, closing
Claude or suspending the PC stops successful observations. The helper does not
change installed companion or plugin settings.

Codex uses a dedicated `app-server` child, existing sign-in and only read-only
account methods. It does not start model work. Limits refresh at most once per
minute; daily tokens refresh hourly only when used. Daily-token results expire
after two hours and are hidden whenever current account provenance is unavailable.
Provider failure clears token data. Exact account-switch behavior still needs
live acceptance; no continuity across account switches is claimed.

Quota history contains only sanitized percentages/reset times from real
observations, with seven-day retention. It lives in `UC_HOME/usage` and is never
used as a current reading. No current numerical PNG is persisted or replayed.
At Rust receipt, freshness is checked again. If even one window/token field was
stripped during rendering, the PNG is rejected even if another window remains
current. PNG bytes are decoded with bounded memory and fixed dimensions before
delivery. Parent runtime integration must re-render on reconnect and suppress
last-good usage images when a request fails or expires.

All app↔helper traffic is bounded JSON lines over inherited stdin/stdout. Requests
are single-flight with a five-second parent deadline and bounded line sizes;
write/read pipe threads cannot block that deadline. Child stderr is discarded.
Codex requests have their own 12-second ceiling and support abort on deactivation.
On Windows a dedicated kill-on-close Job contains Node and its descendants;
containment failure refuses startup. Shutdown first requests graceful stop, then
closes the Job and terminates only the owned process. No process-name kills or
existing application restarts occur. `NODE_OPTIONS`/`NODE_PATH` injection is removed.

## Evidence and checks

Built/tested software only: copied provider regression tests and injected-reader
tests exercise provenance, stale/reset/logout handling, activation, throttling,
abort, privacy and all 16 PNG views at both sizes. Rust standalone slice tests
exercise real private pipes with unknown-only Node fixtures, bounded hung/invalid
children, full PNG validation and receipt-time numerical suppression.
The isolated checks passed 53 Node tests and eight Rust usage-slice tests.

No live account calls, installed-plugin changes, hardware checks or packaging
acceptance were performed by this task. The original Toolkit reader's recorded
live evidence is source provenance, not proof this integration works live. Parent
integration owns module registration, app lifecycle, UI, resources and installation.

Run Node tests with `npm ci --ignore-scripts` then `npm test` in
`companions/usage-reader`. Rust usage tests need its Node dependencies installed
and the standard Node executable (or `UC_TEST_NODE`). They only execute fixture
scripts. The helper needs Node and the pinned resvg native package; Windows uses
the standard Segoe UI regular/bold fonts without scanning all installed fonts.
