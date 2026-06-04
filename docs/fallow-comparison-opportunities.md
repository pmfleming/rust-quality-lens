# Fallow Comparison Opportunities

This note captures ten improvement opportunities found by reviewing
`rust-quality-lens` against `fallow-rs/fallow` at HEAD
`56b0a0da804d1b6824d54c0a6ffbe1b7e592ce27`.

The intent is not to copy Fallow wholesale. The useful pattern is to adopt the
parts that fit this project's next maturity step: clearer contracts, stronger
conformance testing, CI-ready automation, and smaller internal boundaries.

## Current Signals

The local lens run completed successfully:

- `cargo run --bin rqlens -- catalog`
- `cargo run --bin rqlens -- measure all`
- `cargo test`

The self-measurement found no escape-hatch rows and all discovered tests passed.

The largest local hotspots were originally concentrated in:

- `src/facts.rs`
- `src/producers.rs`
- `src/artifacts.rs`
- `src/main.rs`

The second implementation pass reduced the CLI hotspot by moving shared scoring,
classification, confidence, and provenance helpers into `src/measurement.rs`.
The third pass started the producer split by moving clone detection into
`src/producers/clones.rs`.

Fallow's relevant contrast is a more productized shape: a multi-crate workspace,
strict lint policy, typed output contracts, schemas, CI/action surfaces,
conformance fixtures, fuzzing, benchmarks, npm packaging, and editor/agent
integrations.

## Opportunities

1. Split the architecture.

   `Cargo.toml` defines a single package while `main.rs` mixes CLI, scoring
   helpers, risk constants, and module wiring. Move toward a workspace shape
   such as `rqlens-core`, `rqlens-types`, `rqlens-extract`, and `rqlens-cli`,
   with `rust_helpers` as a workspace member.

2. Shrink producer hotspots.

   `src/producers.rs` owns every producer and is one of the measured risk
   hotspots. Split it by producer: `hotspots.rs`, `clones.rs`,
   `correctness.rs`, `escape_hatches.rs`, `type_health.rs`, `locality.rs`,
   `leverage.rs`, and `map.rs`.

3. Move scoring out of the CLI.

   `src/main.rs` should stay thin: parse commands, load config, invoke the
   engine, and write output. Model metadata, layer classification, confidence
   helpers, type-risk scoring, and calibration constants belong in library
   modules.

4. Add typed output contracts.

   Most public JSON is built directly with `serde_json::json!`. Add `serde`
   structs for each artifact surface and generate JSON schemas for outputs such
   as `map.json`, `hotspots.json`, `correctness_review.json`,
   `rust_escape_hatches.json`, `type_health.json`, and `clones.json`.

5. Add config schema and init flow.

   Config loading currently accepts an optional TOML path and falls back to
   defaults. Add `rqlens init`, config discovery, validation, and
   `rqlens config-schema` so downstream tools can bootstrap and verify a repo
   without relying on README examples.

6. Introduce changed-code review mode.

   The CLI exposes `measure` and `catalog`, but no PR-oriented surface. Add a
   command such as `rqlens review --changed-since main` or
   `rqlens review --diff-file <path>` to scope risk, tests, hotspots, and clone
   findings to changed Rust files.

7. Avoid cargo-run helper overhead.

   Fact extraction now builds helpers once and executes cached helper binaries
   directly. Further gains can come from parallelizing extraction where it is
   safe.

8. Strengthen lints and CI discipline.

   Add workspace lint settings, clippy gates, formatter checks, profile
   settings, and a GitHub workflow or action that runs the catalog, tests,
   helper tests, and measurements. Fallow's strict lints are a good model, but
   RQL should adopt them incrementally to avoid a noisy first pass.

9. Broaden golden and conformance testing.

    The current tests check selected fields and fixtures, but not stable
    whole-artifact contracts. Add normalized golden JSON snapshots and a small
    conformance corpus of real Rust repos to track extractor and scoring
    regressions over time.

## Suggested Sequence

1. Add schemas and typed output structs for the current artifact contracts.
2. Split `src/producers.rs` into per-producer modules while preserving output.
3. Move scoring and classification helpers out of `src/main.rs`.
4. Add a CI workflow that runs `cargo fmt`, `cargo check --all-targets`,
   `cargo test`, helper tests, `rqlens catalog`, and `rqlens measure all`.
5. Add golden artifact snapshots for the existing fixtures.
6. Add `rqlens init`, `rqlens config-schema`, and config discovery.
7. Add changed-code review mode.
8. Convert the helper crate into a workspace member and remove repeated
   `cargo run` overhead.
9. Build a small conformance corpus and track results over time.

## Implementation Status

The first implementation pass covered the bootstrap and automation items:

- Workspace structure and shared lint/profile settings.
- `rust_helpers` added as a workspace member.
- GitHub CI for fmt, check, clippy, tests, catalog, and measurement smoke runs.
- Config discovery, `rqlens init`, and `rqlens config-schema`.
- Initial typed contract scaffolding for review output.
- `rqlens review --changed-since` and `rqlens review --diff-file`.

The second implementation pass covered follow-up hardening:

- Extracted measurement helpers from `src/main.rs` into `src/measurement.rs`.
- Added tests for config schema, init, and changed-file review scope.
- Cleaned Clippy warning noise so workspace Clippy now passes cleanly.

The third implementation pass covered the remaining practical gaps:

- Split clone measurement into `src/producers/clones.rs`, starting the
  per-producer module breakdown at the largest self-contained producer.
- Added cached helper binary execution so extractors build once and run directly
  instead of shelling through repeated `cargo run` calls.
- Added `rqlens artifact-schema` for machine-readable public artifact schemas.
- Added golden-style top-level artifact shape coverage for the mini fixture.

The main deferred item is the heavier multi-crate split (`rqlens-core`,
`rqlens-types`, `rqlens-extract`, `rqlens-cli`). The current workspace is ready
for that move, but forcing it before more producer modules are split would add
churn without much immediate user-visible payoff.
