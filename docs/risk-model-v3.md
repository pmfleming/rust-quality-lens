# Architecture Risk Model v3

Model `rqlens.architecture_risk`, version `3`, keeps the version 2 aggregate and
correctness formulas while changing hotspot granularity, graph identity, and
classification bands. Artifact schema remains version 2.

## Function and module hotspots

Every parsed function and impl method now has its own source span, branch/path
pressure, maximum nesting depth, score, risk band, and additive
`score_components`. Module hotspot scores aggregate the maximum, p95, and mean
function scores plus bounded module-size pressure. Large easy modules therefore
no longer look equivalent to modules containing one deeply complex function.

The weights and bands are emitted in `map.json` under
`meta.risk_model_tool_scores.hotspots`.

## Collision-safe identities

Cargo metadata supplies package and target identity. Graph nodes and edges use
`package::target::module` IDs, while `module_key` remains the readable
within-package Rust path. A structured Cargo manifest fallback preserves target
identity when `cargo metadata` is unavailable, and a path fallback is explicitly
reported as partial confidence.

Dependency names are still extracted from Rust syntax and resolved within the
Cargo package; version 3 does not claim rustc `DefId`-level symbol resolution.

## Coverage and score provenance

Correctness output records syntax-attributed modules, Cargo-qualified module
IDs, and modules also observed by aggregate `cargo-llvm-cov` line coverage.
Aggregate coverage corroborates that code was executed but does not claim which
individual test executed it.

Scored producers and map categories emit raw signal contributions. The `check`
command emits every baseline delta with `new`, `regressed`, `improved`, or
`unchanged` direction; model versions must match before comparison.

## Empirical calibration

On 2026-07-13 the calibration runner measured these pinned revisions:

| Project | Revision | Rust files | Functions | Modules |
| --- | --- | ---: | ---: | ---: |
| Scratchpad | `ea9a9b9cda7b` | 331 | 3,895 | 331 |
| nm-daemon | `4c29f6fef1c` | 51 | 793 | 51 |
| ripgrep | `d5b85d44057f` | 79 | 2,637 | 79 |
| Tokio | `33f46a5395cc` | 377 | 3,699 | 377 |

All 838 modules retained Cargo-qualified identities. Across 11,024 functions,
the pooled empirical thresholds were:

| Metric | Watch (p90) | High (p95) | Extreme (p99) |
| --- | ---: | ---: | ---: |
| Function hotspot | 19.15 | 32.54 | 80.14 |
| Module hotspot | 67.18 | 95.51 | 139.94 |
| Total architecture risk | 599.03 | 710.60 | 862.97 |

These are ranking and triage bands from a deliberately heterogeneous but small
sample. They are not defect probabilities or universal ecosystem norms. Static
test discovery also leaves unexecuted tests unknown, so release gates should add
test-run and coverage artifacts. Reproduce or extend the sample with:

```text
rqlens calibrate --project NAME=/checkout/path [--project NAME=/another/path ...]
```

The complete machine-readable report is written to
`target/calibration/calibration_report.json`.
