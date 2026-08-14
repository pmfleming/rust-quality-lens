# Architecture Risk Model v1

This document describes the versioned scoring model used by `map.architecture`
and the shared calibration tables used by scored producer tools.
The model id is `rqlens.architecture_risk`, version `1`.

> **Historical model.** New artifacts use [version 4](risk-model-v4.md). This
> document preserves version 1 semantics for old-artifact interpretation; do
> not apply its classification thresholds to current output or compare scores
> across model versions.

## Raw Facts

Raw facts are measurements observed directly from source code, generated
artifacts, or repository history. They should be preserved independently from
risk scores so future model versions can rescore the same project without
changing what was measured.

Raw fact inputs include:

- dependency graph edges and module public API counts from the Rust syntax extractor
- complexity and size facts from `hotspots.json`
- test discovery and status facts from `correctness_review.json`
- locality facts from `locality_metrics.json`
- leverage facts from `leverage_metrics.json`
- churn, commits, contributors, defect-keyword commits, and co-change facts from git history

When a required input artifact is missing or stale, the affected derived scores
are `null` rather than `0`, and the current map contract records the missing or
stale artifact in `measurement_confidence` and `summary.artifact_status`.

## Derived Scores

Derived risk scores are model outputs. They are not raw facts:

- `maintainability_risk`
- `change_risk`
- `correctness_risk`
- `architectural_risk`
- `quality_risk`
- `total_score`

`total_score` is `null` if any required category score is unknown.

## Weights

For current code, the canonical machine-readable category formulas live in
`src/risk_model.rs`, while weights, calibrations, and classification metadata
are emitted from `src/measurement/scoring.rs`. Architecture category weights
are emitted into map metadata as `meta.risk_model_weights`; producer calibration tables are emitted as
`meta.risk_model_tool_scores`.

### Maintainability

- complexity score: weight `1.0`
- SLOC: weight `0.12`, cap `70.0`
- public API count: weight `2.5`, cap `30.0`
- outbound dependencies: weight `4.0`
- inbound dependencies: weight `1.0`
- combined dependency contribution cap: `35.0`
- entrypoint modules receive an outbound orchestration allowance before
  dependency pressure is scored

### Change

- churn: divided by `12.0`, cap `160.0`
- commit count: weight `2.5`, cap `100.0`
- contributor count: weight `14.0`, cap `80.0`
- defect-keyword commits: weight `18.0`, cap `90.0`
- missing test evidence penalty: `90.0`

### Correctness

- any failed tests penalty: `140.0`
- failed test count: weight `45.0`, cap `120.0`
- unknown test count: weight `4.0`, cap `80.0`
- skipped test count: weight `10.0`, cap `40.0`
- missing test evidence penalty: `90.0`

### Architectural

- outbound dependencies: weight `10.0`, cap `120.0`
- inbound dependencies: weight `8.0`, cap `120.0`
- layer violations: weight `32.0`, cap `120.0`
- cycle membership penalty: `110.0`
- large module threshold: `250` SLOC
- large module penalty: `60.0`
- entrypoint modules receive a small layer-boundary allowance so tool wiring
  remains visible without being penalized like ordinary modules

## Classification

- module total score `>= 300.0`: warning
- module total score `>= 600.0`: bad
- category color warning threshold: `350.0`
- category color bad threshold: `700.0`

## Tool Calibration

Scored producer tools attach `risk_model_id`, `risk_model_version`, and
`risk_calibration` to their records. These fields identify which table from
`RISK_MODEL["tool_scores"]` produced the derived score while leaving raw facts
in the tool-specific count and measurement fields.

Currently shared producer calibrations cover:

- `hotspots`: complexity component caps, weights, multiplier, and signal thresholds
- `escape_hatches`: escape-hatch weights and Clippy/lint suppression de-duplication
- `type_health`: structural pressure caps, free allowances, and signal thresholds
- `locality`: non-locality risk weights for coupling, hidden state, tests, and churn
- `leverage`: leverage score base, bonuses, penalties, pressure scaling, and risk inversion
- `clones_token`: token-window size, minimum line span, and instance weight
- `clones_ast`: minimum AST node threshold and cross-file factor
- `clones_module_responsibility`: coarse module API/type/dependency duplication
- `clones_test_ast`: repeated non-trivial test body structures
- target metadata: `target_kind`, `entrypoint_kind`, and `is_entrypoint`
  derived from common Rust paths and explicit Cargo target paths

## Implementation Boundaries

The current Rust implementation is split by responsibility:

- `src/config.rs`: config loading and absolute path resolution
- `src/catalog.rs`: task catalog contract consumed by dashboards and runners
- `src/facts.rs` and `src/facts/`: run-context caching, module graphs, Cargo target discovery, and test execution
- `src/producers.rs` and `src/producers/`: producer dispatch and individual measurements
- `src/artifacts.rs` and `src/artifacts/`: map artifact loading, indexing, topology, history, cycles, and layer violations
- `src/risk_model.rs`: shared `architecture_risk_scores(...)` function and score structs
- `src/measurement/scoring.rs`: emitted model weights, calibrations, and current classification bands
- `src/util.rs`: small filesystem, path, hashing, and JSON helpers

The map producer calls `architecture_risk_scores(...)` for every module. If an
input category is unavailable, the affected category and `total_score` remain
`null`, and `unknown_metrics` records why.
