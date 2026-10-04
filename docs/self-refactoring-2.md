# Self-review and refactoring after reporter agreement

Baseline: `13889de`. Evidence: `acceptance/self-refactoring-2-results.json` and
`target/self-refactor-2/`. This is a bounded quality improvement, not trust/release
certification.

## Review using RQLens

Built and froze the baseline executable, then measured the unchanged configured
scope (`src`, `rust_helpers/src`, `tests/cli.rs`) before and after refactoring.
Neither source scope, scoring, thresholds nor the 22,500-line budget changed.
No tests or implementation were moved outside the measured scope.

The lens identified repeated fixture/config construction, duplicate AST-clone
processing and evidence loading, and complex rust-analyzer waiting loops.
The highest remaining hotspots are measurement orchestration (69.22), policy
orchestration (68.99), correctness production (68.99), and configuration loading
(68.26). This pass does not claim to eliminate all hotspots.

## Frozen-evaluator results

| Measurement | Before | After |
|---|---:|---:|
| Nonblank measured source lines | 22,499 | 22,457 |
| Token-clone groups | 83 | 72 |
| Duplicated lines | 1,309 | 1,046 |
| Duplication | 5.82% | 4.66% |
| AST-clone groups | 0 | 0 |
| Cyclomatic complexity sum | 3,211 | 3,209 |
| Cognitive complexity sum | 1,396 | 1,383 |
| Function hotspot sum | 13,317.25 | 13,226.52 |
| Maximum hotspot | 69.22 | 69.22 |
| Cyclomatic / cognitive maxima | 20 / 11 | 20 / 11 |
| Mean locality (same 92 modules) | 97.4973 | 97.5299 |
| Mean leverage (same 92 modules) | 22.9348 | 22.8261 |
| Detected escape hatches | 0 | 0 |

Totals include the added protocol regression and expanded coverage-input tests.
Source/semantic evidence remains partial; these are scoped source observations,
not complete compiled inventories.

### Complexity and topology trade-offs

- `LspClient::request`: cyclomatic **9→4**, cognitive **9→1**.
- `LspClient::wait_until_ready`: cyclomatic **9→4**, cognitive **10→2**.
- New shared `wait_for`: cyclomatic **7**, cognitive **6**. Including the helper,
  these routines improve **18→15 cyclomatic** and **19→9 cognitive**; the complexity
  has not merely disappeared into an unreported helper.
- External input collection: cyclomatic **10→8**, cognitive **9→8**.
- Evidence command handling: cyclomatic **18→12**; project commands **14→12**.
  Error propagation is shared at the successful-output reporting boundary.
- Correctness locality: **91→94**. Current-artifact helper leverage: **20→40**.
- Validation/fingerprint leverage falls **80→60 / 60→50** as callers stop depending
  on those internals directly. Average leverage therefore declines slightly;
  maximizing fan-in everywhere would reintroduce coupling rather than improve it.
- Combining source/test AST row generation increases that function's cognitive
  complexity **2→10**, cyclomatic **3→6**, and hotspot **22.29→57.29**, while removing
  duplicate grouping, validation and rendering. This local regression is included
  in the improved project totals, not hidden by them.

## Refactors

1. Share rust-analyzer receive/respond loops, preserving reply selection, server
   requests, loading errors, missing results, disconnection and absolute deadlines.
   A Unix scripted-transport test exercises interleaving; live native fixtures
   continue to pass. This is not new Windows/macOS certification.
2. Reuse current-artifact validation/freshness in SARIF and correctness, preserving
   their distinct error and partial-input behavior. Coverage tests now exercise
   the real disk/validation boundary for current, stale, partial, malformed,
   unsupported and missing artifacts rather than calling a bypass helper.
3. Share enabled external-adapter selection between ingestion checks and policy.
4. Consolidate source/test AST grouping and rendering without changing either
   scoring formula, minimum span, confidence or engine scope. Source subsets now
   borrow facts rather than cloning every string-bearing AST fact. No runtime or
   memory savings are quantified.
5. Centralize parser-only test configuration, fresh CLI fixture roots and shared
   fixture source text. Keep all existing assertions and test cases. All 34 saved
   fixture source/config/manifest files remain byte-identical.
6. Share report-result handling and replace duplicate path-resolution functions
   with one implementation and a semantic alias. Native `Path::join` retains
   absolute-path behavior.

## Effort, dead code and escape hatches

There is **no calibrated effort metric** in these artifacts. Complexity and
repetition reductions are not measured engineering hours or defect probabilities.

Strict workspace/all-targets Clippy passes with warnings denied. The lens's
`rust.dependencies.unused` check is still **skipped**, not verified-clean. No
exhaustive dead-code or unused-dependency result is claimed, and low graph reach
was not used to delete potentially reachable code. Redundant implementations were
removed; no unsupported claim of unreachable production code is made. Escape
hatches were already zero and remain zero in the same scope.

## Behavioral validation

These are historical counts for this refactor. The subsequent
[test consolidation](test-reduction.md) preserves its normalization regression in
`acceptance/test_trust_campaign.py` and records the smaller current suite separately.

- **194 normal Rust tests**, two live fixtures, **25 Python tests**.
- Strict Clippy, rustdoc, bundled-helper parity, package smoke, native self `verify`
  and unchanged self-metric gates pass.
- All **19 artifact schemas**, config schema and complete catalog are unchanged.
- Three frozen/candidate clone fixtures agree exactly after timestamp-only
  normalization: two empty controls and one positive fixture with two AST groups,
  one test-AST group, one token group and one responsibility group.
- Five self producers (hotspots, clones, locality, leverage, escape hatches) agree
  between frozen/candidate executables and between two candidate runs. Only
  `generated_at` and `measured_at` are removed; arrays and all other content remain.
  No executable-command provenance exclusions were needed in these comparisons.
- The previous 28 policy-ingestion cases, 19 practice cases, ten malformed-diagnostic
  cases, six source/import agreement cases and 30 rejection cases still pass.

Reproduce after workspace tests have created the existing CLI fixtures:

```sh
nix develop --offline --no-write-lock-file --command python3 \
  acceptance/self_refactor_campaign.py \
  --frozen target/self-refactor-2/rqlens --output target/self-refactor-new
```

The output directory must be fresh. The baseline executable is retained locally;
its SHA-256 is recorded in the results. The script writes analysis artifacts but
not source/configuration. Full `measure all` determinism is **not** claimed.

Strict policy remains blocked on `configured-rules`, `partial`,
`reliability-finding` and `test-failure`, with no stale artifacts and empty actual
`test_failures`/`practice_failures` lists. These policy labels are not assertions
that executed tests failed. Acceptance ledger validation passes; release remains
blocked. E2/E3/E4/E6 and T4 status is unchanged. No daemon project was touched.
