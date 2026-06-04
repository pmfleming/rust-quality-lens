# Measurement Upgrades

This note summarizes the lens upgrades that make `rust-quality-lens` more
portable, explicit, and trustworthy across Rust projects.

## Extraction

Rust source extraction now flows through a `syn`-backed helper instead of
tool-local regex parsing for the major Rust-aware measurements. This improves
coverage for dependency graph extraction, type health, correctness discovery,
test status attachment, and escape-hatch detection.

`measure all` builds a per-run `RunContext` so source syntax facts are computed
once and passed by reference to producers that need them. Correctness facts are
also cached for correctness and map work. This keeps the major producers from
re-running and re-parsing the same Rust tree independently.

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
- `confidence_scope`
- `required_inputs`
- `observed_inputs`
- `missing_input`
- `stale_input`
- `unsupported_pattern`

Confidence distinguishes source-file coverage from syntax-fact coverage.
Syntax-backed producers such as `hotspots`, `escape-hatches`, `type-health`,
`correctness`, `locality`, `leverage`, and `map` report
`confidence_scope: "syntax_facts"` and require both `rust_source_files` and
`rust_syntax_facts`. Text-oriented producers such as token `clones` report
`confidence_scope: "source_scan"` and require only `rust_source_files`.
Unreadable source files are reported in confidence instead of being treated as
empty text.

The map no longer substitutes `0` for required missing or stale artifacts.
Affected derived fields are marked unknown, usually as `null`, and listed in
the node's `unknown_metrics`. The map-level summary also reports unknown module
counts and artifact status.

## Risk Calibration

Raw facts and derived risk are separated. The versioned model lives in
`src/risk_model.rs`, is documented in
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
uses configured source roots plus top-level Cargo test targets under `tests`,
`benches`, and `examples`, and it honors structured Cargo manifest targets from
`[[bin]]`, `[[test]]`, `[[bench]]`, and `[[example]]`. Nested fixture crates are
not cataloged as tests for the current package unless they are explicitly listed
or configured as source roots.

Test status attachment now prefers stable test identities and qualified names,
which prevents duplicate test names from receiving the wrong pass/fail status.
Macro-style tests are discovered where possible, and unsupported macro-generated
test patterns are surfaced through measurement confidence.

## Dependency And Locality Signals

Dependency extraction now captures more Rust dependency forms, including
relative module paths, re-exports, direct crate calls, external crates, grouped
imports, and attribute-wired modules.

Syntax facts now carry Cargo target context:

- `target_kind`: `lib`, `bin`, `test`, `bench`, `example`, or `module`
- `entrypoint_kind`: runnable entrypoint kind for `bin`, `test`, `bench`, and
  `example` targets
- `is_entrypoint`: whether the file is a tool/runtime entrypoint

The lens infers common target paths such as `src/main.rs`, `src/bin/*.rs`,
`tests/*.rs`, `benches/*.rs`, and `examples/*.rs`, and refines them with
explicit `Cargo.toml` target paths. Map, locality, leverage, clone, and review
outputs preserve this metadata. Entrypoints receive an orchestration allowance
for outbound dependency spread and a small layer-boundary allowance so they stay
visible without being scored like ordinary domain modules.

Attribute evidence is kept separate from dependency extraction. Attribute names
such as `cfg` or `test` do not become dependency edges; only attribute values
that actually wire modules, such as `#[path = "..."]`, influence module wiring.

Test-support detection now uses syntax-resolved test dependencies rather than
file names, stems, or substring matches. This removes easy false positives where
a test file merely mentions a module name without depending on that module.

## Escape Hatches

Escape-hatch detection is `syn`-backed and keeps raw counts separate from
scoring counts. Clippy suppressions no longer double-count as generic lint
suppressions in score calculation, while the raw evidence remains visible.

## Clone Analysis

Clone analysis reports multiple clone and duplication layers:

- `engine: "token"` rows from normalized token windows over source text
- `engine: "ast"` rows from the helper-backed function and method structural
  hasher
- `engine: "module-responsibility"` rows for modules with matching coarse API,
  type, impl, and dependency footprints
- `engine: "test-ast"` rows for repeated non-trivial test body structures

Token rows use source-scan confidence. AST rows use syntax-fact confidence and
are emitted only when multiple non-trivial functions or methods share the same
stable structural hash. The AST hasher records nested functions and impl
methods, normalizes local names and literal values, keeps discriminating
operator/call/type structure, skips trivial functions below the minimum node
threshold, and uses the same stable hash discipline as the token engine.
Module-responsibility rows are intentionally coarse design-duplication signals,
not proof that code should be merged. Test AST rows use Cargo test target
discovery so duplicated integration-test bodies can be surfaced even when
`source_roots` only contains `src`.
Module-responsibility signatures include target and entrypoint kind, which lets
duplicated tool scaffolding group with other entrypoint scaffolding instead of
with ordinary source modules.

## Generic Lens Rules

Correctness and architecture map layer classification now use generic lens
rules in the Rust layer classifier. Scratchpad-specific taxonomy has been removed from
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
