# Self-review after Halstead and compiler inventory

Baseline: `24c4f0e`. This is a bounded RQLens refactor and cross-daemon
compatibility check, **not** completion of every quality objective or strict gate.

## Measurement protocol

The baseline executable was frozen before editing. Its SHA-256 is
`552adbc0847ff59eeb300e4799f5eb9d141cab0338a801c3ce8ab5c7f234b0fe`.
Both source revisions were measured with that executable, the same configuration,
and unchanged measurement/scoring rules. Helper measurement logic was unchanged;
the helper edit only replaces a test glob import with explicit imports.

The scope includes `src`, `rust_helpers/src`, `compiler_helpers/src`,
`compiler_helpers/build.rs`, and all four CLI/compiler/source-expansion/trust
integration-test files. No implementation or tests were moved outside that scope.
This broader scope is not interchangeable with the default self-gate scope.

Raw local evidence is under `target/self-refactor-2026-10-05/`:
`baseline/`, `after/`, `comparison.json`, `rqlens.toml`, the frozen executable,
and `daemon-parity/`. Compact results are retained in
[`acceptance/self-refactoring-3-results.json`](../acceptance/self-refactoring-3-results.json).

## Frozen-evaluator results

| Observation | Before | After |
|---|---:|---:|
| Nonblank source lines | 25,129 | 25,116 |
| Function count | 1,046 | 1,055 |
| Maximum cognitive complexity | 28 | 17 |
| Cognitive complexity sum | 1,555 | 1,548 |
| Maximum cyclomatic complexity, all functions | 27 | 27 |
| Cyclomatic complexity sum, all functions | 3,538 | 3,544 |
| Maximum cyclomatic complexity, excluding test-named code | 27 | 21 |
| Maximum function Halstead effort | 449,835.89 | 349,245.15 |
| Token-clone groups | 80 | 68 |
| Duplicated lines | 1,171 | 1,047 |
| Duplication | 4.66% | 4.17% |
| AST-clone groups | 0 | 0 |
| Detected escape occurrences | 2 | 1 |
| Mean locality, same 103 modules | 97.6772 | 97.6917 |
| Mean leverage, same 103 modules | 21.2621 | 20.9709 |

The test-name filter excludes paths containing `/tests/` and qualified names
containing `::tests::`; it is **not** compiler-certified production selection.
The remaining all-function cyclomatic maximum belongs to the live compiler
integration test. Halstead effort is the genuine versioned syntax-token metric,
not engineering hours, and function efforts are not added into a whole-program
Halstead estimate. The new schema-helper module is included in source totals;
its locality/leverage scores are 100/10 but it is excluded from the fixed-cohort
means above.

Evidence remains partial, including unexpanded syntax and architectural identity
limitations. These observations do not establish complete compiled-program
coverage, dead-code absence, or improved architecture everywhere.

## Refactors and trade-offs

- Decode compiler Cargo metadata into required typed fields once. Separate driver
  validation, Cargo execution, target selection, and capture validation. Index
  selected package ownership/features rather than scanning every dependency for
  every capture. Exact coverage, feature checks, build-failure abstention, and
  duplicate-context rejection remain in force.
- Across **all** functions in `compiler_inventory/capture.rs`, cognitive complexity
  falls **34→23** and cyclomatic complexity **62→58**, including extracted helpers.
  `expected_units` (28/26 cognitive/cyclomatic) is replaced by `Selection::new`
  (9/12) and explicit target-mode selection (7/6); complexity was not simply
  hidden outside the measured scope.
- Share JSON Schema property constraints in `src/contracts/schema.rs`, independent
  of artifact-specific schema construction. Share function-row identity and metric
  metadata in hotspots. All public schemas, the config schema, and the catalog
  remain structurally identical to the frozen executable.
- Share percentile calculation while preserving caller-specific rounding; cover
  empty/singleton inputs, boundary quantiles, and rounding. Reuse CLI fixture
  writing without removing fixture assertions.
- Share project-relative path formatting in `src/util.rs`, borrowing absolute
  paths instead of allocating a temporary `PathBuf`. Remove review's duplicate
  formatter and localize the helper-executable cache to `src/facts/helpers.rs`.
- Share identical source/correctness fact inventories with `Arc<[FileFacts]>`.
  Borrow test-quality facts and deduplication keys rather than deep-cloning them.
  A regression checks allocation sharing and preserves additional test-only files.
  No measured runtime or memory-percentage improvement is claimed.
- Utilities have **40→42** observed consumers. Clone/cohesion producer locality
  improves **91→94 / 97→100**, but correctness locality drops **94→91**, utility
  locality **79→77.5**, and path-classification leverage **100→70** as generic path
  formatting moves out. Average leverage therefore does **not** improve; adding
  gratuitous callers to raise the score would undo the separation.
- Remove the helper test glob import, including its bundled copy. The existing
  Clippy suppression remains visible. Strict Clippy passes, but that is not an
  exhaustive dead-code/unused-dependency proof. Redundant implementations and a
  cache-key wrapper were removed; low graph reach was not treated as proof that
  application code is unreachable.

The added regression and helper boundaries mean aggregate cyclomatic complexity
rises despite simpler capture logic. The 13-line source reduction is modest.
Neither fact is hidden by the better maxima or clone counts.

## Cross-daemon validation

For app, bar, bt, clip, and nm daemons, compare frozen/candidate outputs for
hotspots, clones, escape-hatches, locality, leverage, test-quality, api-health,
and reliability: **40 matching artifact pairs**. Only `generated_at` and
`measured_at` are removed recursively; array order, scores, findings, source
fingerprints, and confidence remain compared. Each binary is staged at the same
invocation path so recorded commands match without masking command differences.
Binary hashes, daemon revisions/worktree status, configs, logs, and raw artifacts
are retained in `daemon-parity/`.

This validates the shared measurement implementation across daemons, not daemon
application refactoring or complete daemon quality evidence. No daemon source,
configuration, policy, or existing worktree changes were modified by this pass.

## Validation and remaining gates

- Formatting, locked workspace/all-target check, strict Clippy, bundled-helper
  consistency, warning-free rustdoc, and packaged external-fixture smoke test pass.
- Workspace tests: **148 passed, 0 failed, 3 ignored**. The compiler-inventory suite
  also passes all **3** tests with its live pinned adapter enabled.
- `verify` is conformant and `measure all` completes, including coverage collection.
- The unchanged self-metric gate fails first at the **70-point function hotspot
  limit**. The frozen broader-scope maximum was 238.11; after is 151.42. Remaining
  complexity/source-size budgets and the retained escape hatch are not waived.
- Regenerated strict policy evidence has **no stale artifacts** but still fails
  `configured-rules`, `partial`, `reliability-finding`, and `test-failure`.
  Observed test/practice/reliability failure lists are empty; incomplete evidence
  is not converted into success. Six existing module-risk limits remain exceeded.

Thresholds, risk formulas, fail-closed policy behavior, and evidence-completeness
requirements were not relaxed. Further daemon refactoring, exhaustive dead-code
analysis, remaining hotspots, and complete body-level evidence remain follow-up
work.
