# Strict-gate implementation checkpoints

Baseline: `c0ef487`. Thresholds and the measured source-root list are unchanged.
Each numbered checkpoint is committed separately. Local logs and the baseline
artifacts are in `target/strict-gates/`.

## 1. Evidence completeness

Replace the fact extractor's nine opaque visitor-macro invocations with ordinary
AST visitor methods. Nested branch/loop/jump handling is preserved and exercised
by the existing complexity regression tests. Source, reliability, architecture
and test-catalog confidence now have no missing syntax facts on this repository.
The timeout regression now executes on Unix as well as Windows, retaining its
platform-specific command rather than leaving an unexecuted Windows-only row in
the Linux self-catalog. `correctness-run` now reports zero unknown test statuses.
Cargo output containing duplicate qualified test names is conservatively marked
unknown instead of overwriting a failure with the last result. Incomplete command
capture cannot certify a passed test run.

Remaining blocker: `--fail-on partial` still rejects type-health and structural
cohesion. Their current source-only backends cannot prove arbitrary macro-generated
ownership or aliased/cfg-dependent receiver relationships. No confidence flag was
weakened to conceal this limitation. Source/rule/test completeness is repaired;
full type/structural completeness is **not** claimed.

## 2. Function hotspot ceiling

Split resource sampling/decoding, Git parsing/window collection, compiler target
selection/projection validation, configuration reading and mutation arguments into
focused helpers. Reuse compiler-command delegation instead of duplicating it.
Every measured function now scores at most **69.66**, below the unchanged 70 gate
(previous maximum 118.05). Workspace tests and Clippy pass. The excluded pinned
nightly compiler adapter cannot be executed here: rustup/rustc-dev is unavailable;
its delegation-only change is source-reviewed and formatted, not claimed live-tested.

## 3. Complexity ceilings

The remaining cyclomatic outliers were exhaustive tool dispatch/name matches,
not deeply nested algorithms. Give the three semantic backends their own small
router and artifact-name helper while preserving exhaustive top-level routing.
The maximum cyclomatic complexity is now **19 ≤ 20** and maximum cognitive
complexity is **11 ≤ 11**. The hotspot ceiling remains satisfied. Workspace tests,
Clippy, existing artifact conformance and golden-output regressions pass.
