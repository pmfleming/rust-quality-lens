# Architecture Risk Model v4

Model `rqlens.architecture_risk`, version `4`, is the current model. It retains
the version 3 category formulas but changes architecture topology from
syntax-only dependency matching to compiler-assisted definition resolution.
Because this can discover more accurate edges, v3 and v4 totals are not directly
comparable. The canonical formulas and emitted metadata live in
`src/risk_model.rs` and `src/measurement/scoring.rs`; `map.json` records the
model ID, version, weights, tool calibrations, and classification bands used for
each run.

## Semantic dependency identity

Syntax extraction records each candidate dependency path with its source line
and UTF-16-compatible column. A single bounded rust-analyzer LSP session asks
for the definition of each internal candidate. Successful local definitions
produce:

- a Cargo-qualified target module ID;
- a stable `package::target::module::symbol` identity;
- `rust_analyzer` edge provenance.

References that are inactive for the loaded cfg, macro-generated, unresolved,
or unavailable within the timeout retain the version 3 Cargo/syntax edge with
`syntax_fallback` provenance. `auto` mode therefore remains usable under partial
resolution; `required` mode rejects any unresolved candidate.

Results are cached in `semantic_identity_cache.json` by source metadata,
resolver settings, cache format, and rust-analyzer version. When full offline
Cargo metadata is unavailable, RQLens generates a local `rust-project.json`
containing target roots, declared feature cfgs, platform cfgs, and workspace
dependencies. Set `[rust].identity_offline = false` to allow normal online Cargo
metadata behavior. `disabled` mode intentionally uses only Cargo/syntax
identity and does not claim semantic resolution.

## Empirical validation

The 2026-07-13 calibration used pinned Scratchpad, nm-daemon, ripgrep, and Tokio
checkouts. It retained 840 modules and observed 22,901 semantic candidates:

| Project | Resolved | Candidates | Resolution |
| --- | ---: | ---: | ---: |
| Scratchpad | 8,449 | 8,461 | 99.86% |
| nm-daemon | 1,514 | 1,520 | 99.61% |
| ripgrep | 3,493 | 3,656 | 95.54% |
| Tokio | 6,380 | 9,264 | 68.87% |
| **Pooled** | **19,836** | **22,901** | **86.62%** |

Tokio's remaining candidates are predominantly cfg/platform or macro-mediated
references outside the generated offline crate graph. They are preserved as
fallbacks rather than discarded.

Topology-sensitive total-risk bands were recalibrated after semantic edges were
enabled:

| Metric | Watch (p90) | High (p95) | Extreme (p99) |
| --- | ---: | ---: | ---: |
| Function hotspot | 19.15 | 32.98 | 80.08 |
| Module hotspot | 67.30 | 95.51 | 139.94 |
| Total architecture risk | 712.99 | 838.64 | 1,028.20 |

These remain triage percentiles from a small heterogeneous sample, not defect
probabilities. The archived machine-readable report was generated at
`target/calibration-semantic/calibration_report.json`. Reproduce a report with
pinned local checkouts and an explicit destination:

```bash
rqlens calibrate \
  --project scratchpad=/path/to/scratchpad \
  --project ripgrep=/path/to/ripgrep \
  --output-dir target/calibration-semantic
```

Record each checkout revision and toolchain with the report. A baseline used by
`rqlens check` must have the same risk-model version as the current map.
