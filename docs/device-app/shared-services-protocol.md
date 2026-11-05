# Shared AI, music and weather services

Plan2.6b. Software implementation verified2026-11-05; installed acceptance is recorded separately.

## Ownership and activation

UC supervises the only account-reading Node helper. The existing usage engine keeps provider-specific polling, private account identity, quota history, token-history separation and logout barriers. Interested dock profiles declare providers; both faces of the delivered page and bounded recent editor previews declare required raster presentations. Previews do not activate an account provider absent an enabled active-profile selection. No account identity or copied authentication file crosses worker IPC.

Music and weather reuse the cooler engine's source code in a provider-only process, supervised by UC. The process enters before any device lease, application, media server or window startup. Its private inherited pipe accepts bounded typed demand and snapshots only. It has no device operation or arbitrary command interface. Cooler shared mode consumes injected readings and never starts a replaced local music watcher or weather fetcher. Standalone CoolerScreen retains its local mode. Once started, the provider process can remain idle until UC quits, with native watchers and HTTP interest off. This preserves the existing bounded in-memory weather cache and retry timing across quick widget changes; it is not an always-on poller. A cold app with no demand never starts it. Source logs use a UC-owned folder, never the standalone app folder.

Demand carries only the explicit music application preference and chosen weather place needed by the source. Music/lyrics widgets select music; the historical live.music switch is not the current widget activation rule. Weather requires a displayed weather widget and a chosen place. No automatic location discovery or newly enabled integration is added.

## Timing and validation

All messages reject unknown fields and enforce bounds. Channel generation, increasing sequence and original timestamps prevent replay; consumer caches cannot renew source observation age. A three-second delivery lease makes stale host/worker communication unknown even when a source has a longer cache policy. Empty/expired demand stops native watchers and forecast polling; a slow consumer cannot block another service.

AI frames include their guarded UsageSnapshot and original earliest numerical deadline: five-minute quota freshness, quota reset and applicable token-history expiry. Publication, receipt, rendering and delivery each retain the account/freshness gate. Cached numeric images are replaced on source failure/logout, and are suppressed after transport expiry. No render command accepts snapshots from a user interface.

Music transports position at its original successful observation, playing/paused state and the original state-change time. Receipt does not restart position extrapolation, the paused-panel minute or track identity. Known no-session and failed reads are distinct. Artwork stays bounded, with decoded dimensions at most640x640 and PNG at most2MiB; services frames are at most8MiB.

Weather keeps Celsius data and forecast detail, a15-minute refresh cache,60-second retry and120-second idle fetch-thread policy, with at most eight place entries retained in memory. The cache is keyed by explicit place; a changed city cannot briefly display the previous city's data. Fetch failure can retain only matching-place data with its stale marker and original observation time. Pipe delivery never claims a new weather fetch.

## Verification boundary

Automated checks use isolated quiet debug instances, synthetic account/music/weather sources and fake devices. They must establish one source owner, no replaced local polls, profile/widget activation, cache and time preservation, failure/expiry/recovery, both-way worker crash independence and safe Quit. Actual account agreement, physical readability and long-run reliability remain separate acceptance.

Software gate:193 UC core/11 host/62 reader tests pass (three existing ignored UC checks), plus five cooler receiver cases, seven music, four weather and nine worker/source cases. Headless interface/cooler checks and the isolated native shared-services/profile/MCP suites pass. The source fixture proves one owner/no local duplicates, idle cache reuse, provider failure/auth modes, original expiry, configuration isolation, provider/device-worker crashes and safe Quit. Existing hardware/dock-worker/cooler and final shared-services/lifecycle native checks pass. Installed verification is recorded in acceptance.
