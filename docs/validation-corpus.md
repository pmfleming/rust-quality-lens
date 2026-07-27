# Validation Corpus

RQLens rules must be tested against heterogeneous real projects rather than calibrated only against this repository.

## Local Scratchpad validation

The initial practice, safety, reliability, API-documentation, workspace-discovery, and packaged-helper work was validated against Scratchpad revision `5a06d1afca65ac29ea414275fd792c33903b3425`.

Configuration: `examples/scratchpad.toml`.

Observed on 2026-07-27:

- Cargo discovery extracted 334 Rust files from Scratchpad and its local packages with complete syntax confidence.
- rustfmt, Cargo check, Clippy, tests, and doctests passed under Scratchpad's configured all-feature profile.
- rustdoc correctly failed on a bare URL in `src/app/ui/scrolling/mod.rs`; the command output remained structured evidence rather than making measurement partial.
- Reliability classification reported 63 production panic-path findings and 231 test-scope advisories.
- One safety-contract finding identified an `unsafe impl GlobalAlloc` without a nearby `// SAFETY:` rationale in `src/bin/resource_probe/alloc_metrics.rs`. The unsafe operations inside its methods already had local safety comments.
- API health emitted 92 module rows. These are documentation inventory signals, not correctness failures.

Scratchpad source was not modified by this validation.

## Required expansion

Before treating thresholds as broadly representative, pin and periodically run:

- Tokio for async, cfg, and feature-heavy behavior
- Serde for public APIs and procedural macros
- ripgrep for application and workspace behavior
- rust-analyzer for a large workspace
- Wasmtime for unsafe, fuzzing, performance, and security-sensitive behavior
- an embedded `no_std` project

For every corpus run, retain the revision, toolchain, enabled features, runtime, unsupported patterns, tool availability, and reviewed false positives. Percentiles remain triage aids and must not be described as defect probabilities.
