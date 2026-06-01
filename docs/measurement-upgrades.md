# Measurement Upgrades

This note summarizes the lens upgrades that make `rust-quality-lens` more
portable, explicit, and trustworthy across Rust projects.

## Extraction

Rust source extraction now flows through a `syn`-backed helper instead of
tool-local regex parsing for the major Rust-aware measurements. This improves
coverage for dependency graph extraction, type health, correctness discovery,
test status attachment, and escape-hatch detection.

The extractor now handles common Rust patterns that were previously missed or
fragile:

- grouped imports, including multiple items inside one `use` tree
- `self::`, `super::`, direct `crate::x::y()` calls, and module-qualified calls
- `pub use` re-exports and external crate-qualified paths
- same-named types in different modules
- tuple structs, unit structs, enum variants, and enum payload fields
- attribute-driven module files such as `#[path = "..."] mod name;`
- macro-shaped tests and unsupported macro-generated module wiring as explicit
  confidence signals

Unsupported patterns are reported rather than silently treated as absent.

## Measurement Confidence

JSON outputs now include measurement confidence fields so consumers can
distinguish clean measurements from incomplete or stale ones:

- `complete`
- `partial`
- `missing_input`
- `stale_input`
- `unsupported_pattern`

The map no longer substitutes `0` for required missing or stale artifacts.
Affected derived fields are marked unknown, usually as `null`, and listed in
the node's `unknown_metrics`. The map-level summary also reports unknown module
counts and artifact status.

## Risk Calibration

Raw facts and derived risk are separated. The versioned model lives in
`src/rust_quality_lens/tools/risk_model.py`, is documented in
`docs/risk-model-v1.md`, and is emitted into `map.json` metadata.

Architecture map scoring now calls the shared `architecture_risk_scores(...)`
function instead of carrying local weight math. Producer tools that emit scores
also identify the calibration table that produced those scores with:

- `risk_model_id`
- `risk_model_version`
- `risk_calibration`

Shared producer calibrations currently cover:

- hotspots
- escape hatches
- type health
- locality
- leverage

## Correctness Discovery

Correctness discovery is now project-neutral rather than Scratchpad-shaped. It
uses configured source roots plus common Rust target roots such as `tests`,
`benches`, and `examples`, and it honors Cargo manifest targets where present.

Test status attachment now prefers stable test identities and qualified names,
which prevents duplicate test names from receiving the wrong pass/fail status.
Macro-style tests are discovered where possible, and unsupported macro-generated
test patterns are surfaced through measurement confidence.

## Dependency And Locality Signals

Dependency extraction now captures more Rust dependency forms, including
relative module paths, re-exports, direct crate calls, external crates, grouped
imports, and attribute-wired modules.

Test-support detection now uses syntax-resolved test dependencies rather than
file names, stems, or substring matches. This removes easy false positives where
a test file merely mentions a module name without depending on that module.

## Escape Hatches

Escape-hatch detection is `syn`-backed and keeps raw counts separate from
scoring counts. Clippy suppressions no longer double-count as generic lint
suppressions in score calculation, while the raw evidence remains visible.

## Clone Analysis

The catalog promises AST clone analysis, and normal catalog-driven runs now use
`--engine all` so token and AST clone signals are both requested when the helper
toolchain is available. If AST analysis cannot run, clone records carry partial
measurement confidence instead of pretending the requested signal was complete.

## Generic Lens Rules

Correctness and architecture map layer classification now use generic lens
rules in `lens_rules.py`. Scratchpad-specific taxonomy has been removed from
the reusable correctness and map logic.

## Golden Fixtures

The golden Rust fixture now covers the edge cases that drove these upgrades:

- grouped imports
- duplicate test names
- same-named types in different modules
- tuple and unit structs
- `pub use`
- direct `crate::` calls
- macro-style tests
- Clippy suppression de-duplication
- relative module dependencies
- external crate paths
- attribute module wiring

These fixtures make the upgraded behavior executable and harder to regress.
