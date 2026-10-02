# Self-measured refactoring

Baseline: `80d0798`. Evidence: `acceptance/self-refactoring-results.json` and
`target/self-improve/`. This is quality refactoring, **not T4 closure**.

## Method

Built and froze the baseline evaluator, then ran it on the unchanged configured
scope before and after editing: `src`, `rust_helpers/src`, `tests/cli.rs`. Scoring
weights, clone thresholds, source roots and quality budgets were not changed.
The candidate evaluator also produces the same metric records on the final code.

## Results

| Measured quantity | Before | After |
|---|---:|---:|
| Nonblank source lines | 22,492 | 22,462 |
| Token-clone groups | 111 | 88 |
| Duplicated source lines | 1,641 | 1,382 |
| Duplication percentage | 7.30% | 6.15% |
| AST-clone groups | 0 | 0 |
| Sum of function cyclomatic complexity | 3,223 | 3,214 |
| Sum of function cognitive complexity | 1,387 | 1,383 |
| Maximum function hotspot | 69.55 | 69.50 |
| Mean module locality | 97.49 | 97.59 |
| Mean observed-reuse leverage | 22.25 | 22.64 |

Cognitive/cyclomatic maxima remain **11/20**. There is no calibrated effort
metric in these artifacts; reduced repetition and complexity are not a quantified
labor saving or defect prediction. Source and semantic confidence remain partial.

### Targeted functions

| Function | Cognitive before → after | Cyclomatic before → after |
|---|---:|---:|
| Detached Rust project construction | 11 → 5 | 10 → 8 |
| Changed-line coverage | 10 → 5 | 8 → 10 |
| Benchmark confidence interval | 2 → 2 | 14 → 11 |

The coverage caller gains explicit shared-loader error handling, increasing its
cyclomatic count while removing nested traversal and duplicated validation logic.
No complexity increase is hidden from the totals.

### Topology trade-offs

The module count grows from 89 to 91. On the **same 89 modules**, mean locality
improves from 97.49 to 97.53, while mean leverage falls from 22.25 to 22.02:
shared validation removes duplicate direct consumers of validation/fingerprinting.
Increasing dependency fan-in everywhere would not be a sound optimization target.

- Review locality: **85 → 88**.
- Function-risk locality: **97 → 100**.
- Configuration locality: **70.75 → 72.25**.
- Shared fixture helper: locality **100**, eight consumers, leverage **80**.
- Shared current-artifact reader: locality **100**, leverage **20**.
- Trade-offs: policy-test locality **97 → 94**; tool locality **98.5 → 97.75**.

## Changes

- Catalog uses the existing typed tool/name/output registry and central task
  dependency/expense metadata rather than repeating file names and defaults.
- Common summary/array and topology schemas share builders; all 19 published
  artifact schemas and the complete catalog remain identical.
- Review and function-risk share a contract validation/freshness boundary while
  preserving their distinct handling of fresh partial coverage and error messages.
- Temporary source/config setup is shared by eight test modules. Tests remain in
  the original measured scope; no implementation or tests were moved out of it.
- Dependency selection separates eligibility from package deduplication. Coverage
  aggregation filters actual changed executable lines before folding counts.
- Confidence-interval decoding is typed; numeric/sample/bounds requirements remain
  intact. Regression assertions cover invalid intervals and stale/partial coverage.

## Validation and limits

Passed 193 workspace tests, two live macro fixtures, 14 Python tests, strict Clippy,
docs, packaged external-fixture smoke, helper parity and unchanged self-metric gates.
Eight focused measurements of hotspots/clones/locality/leverage produce four exact
repeated comparisons after removing only `generated_at` and `measured_at`.

Two full `measure all` runs were **not** identical: executed-test duration/stdout
and runtime line coverage varied, affecting correctness and the coverage-dependent
map. They are not claimed as deterministic acceptance evidence. The fixed/candidate
scoring comparison separately excludes executable-command provenance because the
paths differ; it does not redefine the timestamp-only repeatability comparison.
