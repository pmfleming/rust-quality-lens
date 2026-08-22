# Architecture Risk Model v1

This document describes the original scoring model used by
`map.architecture`. Its ID is `rqlens.architecture_risk`, version `1`.

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

## Derived Scores

Derived risk scores are model outputs. They are not raw facts:

- `maintainability_risk`
- `change_risk`
- `correctness_risk`
- `architectural_risk`
- `quality_risk`
- `total_score`

Version 1 calculated:

```text
quality_risk = min(maintainability_risk + locality_risk + leverage_pressure, 600)
total_score = maintainability_risk + quality_risk + change_risk
              + correctness_risk + architectural_risk
```

This counted maintainability twice. Version 2 corrected the aggregate formula.

## Weights

The following values reconstruct version 1 artifacts. Current formulas live in
`src/risk_model.rs`, and current runs emit their weights and classification
metadata in `map.json`.

### Maintainability

- complexity score: weight `1.0`
- SLOC: weight `0.12`, cap `70.0`
- public API count: weight `2.5`, cap `30.0`
- outbound dependencies: weight `4.0`
- inbound dependencies: weight `1.0`
- combined dependency contribution cap: `35.0`

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

## Classification

- module total score `>= 300.0`: warning
- module total score `>= 600.0`: bad
- category color warning threshold: `350.0`
- category color bad threshold: `700.0`

## Current behavior

This file preserves version 1 scoring semantics only. Current artifacts use
[version 4](risk-model-v4.md), include explicit calibration metadata, preserve
missing inputs as unknown values, and apply bounded entrypoint allowances. See
[Measurement Design and Upgrades](measurement-upgrades.md) for the current
implementation contract.
