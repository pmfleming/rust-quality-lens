# Self-review after extended metrics

Baseline: `2e82dff`. This is a focused improvement checkpoint, **not** a claim
that every requested metric or the strict self-gates now passes.

## Comparable evidence

The original evaluator and helper sources were frozen before editing. Its binary
SHA-256 is `0028f5195ee074034499d8c6ec9a9ff60ad49962f54c78285a7eed6eedd33e36`.
Both revisions were measured over the same expanded scope: `src`,
`rust_helpers/src`, `compiler_helpers/src`, `compiler_helpers/build.rs`, and the
five current top-level integration-test files. Synthetic fixtures and mirrored
bundled helper text are excluded. The repository's `rqlens.toml` now declares
that scope explicitly; comparing its old narrower default directly to the new
default would be misleading.

A second comparison uses the corrected evaluator on both the baseline (a detached
Git worktree) and the edited sources. This separates refactoring from repaired
measurement behavior. All 1,250 baseline hotspot rows retain identical numeric
SLOC, complexity, Halstead and hotspot-score fields under both evaluators when
matched by Cargo/module identity, function identity and source position.

Retained aggregates: [acceptance results](../acceptance/self-refactoring-4-results.json).
Local full artifacts, frozen source/binary, configurations and validation logs are
under `target/self-review-4/`; these build outputs are not committed.

## Results

Complexity, effort and clone results agree across the two evaluators. Leverage,
locality and escape counts below use the corrected evaluator on **both** revisions.
Means are unweighted across measured modules, whose count changes from 113 to 114.

| Metric | Before | After |
| --- | ---: | ---: |
| Maximum function cognitive complexity | 33 | 16 |
| Sum of function cognitive complexity | 1,824 | 1,773 |
| Maximum function cyclomatic complexity | 27 | 21 |
| Sum of function cyclomatic complexity | 3,902 | 3,945 |
| Maximum function Halstead effort | 440,764.00 | 284,198.73 |
| Sum of function Halstead effort | 16,613,220.43 | 16,200,950.37 |
| Token clone groups | 80 | 76 |
| AST clone groups | 0 | 0 |
| Duplicated source lines | 1,144 | 1,103 |
| Duplication percentage | 4.15% | 3.97% |
| Escape occurrences | 12 | 3 |
| Mean observed-reuse leverage | 24.60 | 25.00 |
| Mean locality | 96.7412 | 96.7237 |
| Nonblank scoped Rust LOC | 27,590 | 27,749 |
| Functions | 1,137 | 1,157 |

Maximum complexity/effort and duplication improve substantially. Aggregate
cyclomatic complexity and LOC do **not** improve: extraction, correctness checks
and regression coverage add 20 functions and 159 nonblank lines. Locality is
essentially flat but slightly worse overall; the cohesion producer itself improves
from 94 to 97 by keeping Git collection in its maintenance module. Shared parsing
and result capture remain within the command-runner boundary, limiting new fan-out.
No score weights, source exclusions or quality thresholds were loosened.

The remaining escapes are the existing crate-level Clippy suppressions in
`tests/cli.rs`, `tests/trust.rs` and `tests/profiles.rs`. The frozen evaluator
incorrectly reports 9 → 0 because it misses those attributes; corrected counts are
12 → 3. They have not been hidden or reclassified as clean.

## Refactoring and measurement repairs

- Shared typed Cargo artifact decoding replaces separate flakiness/build parsers.
  Shared evidence confidence and explicit failed-result capture remove duplication.
- Split schema construction, token classification, structural collection, history
  accumulation and compiler integration assertions into smaller responsibilities.
  Mutation parsing borrows report objects instead of cloning them.
- Resolve bare nested module paths before root paths, honor Cargo target boundaries,
  and consume each `super::` segment separately. Semantic cache version is now 14.
- Scan attributes once through the AST visitor, including crate, field and method
  attributes previously missed. Remove eight glob imports and the Halstead test
  lint suppression; tests propagate errors instead of unwrapping them.
- Change entropy model 2 uses unique measured paths and sums before rounding.
  On the unchanged baseline, the denominator changes from 118,612 repository-wide
  lines to 53,256 measured-file lines, and entropy from 3.15 to 5.93 bits. This is
  a **measurement correction**, not a code-quality improvement. Missing Git data
  remains null. This advisory formula is versioned separately; calibrated risk
  coefficients and Halstead counting rules are unchanged.
- Incomplete execution cannot yield healthy resource budgets or complete per-test
  flakiness evidence. Incomplete enabled behavioral measurements prevent practices
  conformity. Macro-only structural groups cannot certify complete empty evidence.

## Validation and remaining work

Passed: formatting, bundled-helper parity, locked workspace check and Clippy with
warnings denied, **167 Rust tests** (three environment-dependent tests ignored),
14 Python acceptance unit tests, rustdoc with warnings denied, `rqlens verify`,
`measure all`, `measure correctness-run`, package installation/fixture smoke test,
and whitespace checks. Verification reported 17 passed and 11 skipped checks.

Not passing: `scripts/check-self-metrics.sh` stops at the existing hotspot-score
ceiling. The strict `rqlens check` invocation remains red for partial source/rule
evidence and incomplete test attribution, not observed test failures. These gates
and confidence requirements were retained rather than relaxed.

Neither cargo-machete nor cargo-mutants is installed. Compiler warnings found no
new statically detected dead code; that is not proof of absence of runtime-dead
paths or unused dependencies. No live mutation campaign, cross-daemon campaign,
or new predictive calibration is claimed.

Next priorities are genuine LOC/locality reduction, the remaining resource/history
hotspots, and replacing the three integration-test blanket suppressions with
error-propagating test helpers. Do not remove coverage just to lower the totals.

## Reproduction

Freeze a build and helper sources from `2e82dff`; measure both source revisions
with that evaluator and the identical source-root list above. Then repeat with the
corrected evaluator and helper sources on both revisions, using a detached baseline
worktree so Git evidence retains its original history. Use separate output/cache
directories, offline automatic identity resolution and the same toolchain. Run
`hotspots`, `clones`, `escape-hatches`, `locality`, `leverage` and `module-cohesion`.
Never attribute differences between evaluator versions to the refactor.
