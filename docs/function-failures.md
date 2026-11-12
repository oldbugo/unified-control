# Reported dock function failures
*Document type: dated evidence. Facts are true for the date shown. For the current state, read [status](status.md).*

Historical status at the 2026-11-04 pause (current state: [status](status.md)): Monitor Toggle is user-confirmed working after 0.2.3. The AI Usage reader launch defect is fixed in 0.3.1, with both providers verified beyond the freshness cutoff and observed transient recovery. Remaining usage work is physical/account-state acceptance and reliability follow-up, not an unimplemented reader. See [ordered project work](status.md).

Historical report, 2026-11-01: AI Usage and Monitor Toggle did not work. The diagnosis and fixes below retain their dates.

Private follow-ups: [AI Usage #6](https://github.com/oldbugo/unified-control/issues/6) retains physical/account-state acceptance and reliability follow-up. [Monitor Toggle #7](https://github.com/oldbugo/unified-control/issues/7) is now user-confirmed working after the 0.2.3 path fix.

## Monitor Toggle

2026-11-02 live diagnosis: Key 10 had an empty action value because the picker failed to discover displays. The helper
worked from ordinary paths, including UC's read-only Rust test, but failed under the installed app. A temporary local-only
error capture identified PowerShell `Join-Path` rejecting the verbatim canonical resource path (`ArgumentNull`). The capture
was removed. A targeted temporary installed-helper path correction restored the live picker (two displays); the user
confirmed the physical key worked after selecting only the Dell and labelling both faces Monitor.

Permanent fix in 0.2.3: normalize trusted verbatim disk/UNC filesystem paths before passing the helper script and snapshot
path to PowerShell. Preserve UTF-16 and leave target identifiers/device namespaces unchanged. The original helper files stay
unchanged. A canonical-resource regression reproduced failure before the fix and passed afterward; pure disk/UNC/Unicode
tests, headless picker retry/save tests and native fake discovery IPC pass. Installed startup/new PID and original helper hashes
were verified; the final computer-use picker check was stopped with Escape. The user subsequently confirmed Monitor Toggle works.
Automated checks never change display topology or audio. User profiles, targets and snapshots remain local.

Read-only audit found a concrete adapter defect: the preserved PowerShell helper can return `displays` as an object for exactly one monitor and null for no monitors, while UC required an array. UC now normalizes null/object/array before validating stable keys and readable names. Missing/malformed shapes still fail. Pure fixture regression covers zero/one/many shapes and rejects malformed single objects; the original helper is unchanged. This may explain discovery failure on a one-display setup, but has not been established as the user's cause.

The action also needs an explicit selected target. An empty target list is rejected; assigning the action alone does not choose a display. Tests never change monitor topology or audio.

## AI Usage

2026-11-02 diagnosis: live MCP reports `reader-unavailable` with both selected providers `reader-inactive`. The installed
bundled reader passes its account-inactive health/render/shutdown smoke check from an ordinary filesystem path. A Rust
fixture reproduces the failure when the script path has the canonical Windows `\\?\` prefix: Node exits before replying.
The prepared 0.3.1 change normalizes trusted disk/UNC paths at the Node process boundary, preserving UTF-16 and other
device namespaces. The same canonical-path regression fails before the fix and passes afterward without account access.
This fixes a reproducible launch defect; sustained account refresh and physical usage acceptance are checked separately.

0.3.1 was installed after explicit approval using `tools/install-when-idle.ps1`. Startup/new PID, MCP appVersion0.3.1,
dock reconnection and the dedicated request proxy SHA-256 matching the built executable were verified. Codex immediately
reported verified usage; Claude changed from stale to verified within the first minute, without an agent-issued refresh
or ordinary model request. The inactive profile still matched all fourteen baseline keys after the update.

Bounded live watch, 2026-11-02: both providers were verified at 02:24:11 UTC and again at 02:29:26 UTC (five minutes fifteen
seconds later), with reader ready/dock connected throughout the sampled checks. Codex briefly reported unavailable at
02:26:53 UTC, then recovered without intervention by 02:27:42 UTC. Claude remained verified in those samples. MCP status
inspection does not request account refresh. This establishes fresh readings beyond the cutoff and observed automatic
recovery, not uninterrupted reliability, exact account-panel agreement, physical pixels or account-switch acceptance.
The transient's cause remains unknown. Claude still depends on a supporting authenticated Desktop Code companion session.

The 0.3.2 diagnostic correction preserves guarded Codex `error` and `stale` statuses instead of collapsing every non-ok
result without identity into unavailable. Otherwise-ok readings without verified identity still become unavailable.
Fixture tests verify number/history/token suppression, minute throttling and scheduled recovery. This is included in
installed 0.3.2 and does not identify or fix the transient source failure.

Installer: `dist/UnifiedControl_0.3.1_x64-setup.exe`. Executable version 0.3.1 and embedded
source e23be94 were verified. Installer SHA-256: `518D2E48D5E86A19E4DCD92DD69E67153909A74984985DD56DD54B34AF49DD61`.
All 120 core/11 host tests, touched-file rustfmt, official-SDK MCP fixtures and the staged resource inactive-reader
health/render/shutdown check pass. The resource smoke check uses the combined packaging layout: runtime Node/reader plus
`resources/monitor`, rather than treating `runtime` alone as a complete install root. Whole-repository formatting reports
unrelated existing differences. No account source or physical action was activated by these checks.

The managed reader and all presentations exist. Runtime activation/render errors were previously swallowed while unknown faces were correctly rendered. `RuntimeResources::usage_status()` now exposes only fixed reader/provider codes and messages, including missing bundled runtime/reader, start failure, timeout, stale observations and unavailable/unverified data. No account IDs, quota values, filesystem paths or raw helper/OS messages are returned. Reader code transitions are logged without payloads. The host can add this value to diagnostics state.

Unknown/stale/error/inactive snapshots now retain distinct fixed messages instead of every reason becoming generic unavailable. Untrusted helper text is still discarded. Freshness, account provenance, reset expiry, receipt/render/delivery gates and unknown physical fallback remain unchanged. Missing-resource and stale/unverified regressions verify useful reasons without starting a reader or allowing numerical gauges. These diagnostics do not fix an absent fresh Claude Desktop source or establish the user's source/helper failure; physical/account acceptance remains pending.
