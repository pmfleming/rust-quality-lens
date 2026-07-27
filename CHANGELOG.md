# Changelog

All notable changes to this project will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and releases follow semantic versioning.

## Unreleased

### Added

- Official Rust practice verification with structured command evidence.
- Safety-contract and explicit panic-path findings.
- Automatic local Cargo package source discovery.
- Bundled helper materialization for packaged installations.
- Configurable verification scope and optional `cargo-audit`, `cargo-deny`, cargo-semver-checks, cargo-hack, and Miri gates.
- Selected-dependency MSRV compatibility evidence.
- Owned, reasoned, expiring policy waivers with path scoping and SARIF suppression.

### Changed

- Verified gates, static findings, and heuristic architecture signals are represented separately.
