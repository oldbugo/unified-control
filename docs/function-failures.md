# Reported dock function failures

User-reported 2026-11-01: AI Usage and Monitor Toggle did not work. Both are implemented; fake/software checks do not establish their physical behavior. CoolerScreen integration is the user's current priority, followed by UI refinement.

Private follow-ups: [AI Usage #6](https://github.com/oldbugo/unified-control/issues/6) and [Monitor Toggle #7](https://github.com/oldbugo/unified-control/issues/7). The fixes/status messages are included in 0.2.0; neither issue is claimed physically resolved.

## Monitor Toggle

Read-only audit found a concrete adapter defect: the preserved PowerShell helper can return `displays` as an object for exactly one monitor and null for no monitors, while UC required an array. UC now normalizes null/object/array before validating stable keys and readable names. Missing/malformed shapes still fail. Pure fixture regression covers zero/one/many shapes and rejects malformed single objects; the original helper is unchanged. This may explain discovery failure on a one-display setup, but has not been established as the user's cause.

The action also needs an explicit selected target. An empty target list is rejected; assigning the action alone does not choose a display. Tests never change monitor topology or audio.

## AI Usage

The managed reader and all presentations exist. Runtime activation/render errors were previously swallowed while unknown faces were correctly rendered. `RuntimeResources::usage_status()` now exposes only fixed reader/provider codes and messages, including missing bundled runtime/reader, start failure, timeout, stale observations and unavailable/unverified data. No account IDs, quota values, filesystem paths or raw helper/OS messages are returned. Reader code transitions are logged without payloads. The host can add this value to diagnostics state.

Unknown/stale/error/inactive snapshots now retain distinct fixed messages instead of every reason becoming generic unavailable. Untrusted helper text is still discarded. Freshness, account provenance, reset expiry, receipt/render/delivery gates and unknown physical fallback remain unchanged. Missing-resource and stale/unverified regressions verify useful reasons without starting a reader or allowing numerical gauges. These diagnostics do not fix an absent fresh Claude Desktop source or establish the user's source/helper failure; physical/account acceptance remains pending.
