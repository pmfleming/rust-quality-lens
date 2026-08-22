# Measurement Design and Upgrades

This document summarizes the current artifact, extraction, confidence, and
scoring design. The [changelog](../CHANGELOG.md) records individual user-visible
changes.

## Artifact contract v3

Standard producer files use envelope version 3, which intentionally breaks the
version 2 contract. Each envelope includes:

- generator and Rust toolchain provenance;
- generation time and a source/manifest fingerprint;
- artifact-level confidence and summary data;
- risk-model identity and calibration metadata;
- `records` for row-oriented output or `data` for structured output.

The toolchain object records active Rust, Cargo, host, LLVM, declared MSRV, and
pinned channel evidence. `verify` compares the declared, pinned, and active
versions and preserves compiler and Clippy diagnostics with source locations.
Policy checks reject stale fingerprints when incomplete evidence is configured
to fail.

Readers remain compatible with legacy unenveloped artifacts. Performance,
outcome, telemetry, validation, calibration, policy, SARIF, and review reports
use separate contracts rather than the standard producer envelope. A checked-in
snapshot guards envelope conformance.

## Rust 1.95 and TOML 1.1

The workspace MSRV is Rust 1.95. Cargo manifests and RQLens configuration use a
TOML 1.1 parser so metadata accepted by Cargo 1.95 can be inspected directly.
Cross-process locks protect helper, semantic-cache, and measurement writes.

Extraction covers inferred const arguments, 128-bit representations,
C-variadic declarations, and `if let` match guards. `cfg_select!` requires macro
expansion, so RQLens reports it as unsupported evidence rather than silently
omitting conditional items.

## Rust extraction

A bundled `syn` helper provides shared facts for Rust-aware measurements.
`measure all` creates one `RunContext`, avoiding repeated parsing and test-fact
collection across producers.

The extractor handles:

- grouped, relative, crate-qualified, and external imports and calls;
- re-exports and same-named types in different modules;
- tuple and unit structs, variants, and payload fields;
- `#[path = "..."]` modules and structured Cargo target paths;
- common macro-shaped tests and explicit unsupported-pattern reporting.

Attribute names do not become dependencies. Only values that wire modules, such
as `#[path]`, affect the graph. Bare `crate`, `self`, and `super` prefixes also
do not create pseudo-dependencies without a qualified segment.

### Parse recovery

When a readable file fails full `syn` parsing, `tree-sitter-rust` recovers text
metrics, Cargo target evidence, function spans, and raw complexity where
possible. Recovered facts record the backend and error-node count.

Recovery remains partial: calibrated hotspot scores are `null`, and map inputs
that require complete syntax remain unknown. If Tree-sitter cannot produce a
tree, RQLens retains text-only evidence.

### Complexity model v2

Function facts include versioned cyclomatic and cognitive complexity.
Cyclomatic complexity starts at one and counts Rust decisions, match arms and
guards, `?`, and short-circuit operators. Cognitive complexity counts nested
control flow, guards, `else` branches, logical sequences, labeled jumps, and
closures.

Module hotspot rows aggregate these raw metrics. They do not change the
calibrated hotspot score.

## Measurement confidence

Confidence lets consumers distinguish a clean empty result from incomplete or
stale extraction. It records:

- `complete` and `partial`;
- `confidence_scope`;
- `required_inputs` and `observed_inputs`;
- `missing_input`, `stale_input`, and `unsupported_pattern`.

Syntax-backed producers require source files and syntax facts. Text-oriented
token clones require readable source files but not complete syntax. Unreadable
files are reported rather than treated as empty.

The architecture map never substitutes zero for a missing or stale required
artifact. Affected values are `null`, nodes list `unknown_metrics`, and the map
summary reports artifact status and unknown-module counts.

## Architecture identity and pressure

Source extraction records positioned dependency candidates. One bounded
rust-analyzer LSP session resolves candidates to Cargo-qualified module and
symbol identities. Each edge records semantic or syntax-fallback provenance.

Resolution modes are:

- `auto`: use semantic results with explicit fallback;
- `required`: reject unresolved candidates;
- `disabled`: use Cargo and syntax identity only.

The semantic cache is keyed by analyzer version, settings, and source
fingerprints. Offline projects can use a generated `rust-project.json` with
local targets, cfgs, features, and dependencies. Inactive, platform-specific,
and macro-generated references remain visible as syntax fallbacks.

Target facts identify libraries, binaries, tests, benches, examples, and module
files. Runnable entrypoints remain visible in map, locality, leverage, clone,
and review output but receive a bounded orchestration allowance. This prevents
tool wiring from being scored like ordinary domain code.

Test-support detection follows syntax-resolved dependencies rather than file
names or substring matches.

## Correctness and coverage

Correctness discovery uses configured roots and Cargo metadata for local
packages, integration tests, benches, examples, and explicit manifest targets.
Nested fixture crates are excluded unless configured or part of the current
workspace.

Test results attach through stable qualified identities, preventing duplicate
test names from receiving the wrong status. Compilation and command failures
remain separate from individual test results. Unsupported generated tests are
reported through confidence.

Coverage comes from `cargo-llvm-cov`. Correctness attribution accepts only fresh
coverage and records which modules have aggregate execution evidence; it does
not claim which test executed a line.

## Clone and escape-hatch analysis

Clone output separates four signals:

- `token`: normalized source windows;
- `ast`: non-trivial repeated function or method structure;
- `module-responsibility`: similar API, type, impl, and dependency footprints;
- `test-ast`: repeated non-trivial test bodies from Cargo test targets.

AST hashing normalizes names and literals while preserving operators, calls,
and type structure. Minimum-size thresholds suppress trivial wrappers.
Module-responsibility matches are coarse design signals, not proof that modules
should be merged.

Escape-hatch extraction is syntax-backed and keeps raw counts separate from
scores. Clippy suppressions are not scored again as generic lint suppressions.
New raw-memory and pointer-provenance APIs remain visible as unscored evidence
until calibration supports a weight change.

## Behavioral and contextual evidence

Optional verification can run cargo-mutants, repeated tests, fuzz targets,
sanitizers, and Miri. Each gate records its command and distinguishes disabled,
unavailable, failed, and timed-out states. These gates add reachability evidence;
they do not prove soundness or defect freedom.

Separate commands preserve evidence that does not belong in static quality
scores:

- `performance` captures Criterion estimates and benchmark deltas;
- `outcomes` separates reviewed labels from commit-message inference;
- `telemetry` records normalized production signals and freshness;
- `validate` measures ranking association against outcome labels.

Changed-code review records diff hunks and executable changed-line coverage when
fresh coverage exists. Missing or stale coverage remains explicit.

## Risk calibration

Raw facts and derived risk stay separate. The current model lives in
`src/risk_model.rs`, is documented in [Risk Model v4](risk-model-v4.md), and is
identified in `map.json`.

Scored tools identify their model and calibration with `risk_model_id`,
`risk_model_version`, and `risk_calibration`. Shared calibrations cover
hotspots, clone engines, escape hatches, type health, locality, and leverage.

Version 4 uses compiler-assisted dependency identity and retains positioned,
per-edge fallback provenance. The proposed version 5 remains deferred until
held-out validation supports incorporating newer correctness, architecture,
test-quality, clone, and entropy evidence.

## Portability and fixtures

Correctness and map classification use generic Rust rules rather than
project-specific taxonomy. Golden fixtures cover parsing, identity, confidence,
test discovery, fallback behavior, and Rust 1.95 syntax. The fixture and
envelope snapshots make these contracts executable and harder to regress.
