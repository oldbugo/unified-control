# AI usage adapter

The app owns `core::usage::UsageService`. It starts no process at construction.
Only `set_active_providers` starts the managed Node helper. Empty configuration,
`shutdown` and Drop end that owned helper. An inactive `render` returns an error
for the caller to draw unknown; it never starts a reader or returns a cached PNG.
The app must keep all usage requests off its UI thread. Fake/quiet application
tests leave providers inactive; explicit standalone fixtures contain no accounts.
`is_running` checks actual child exit and pipe health, not just a stored process
object. `check_health` adds an account-free private-pipe ping with a 500 ms deadline.
A parent supervisor can use this every 30 seconds and retry desired providers
with backoff; neither health method starts a process on its own.

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

Codex uses dedicated `app-server` children, existing sign-in and only read-only
account methods. It does not start model work. Each quota/token transaction is
bracketed by `account/read` with `refreshToken:false`. Missing/malformed ChatGPT
identity metadata, a changed before/after identity, or an `account/updated`
notification during the transaction makes the result unknown. This API contract
was checked using the installed CLI's generated JSON schema and
[official App Server documentation](https://learn.chatgpt.com/docs/app-server).
Schema generation does not open an account session or read an authentication file.

Email, plan type and optional workspace display name are processed only inside
the Node helper to form a process-salted HMAC fingerprint. A WeakMap side channel
binds the normalized result to it; neither metadata nor fingerprint is enumerable,
sent through app pipes, logged, or persisted. The current API exposes no unique
workspace identifier, and email may be null. Null identity is unknown. Matching
display metadata cannot prove that two same-name workspaces are identical; real
account/workspace-switch acceptance remains pending. No full switch reliability
claim is made from these software tests.

Limits refresh at most once per minute. Token totals refresh only when used, after
each successful quota revision, on that same minute cadence. Every new quota
revision discards the previous token cache even when exposed identity metadata
matches. Pending token responses require the same private identity and quota
revision; late results from an earlier revision are dropped. Changed/unverified
identity clears quota history and tokens; token identity differing from the latest
quota hides both until a new bracketed quota read succeeds. Failures do not spin
additional account reads or fall back to previous account data.

Quota history contains only sanitized percentages/reset times from real
observations, with seven-day retention. It lives in `UC_HOME/usage` and is never
used as a current reading. No current numerical PNG is persisted or replayed.
Because no account key is persisted, the first verified Codex identity in a new
helper clears any previously loaded Codex history rather than assuming ownership.
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
The isolated checks passed 61 Node tests and 11 Rust usage-slice tests.

No live account calls, installed-plugin changes, hardware checks or packaging
acceptance were performed by this task. The original Toolkit reader's recorded
live evidence is source provenance, not proof this integration works live. Parent
integration owns module registration, app lifecycle, UI, resources and installation.

Run Node tests with `npm ci --ignore-scripts` then `npm test` in
`companions/usage-reader`. Rust usage tests need its Node dependencies installed
and the standard Node executable (or `UC_TEST_NODE`). They only execute fixture
scripts. The helper needs Node and the pinned resvg native package; Windows uses
the standard Segoe UI regular/bold fonts without scanning all installed fonts.
