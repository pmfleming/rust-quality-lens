# Changelog

All notable changes to this project will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and releases follow semantic versioning.

## Unreleased

### Added

- Source and manifest fingerprints on every artifact, with stale-input enforcement in policy checks.
- Optional cargo-mutants and repeated-test behavioral gates for mutation strength and flakiness evidence.
- Configured cargo-fuzz target execution and compiler-sanitizer test gates alongside Miri.
- Criterion performance artifacts with baseline percentage regression enforcement.
- Repository outcome artifacts for inferred and reviewed defect, revert, and security labels.
- Official Rust practice verification with structured command evidence.
- Safety-contract and explicit panic-path findings.
- Automatic local Cargo package source discovery.
- Bundled helper materialization for packaged installations.
- Configurable verification scope and optional `cargo-audit`, `cargo-deny`, cargo-semver-checks, cargo-hack, and Miri gates.
- Selected-dependency MSRV compatibility evidence.
- Owned, reasoned, expiring policy waivers with path scoping and SARIF suppression.
- Stable per-rule finding limits with severity and path/package exclusions.

### Changed

- Verified gates, correctness runs, and coverage now share workspace, target, feature, exclusion, and optional lockfile scope.
- Coverage rejects malformed exports and nested invocations instead of emitting misleading complete metrics.
- Correctness target discovery now uses Cargo metadata across local workspace packages.
- Artifacts record generation time and RQLens version; the CLI exposes `--version`.
- The self-metric gate tracks hotspots and duplication instead of treating total repository LOC as a quality regression.
- Verified gates, static findings, and heuristic architecture signals are represented separately.
