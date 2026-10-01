# Operational guarantees and recovery limits

## TS/React

Audit producers stage their output. Native single-file writers and audit publishers cooperate on `.audit.lock`; native readers refuse evidence while that lock or `.publication-incomplete` exists. Audit publication saves previous regular-file contents before replacement and publishes the verdict last. A caught publication error restores previous contents. If restoration also fails, the recovery marker and staging directory are retained and native readers/writers refuse further use. Baselines advance only after successful publication. Fault injection covers publication failure, failed restoration, concurrent access and baseline preservation.

Do not delete recovery markers blindly. First establish that the originating process is no longer active; retain a copy of the output/recovery directories. The marker identifies the staging directory, affected artifacts and which names previously existed. Restore remaining `.previous` copies; a previously existing name whose backup was already moved back must not be deleted. Names absent from the marker's `previous` list did not exist before the failed publication. Verify the entire restored set and input identity before removing markers. A lock without a recovery marker can also mean a live writer or a crash before publication. Recovery is deliberately not automatic.

This is **not** a filesystem/power-loss transaction: no fsync durability guarantee, no immutable generation pinned across multiple reader calls, and no guarantee for programs bypassing native locks. Non-audit measurement sequences remain independent per-file publications. Existing symlink/directory artifact destinations are not accepted by batch publication.

Builds clear `dist`; packages contain runtime output, not compiled development tests/scripts or obsolete modules. The smoke campaign installs the tarball into a fresh consumer without development dependencies/install scripts, then invokes its CLI, a measurement and the actual project-test subprocess runner.

## QML

Clones, rules and qmllint are lazy and memoized. Focused source-only measurement does not launch a configured qmllint command or version probe. Unrequested Qt fields are null/`not_requested`, not zero verified findings. Provenance snapshots tool versions; observing Qt later does not invalidate an earlier artifact explicitly not requesting it. Qt-derived evidence still requires the matching observed producer version. Source parsing, type metadata and project resolution remain eager.

Run/source/config/tool/report freshness checks are not a multi-file rollback/crash transaction. Optional-parser fallback is approximate; unresolved JS-module/C++ boundaries are not certified safe.

## Rust

Measurement batches acquire `.rqlens-batch.lock` before staging; competing batches fail before producing evidence. Invalid artifact basenames cannot escape staging. Normal completion/error unwinding releases the lock. A hard crash leaves the marker for explicit operator inspection; never remove a live writer's lock. Existing changed-input and producer-failure protection remains.

Publication still performs sequential per-file replacements. The lock is cooperative between batches, not an immutable reader generation or a global lock for every auxiliary writer. A crash/publication I/O failure requires inspection; automatic rollback is not implemented. Saved full-project `--baseline-review` comparisons are supported, not automatic Git-base checkout analysis.

## Shared unaccepted obligations

Existing subprocess timeout/output-limit tests do not prove universal user-cancellation/process-death cleanup. Distinct permissions for every test/build/macro/package lifecycle command, complete per-rule applicability inventories, broader typed payload validation, source-map/branch attribution, and representative semantic resource campaigns remain required. Synthetic resource ceilings do not impose universal analyzer memory limits. Consult `criteria.json`; none of these limitations silently waive an acceptance criterion.
