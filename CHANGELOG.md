# Changelog

All notable changes to this project will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and releases follow semantic versioning.

## Unreleased

### Added

- Source and manifest fingerprints on project measurement and evidence artifacts, with stale-input enforcement in policy checks.
- Optional cargo-mutants and repeated-test behavioral gates for mutation strength and flakiness evidence.
- Configured cargo-fuzz target execution and compiler-sanitizer test gates alongside Miri.
- Criterion performance artifacts with baseline percentage regression enforcement.
- Repository outcome artifacts for inferred and reviewed defect, revert, and security labels.
- Vendor-neutral operational telemetry ingestion with freshness and breach policy enforcement.
- Multi-project outcome-ranking validation with precision, recall, percentile rank, and lift metrics.
- Official Rust practice verification with structured command evidence.
- Safety-contract and explicit panic-path findings.
- Automatic local Cargo package source discovery.
- Bundled helper materialization for packaged installations.
- Configurable verification scope and optional `cargo-audit`, `cargo-deny`, cargo-semver-checks, cargo-hack, and Miri gates.
- Selected-dependency MSRV compatibility evidence.
- Owned, reasoned, expiring policy waivers with path scoping and SARIF suppression.
- Stable per-rule finding limits with severity and path/package exclusions.
- Executable changed-line coverage in changed-code review output.
- Versioned cyclomatic and cognitive complexity facts on function hotspots, with module aggregates.
- `tree-sitter-rust` recovery of function spans and raw complexity for files that fail full `syn` parsing, with text-only evidence retained as the final fallback.

### Changed

- Verified gates, correctness runs, and coverage now share workspace, target, feature, exclusion, and optional lockfile scope.
- Coverage rejects malformed exports and nested invocations instead of emitting misleading complete metrics.
- Correctness target discovery now uses Cargo metadata across local workspace packages.
- Artifacts record generation time and RQLens version; the CLI exposes `--version`.
- Configured project roots are canonicalized for stable paths and identities.
- Reused Criterion estimates from `performance --no-run` are marked partial because their execution time is unknown.
- The self-metric gate tracks hotspots and duplication instead of treating total repository LOC as a quality regression.
- Verified gates, static findings, and heuristic architecture signals are represented separately.
- CLI argument definitions and measurement-tool metadata now live in focused modules, keeping the binary entrypoint thin.
- Coverage collection and baseline-regression analysis are split into smaller, testable stages.
- The self-metric maximum function-hotspot baseline is tightened from `100` to `95`.

### Fixed

- `measure all` coverage of the escape-hatch producer is guarded by a regression test.
- Bare `crate`, `self`, and `super` import prefixes no longer create false semantic dependency edges to the crate root or inflate cycle risk.
- Calibration project specifications now trim surrounding path whitespace before canonicalization.
