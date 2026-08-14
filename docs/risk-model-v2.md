# Architecture Risk Model v2

Model `rqlens.architecture_risk`, version `2`, retains the raw facts and base
category calibrations documented in [version 1](risk-model-v1.md) with the
following intentional scoring changes.

> **Historical model.** New artifacts use [version 4](risk-model-v4.md). Keep
> version 2 baselines only for interpreting version 2 output; baseline
> comparison requires matching model versions.

## Aggregate Score

`quality_risk` already contains `maintainability_risk`, `locality_risk`, and
`leverage_pressure`. Version 2 therefore computes `total_score` as:

```text
quality_risk + change_risk + correctness_risk + architectural_risk
```

Version 1 added `maintainability_risk` a second time. Scores from the two model
versions should not be compared without checking `risk_model_version`.

## Correctness Evidence

Correctness facts now include whole-run and coverage evidence:

- any failed tests penalty: `140.0`
- failed test count: weight `45.0`, cap `120.0`
- unknown test count: weight `4.0`, cap `80.0`
- skipped test count: weight `10.0`, cap `40.0`
- no attributed tests or positive coverage: `90.0`
- test command failure without an attributed failing test: `140.0`
- test compilation failure: `180.0`
- line coverage below `70%`: weight `1.5` per percentage point, cap `105.0`

Coverage pressure is included only when a valid per-module coverage fact is
available. Missing coverage does not become zero coverage.

## Hotspot Terminology

Hotspot calculations remain RQLens-specific pressure proxies. Version 2 names
them `branch_pressure`, `path_pressure`, `maintainability_pressure`,
`effort_proxy`, and `defect_surface_proxy`; they are not presented as standard
cognitive complexity, cyclomatic complexity, maintainability index, Halstead
effort, or predicted defect counts.

The canonical machine-readable weights remain emitted in map metadata from
`src/measurement/scoring.rs`.
