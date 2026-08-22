# Changelog

All notable changes to this project will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and releases follow semantic versioning.

## Unreleased

### Added

- Rust 1.95 toolchain provenance and compatibility evidence, structured compiler diagnostics, and SARIF locations for compiler and Clippy findings.
- Rust 1.95 match-guard complexity, explicit `cfg_select!` confidence reporting, and unscored evidence for newer raw-memory and pointer-provenance APIs.
- Cross-process locks for measurement output, helper materialization, and semantic identity caches.
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
- Cargo-qualified configurable architecture rules with `*` and `**` module matching.
- Test-quality evidence for assertions, system-under-test calls, and ignored tests.
- Function-level CRAP evidence from cyclomatic complexity and fresh executable-line coverage.
- Optional compiler-derived reachable API inventory through cargo-public-api.
- Optional cargo-geiger unsafe inventory and configurable unused-dependency verification.
- Clone summaries with duplicated-line percentage and conservative declaration-only token filtering.
- Module responsibility-focus and Git change-entropy evidence.
- A versioned external evidence ingestion contract for focused tool adapters.
- Calibration distributions for standard complexity and change entropy, plus an explicit deferred risk-model-v5 candidate decision.
- Versioned cyclomatic and cognitive complexity facts on function hotspots, with module aggregates.
- `tree-sitter-rust` recovery of function spans and raw complexity for files that fail full `syn` parsing, with text-only evidence retained as the final fallback.

### Changed

- Raised the workspace MSRV from Rust 1.88 to Rust 1.95, upgraded manifest/configuration parsing to TOML 1.1, and replaced the standard artifact envelope v2 with toolchain-aware v3.
- Bumped the standard raw complexity metric to version 2 to count match guards without changing calibrated hotspot or architecture-risk scoring.
- Split command orchestration and architecture-map scoring behind focused boundaries, tightened the self-metric baseline, and reduced maximum function effort, cyclomatic/cognitive complexity, dependency pressure, duplication, and source size.
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
- Syntax complexity visitors share common traversal logic, and run-context input requirements are centralized in measurement-tool metadata. The token-clone ceiling is rebased from `205` to `244` for the fallback and new evidence producers, a duplicated-line percentage ceiling is added, and the AST-clone ceiling remains zero.

### Fixed

- Trivial six- and seven-node test wrappers no longer qualify as structural AST clones.
- Removed unused artifact contract types and centralized repeated command-success and producer identity scaffolding.
- Automatic source discovery includes custom Cargo target module trees outside `src/` while preserving target-relative module identities.
- Verification timeouts terminate Cargo's descendant process tree so inherited output pipes cannot keep RQLens blocked past the deadline.
- Correctness evidence rejects stale aggregate coverage and marks the resulting measurement partial.
- Unknown configuration fields are rejected instead of silently reverting misspelled options to defaults.
- Reviewed outcome ingestion rejects malformed labels instead of silently dropping records.
- Telemetry windows beyond a bounded future clock-skew tolerance are rejected.
- `measure all` coverage of the escape-hatch producer is guarded by a regression test.
- Bare `crate`, `self`, and `super` import prefixes no longer create false semantic dependency edges to the crate root or inflate cycle risk.
- Calibration project specifications now trim surrounding path whitespace before canonicalization.
