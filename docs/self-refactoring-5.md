# Self-review: scoped refactoring and measurement corrections

Compared with `687c732`, using the corrected lens on **both** revisions and the
same source roots. The self-config now includes previously omitted lexical
ownership and compiler architecture integration tests. Fixtures remain excluded;
no score weights, thresholds or confidence requirements were relaxed.

Full local evidence and the detached baseline worktree are under
`target/self-review-5/`; retained aggregates are in
[acceptance results](../acceptance/self-refactoring-5-results.json).

## Results and tradeoffs

| Metric | Before | After |
| --- | ---: | ---: |
| Maximum cognitive complexity | 16 | 14 |
| Aggregate cognitive complexity | 1,959 | 1,954 |
| Maximum cyclomatic complexity | 21 | 19 |
| Aggregate cyclomatic complexity | 4,692 | 4,701 |
| Maximum Halstead effort | 326,964.71 | 254,915.06 |
| Aggregate Halstead effort | 17,996,403.13 | 17,825,404.76 |
| Token clone groups | 105 | 103 |
| AST clone groups | 4 | 4 |
| Duplicated nonblank lines | 1,302 | 1,218 |
| Duplication percentage | 4.31% | 4.03% |
| Mean locality | 96.8274 | 96.8750 |
| Mean observed-consumer leverage | 24.2857 | 24.1270 |
| Nonblank Rust lines | 30,220 | 30,209 |
| Escape occurrences | 0 | 0 |

This is a targeted improvement, not success on every objective. Eight added
functions (including regression tests) increase aggregate cyclomatic complexity.
Observed-consumer leverage decreases slightly when unnecessary dependencies on
artifact handling are removed. Internal helper reuse increases, but this module
reach metric does not measure that. Net Rust physical LOC decreases by only three
lines; regression coverage consumes most implementation savings. Documentation
and evidence are additional non-code lines.

## Refactoring

- Replace repeated fixture compilation and tampered-report checks with local
  helpers; keep every behavioral assertion and error-propagating test path.
- Split large schema builders by responsibility, reuse property/summary builders,
  and share complexity aggregation between full and fallback hotspot reports.
- Simplify inventory selection and module indexing; remove an unnecessary
  intermediate hotspot buffer and redundant architecture rule filtering.
- Remove the graph's deep copy of every `FileFacts`: retain only its needed
  unresolved-reference count; map confidence uses the existing source inventory.
- Keep producer-only numeric sorting support within producers rather than
  depending on artifact handling. Move JSON object members instead of cloning
  them; borrow topology backend names until serialization.

## Measurement repairs

- Clone summary model 2 counts unique **nonblank** snippet lines, matching its
  denominator. On the unchanged baseline this corrects 1,306 to 1,302 lines
  (4.32% to 4.31%); that change is not credited to refactoring. Empty inventories,
  overlapping instances and exclusion of AST groups have regression coverage.
- Containment edges must match Cargo target as well as package and module path;
  a regression fixture has conflicting children in two targets.
- Published schemas now admit the emitted `capabilities` field and describe map
  module identities as objects rather than strings. All other published schema
  content matches the frozen evaluator exactly.
- Both evaluations use the expanded self-scope. Frozen/corrected baseline
  hotspot numeric fields agree; helper sources and complexity/effort models
  are unchanged. Authored-syntax and semantic-confidence limitations remain.

## Validation and remaining work

Passed: 185 workspace Rust tests (eight environment-dependent tests ignored),
Clippy with warnings denied, formatting, bundled-helper parity, rustdoc with
warnings denied, and `rqlens verify` (17 passed, 11 skipped). The changed ignored
compiler-policy test also passes separately with the existing pinned adapter:

```bash
RQLENS_TEST_COMPILER_DRIVER="$PWD/target/compiler-helper-build/debug/rqlens-compiler-inventory" \
  cargo test --locked --test compiler_architecture \
  compiler_policy_detects_macro_mutations_and_rejects_partial_or_forged_evidence -- --ignored
```

`scripts/check-self-metrics.sh` still fails its existing hotspot ceiling of 70.
No thresholds were raised. Compiler warnings find no statically detected dead
code; removing redundant work is not proof of absence of runtime-dead paths.
Remaining priorities include aggregate cyclomatic complexity, meaningful reuse
beyond import reach, four AST clone groups, and substantial LOC reduction without
removing coverage. No live mutation or coverage campaign is claimed.

To reproduce, build/freeze the baseline binary, create a detached baseline
worktree, and measure `hotspots`, `clones`, `escape-hatches`, `locality` and
`leverage` with the current binary on both revisions. Use the current source-root
list in both configurations, separate outputs, and unchanged helper sources.
