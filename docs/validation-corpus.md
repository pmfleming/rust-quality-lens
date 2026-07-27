# Validation Corpus

RQLens rules must be tested against heterogeneous real projects rather than calibrated only against this repository.

## Local Scratchpad validation

The initial practice, safety, reliability, API-documentation, workspace-discovery, and packaged-helper work was validated against Scratchpad revision `5a06d1afca65ac29ea414275fd792c33903b3425`.

Configuration: `examples/scratchpad.toml`.

Observed on 2026-07-27:

- Cargo discovery extracted 334 Rust files from Scratchpad and its local packages with complete syntax confidence.
- rustfmt, Cargo check, Clippy, tests, doctests, and warning-free rustdoc passed under Scratchpad's configured all-feature profile.
- The first rustdoc run found a bare URL in `src/app/ui/scrolling/mod.rs`; the structured evidence drove a documentation fix.
- RustSec initially found vulnerable `crossbeam-epoch` and `quick-xml` versions. Compatible transitive updates removed those advisories; two advisories for an unrelated crates.io package also named `scratchpad` are documented in Scratchpad's `.cargo/audit.toml`.
- Reliability classification now reports 57 production panic-path findings and 234 test-scope advisories.
- A safety-contract finding on `unsafe impl GlobalAlloc` drove a nearby invariant comment; no production safety-contract errors remain.
- API health emitted 92 module rows. These are documentation inventory signals, not correctness failures.
- Cargo metadata established Rust 1.95 as the selected dependency graph's effective minimum, matching Scratchpad's pinned toolchain and declared `rust-version`.
- cargo-hack verified all four combinations of Scratchpad's `inspection` and `count-lines` features.
- The unresolved repository-license decision is represented by an owned waiver expiring on 2026-10-31 rather than silently passing.

The resulting Scratchpad changes were validated with 479 passing tests, Clippy, rustdoc, and RustSec.

## Required expansion

Before treating thresholds as broadly representative, pin and periodically run:

- Tokio for async, cfg, and feature-heavy behavior
- Serde for public APIs and procedural macros
- ripgrep for application and workspace behavior
- rust-analyzer for a large workspace
- Wasmtime for unsafe, fuzzing, performance, and security-sensitive behavior
- an embedded `no_std` project

For every corpus run, retain the revision, toolchain, enabled features, runtime, unsupported patterns, tool availability, and reviewed false positives. Percentiles remain triage aids and must not be described as defect probabilities.
