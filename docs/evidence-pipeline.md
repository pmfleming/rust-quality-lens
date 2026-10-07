# Evidence pipeline

## Input preparation and snapshot

Configuration loading resolves the project's Cargo metadata once, respecting configured
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

V5 fingerprints also cover the configured helper sources, manifests, lockfiles,
and local workspace/dependency inputs. Helper metadata is resolved before the
snapshot, including initial lockfile preparation; missing helper metadata cannot
produce complete evidence. A separate `helper_digest` lets campaign comparisons
reject helper changes independently of intentional measured-source changes.
Helper executables use locked builds in per-manifest cache targets, not ordinary
project Cargo output. This prevents same-named executables from another checkout
from being selected merely because a shared output path exists. Campaign tool
builds likewise use a dedicated target, and captures freeze the main executable.

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

## Generated semantic identities

`rust.identity_build_macros` defaults to `false`. A trusted-project opt-in enables
both rust-analyzer build scripts and proc macros; it is not safe for arbitrary
untrusted source. Semantic cache version 15 includes this option and the
unindexed-definition classification described below, and normal
content fingerprints cover declared `fingerprint_inputs` such as protocol XML.
Feature selection is supplied once (`cargo.features = "all"`), not duplicated in
Cargo extra arguments where it would prevent build-script loading.

Definition responses retain all locations. A measured local origin takes priority
over an external generator; the referenced symbol is associated with its local
module rather than the generator's token. A file absent from the measured path
index is **not automatically external**. Definitions under the project or local
Cargo package roots remain `unindexed_project_definition`; unmapped definitions
under Cargo's target directory (including a relocated target directory) remain
`unmapped_generated_definition`. Build output can belong to a project or a
dependency: its directory name is not ownership evidence. Resolved navigation
paths are retained for diagnosis, but no local module edge is invented. An
indexed origin still takes precedence. Other external file definitions remain
external, while unsupported virtual URIs are explicit unresolved evidence. Glob
imports navigate at their module identifier, independently of the star location
retained for escape-hatch evidence.

Source confidence declares `inventory_scope =
"authored_syntax_before_cfg_and_macro_expansion"`. The nested `semantic_identity`
declares `resolution_scope =
"collected_dependency_references_only_not_inventory_completeness"` and reports
`unresolved_by_status` counts. These separate questions must not be conflated:
100% resolution can coexist with a partial source inventory, and successful
navigation without an inventoried owner does not count as resolved identity.
Unknown ownership propagates to the outer confidence and strict policy gates;
`identity_resolution = "required"` rejects it. Cold/warm cache results retain the
same scope and reason counts. Navigation alone does not expand macro bodies or
certify compiler-complete architecture. Separate lexical scope frames resolve
unambiguous block-local type owners without promoting imports, aliases or generic
shadowing into guessed joins.

The separately opted-in [compiler architecture backend](compiler-architecture.md)
can certify static module rules for completely captured selected contexts. Its
confidence does not promote source metrics or type counts, and compiler failure
never silently substitutes a syntax graph.

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
# Requires rust-analyzer, sysroot sources, and its proc-macro server:
cargo test --locked semantic::macro_tests -- --ignored
```

The live local build-script/proc-macro fixture tests trust opt-in, generated and
glob imports, and cache reuse without registry dependencies. Unit tests cover
location ordering, external/virtual definitions, XML edits with preserved mtimes,
and default-safe analyzer configuration. Linux CI also runs the live fixture.
