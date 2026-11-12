# Cooler data migration

UC 0.5.1 completes the independent cooler data-copy path. Installation and real-data results are recorded separately in acceptance.md.

First enable copies settings, companion configuration, library, gallery and optional lyrics offsets into UC's own `devices/cooler` folder. Known active file references are rebased. Existing dock profiles and later UC cooler edits are preserved. The original CoolerScreen data and gallery remain untouched as rollback.

The V2 upgrade also copies custom JSON source definitions once into UC-owned `Sources`, then publishes its completion marker. The worker receives that directory explicitly; an inherited source-directory environment variable cannot redirect it into the old app's folder. Existing differing UC definitions stop the upgrade for review rather than being overwritten. Later UC edits and deletions do not synchronize back to the old copy. User-selected external source folders remain intentional references.

## Verification

General shows **Cooler data migration** and **Verify imported data**. One automatic host job runs after migration or an incomplete previous check. A successful historical receipt skips repeat scans at startup. The explicit button starts a new check. Status and MCP reads expose a cached finite summary and never scan media.

The host compares file presence and streamed media bytes, settings, companion data, lyrics offsets and known active references. JSON differences may reflect later UC edits and are counted without overwriting them. Historical metadata backups are not active dependencies. The three generated theme preview filenames directly inside an item with valid theme metadata are counted separately when their bytes differ; originals and nested assets remain strict. Missing media, old-data dependencies, unreadable records, unstable files or incomplete source ownership prevent success. A new setup with no imported settings is labelled separately.

This is a bounded, cancellable disk comparison, not a recurring background reader. It uses no USB commands and leaves screen playback running. Quit waits for the visible job; Cancel stops scanning. Host-side receipts contain counts/flags and a timestamp, never record names or paths. They are bounded, validated and atomically published. A pending check invalidates older success across cancellation, write failure and restart.

## Rollback and remaining release work

The old standalone installation is retained as fallback. Quit UC before opening it, and quit standalone CoolerScreen before enabling UC. There is no automatic dual ownership or two-way data synchronization. No old data is deleted.

The original phase 2.8 proposal to replace the standalone installer in place remains a separate release step, after the outstanding phase 2 architecture/release gates. This data migration does not claim dock-worker extraction, shared providers, cross-device actions, both-device sleep/resume or a one-hour release soak. Sleep/resume remains deferred by the user.

## Installer preparation 2026-11-09

[Installer-transition tooling](installer-transition.md) adds a read-only shell inventory and owned synthetic upgrade/backup/rollback rehearsal.33 checks pass. It cannot retire either real installation, does not audit real AppData, and never treats a historical migration result as current authorization. Actual immutable snapshot, identity/shortcut transition and NSIS/real 4.x rollback acceptance remain separate work behind the release gate.
