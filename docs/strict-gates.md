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

## 4. Zero escape occurrences

Remove all three integration-test blanket lint suppressions. Test setup, command
execution, artifact decoding and lookups now propagate errors through `anyhow::Result`
instead of panicking through unwrap/expect. Fallible iterator collection validates
JSON shapes before predicates run. Split the resulting long integration scenarios
into focused assertion helpers so steps 2–3 stay green. Fixture-program literals
are unchanged. Comparing the step-3 and step-4 test logs shows the identical 172
test identities/statuses: **169 passed, 3 ignored**. Workspace Clippy passes without
suppressions, the escape artifact has **zero records**, and the hotspot/cognitive/
cyclomatic ceilings still pass. This checkpoint adds some explicit error-handling
LOC; the next checkpoint addresses reduction without deleting test coverage.

## 5. LOC reduction checkpoint — budget still exceeded

Factor repeated clone-fixture source through parameterized templates and share
fallible JSON-array access. The generated fixture's five Rust files, manifest and
configuration are byte-for-byte identical to the pre-refactor fixture; no test
identity/status changed. This removes duplicated scaffolding without moving code
outside the measured roots or deleting cases.

Complete source evidence also exposed eleven pre-existing compiler-helper `expect`
findings. Propagate probe/capture/publication errors instead; capture failure stops
the driver and cannot certify an inventory. Compiler path validation now fails
closed if canonicalization fails. The build script compiles on the available
compiler and a missing-RUSTC probe exits unsuccessfully without panicking. Full
rustc-private adapter execution still requires the unavailable pinned toolchain.

Step-5 scoped LOC falls **28,007 → 27,925** (82 lines). The 22,500 ceiling is **not
met**: another 5,425 lines would need genuine removal. Across the entire campaign,
LOC rises from 27,749 to 27,925 due to explicit error handling and decomposition.
No threshold was raised and no source roots, features or tests were removed. A
larger architectural reduction is still required; this is not a completed size gate.

## Final validation

- Maximum hotspot **69.66**, cognitive **11**, cyclomatic **19**: pass.
- Escape records **0**, AST clones **0**, token clone groups **81**, duplication
  **3.87%**, map locality **97** and observed-reuse formula: pass their self-gates.
- **169 Rust tests pass**, three environment-dependent tests remain ignored;
  **14 Python acceptance unit tests pass**. Test inventory/status comparison and
  byte-for-byte generated-fixture comparison pass.
- Workspace check, Clippy with warnings denied, formatting (including the separate
  compiler-helper crate), bundled-helper parity, rustdoc with warnings denied,
  `measure all`, `verify`, packaging smoke and whitespace checks pass.
- Configured rules have **zero violations**, all test statuses are known, and
  practices report 17 passed / 11 skipped checks. Strict policy now fails only for
  partial `type_health.json` and `module_cohesion.json` evidence.
- `check-self-metrics.sh` now reaches and fails only the LOC ceiling; all its other
  conditions were also evaluated independently and pass.

[Retained results](../acceptance/strict-gates-results.json). Steps 2–4 are closed;
step 1 still needs stronger type/cohesion backends and step 5 remains over budget.
