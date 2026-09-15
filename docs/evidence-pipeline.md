# Evidence pipeline

## Input preparation and snapshot

Configuration loading resolves Cargo metadata once, respecting configured
features, `locked`, and semantic offline mode. Full metadata can initialize
`Cargo.lock` before a measurement begins; a no-dependencies fallback supports
incomplete/offline projects. Source-root discovery, target identity, and
benchmark target-directory lookup share this result.

`InputSnapshot` captures toolchain provenance and the quality-input fingerprint
before extraction. `RunContext` owns that snapshot, the source/test inventories,
extracted facts, and a lazily initialized module graph. Producers and their
artifact consumers use this same fingerprint rather than independently
fingerprinting after expensive commands.

Local path dependencies (including sibling repositories) are fingerprinted by
content without being added to the measured source scope. Their nearest workspace
manifest, lockfile and toolchain/Cargo configuration are included for inherited
settings. An unresolved local dependency graph makes the fingerprint partial.

Automatic fingerprint inputs include Rust sources, Cargo manifests and lockfiles,
Cargo configuration, Rust toolchain files, rustfmt/Clippy configuration, and the
license/security/contribution/changelog files inspected by verification.
`fingerprint_inputs` adds explicit project-relative files or directories of any
file type, including assets consumed by `include_str!`, build scripts, or tests.
It does not accept glob patterns. Missing or unreadable inputs make the
fingerprint incomplete. Cargo's configured target directory and the analysis
output are excluded from automatic scanning.

A pre/post check detects changes in input contents, existence, observability,
configuration, Git state, or toolchain evidence. It is not a filesystem lock or
sandbox: run CI against a stable checkout, and do not list generated analysis
output as an input. A change that occurs and is completely reverted between
checkpoints cannot be detected by these checkpoints.

## Staged publication

`ArtifactBatch` seeds a temporary output directory with existing standard
artifacts so focused measurements can still read their dependencies. Each
producer writes into this directory. Dependent producers can consume earlier
results in the same batch, subject to the usual schema/confidence checks.

Before publication, the initial fingerprint is compared with current inputs.
Producer errors and detected input changes leave previous artifacts untouched.
Successful artifacts are then individually atomically replaced. This is not a
multi-file transactional filesystem commit: an I/O failure during publication
can still leave some old and some new files. Every standard artifact records its
input fingerprint, and writers share the output lock.

Review and benchmark commands also validate their snapshot before writing their
final output. Calibration uses the same batch mechanism.

## Typed boundaries

- `MeasurementConfidence` represents either complete or partial evidence. Its
  serde boundary preserves the existing two-boolean JSON contract but rejects
  contradictory or missing states.
- `ArtifactHeader` and `FingerprintHeader` validate common provenance fields.
- `BenchmarkRecord` preserves additional metadata while requiring a nonempty,
  unique identity and a positive finite estimate. Invalid Criterion files remain
  explicit partial-input errors, not silently discarded successes.
- `CoveragePayload` requires a valid numerical percentage for comparisons.
- `InputStatus` and `InputState` prevent policy code from comparing ad-hoc status
  strings or interpreting absent fields as valid empty evidence.

Standard artifact generation validates the contract before staging it. Baseline
readers require compatible, complete evidence, but do not require historical
fingerprints to match today's sources. Unmatched benchmark identities are
incomparable and fail regression policy. Operational evidence additionally
rechecks its observation deadline when policy consumes it.

## Regression tests

The workspace suite covers preserved-timestamp edits, member manifests,
non-Rust inputs, custom Cargo target directories, stale and malformed Criterion
estimates, isolated benchmark execution, telemetry deadline boundaries, quoted
Git filenames, context-free changed-line coverage, malformed baselines, shared
snapshot/cache behavior, and aborted-batch preservation.

Run:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```
