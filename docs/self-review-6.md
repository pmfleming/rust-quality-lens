# Self-review 6: measurement boundaries and value-based refactoring

Work starts at `5be2262`. Original self evidence is retained locally under
`target/self-review-6/before/`; final comparisons and validation follow below.
No score weights, gates or budgets are relaxed.

## Measurement corrections

- Unresolved lexical impl owners and unsupported impl target shapes now remain
  **type-identity** limitations. They no longer invalidate successfully parsed
  source bodies and test inventories. Unresolved impl observations remain counted;
  type-health still abstains, including aliases, generic shadowing and non-path
  targets. The native lexical-ownership journey checks both partial type evidence
  and complete syntax evidence for the same compiling input.
- Semantic candidate selection now includes a binary's own package library by
  its Cargo library target name. Previously these calls were never submitted to
  rust-analyzer and observed reuse could incorrectly be zero. Declared children
  with shared physical files are also resolver candidates. Actual edges still
  require resolved definitions; names alone do not prove a dependency. Semantic
  cache version 16 invalidates results made with the old candidate selection.
- The native resolver journey exercises library→module and binary→same-package
  library resolution, then the cache, rather than merely testing a classifier.

These are evaluator corrections, not architectural improvements. Topology and
confidence changes must be distinguished from refactoring gains. Type ownership
and macro/compiler limitations are not waived to obtain complete syntax evidence.

## Initial cleanup

Simplified lexical item binding and impl-owner extraction into explicit domain
operations, reducing deeply nested control flow without changing resolution rules.
Removed the test campaign's wildcard import using explicit dependencies rather
than suppressing the escape-hatch finding. Bundled helper copies remain identical.

Validation at this checkpoint: native lexical and trust CLI suites, the live
resolver/cache test, and all helper crate tests pass.

## Refactoring and test ownership

- Separate compiler-inventory acquisition from `RunContext` orchestration, keeping
  one shared capture when type and architecture consumers both request it.
- Reuse normal configuration/path/metadata preparation for calibration instead of
  maintaining a second full `LensConfig` initializer. Calibration still deliberately
  ignores project policy and retains its existing source-root selection.
- Separate regression-evidence failure handling from policy orchestration; retain
  the explicit report fields and borrow waived findings instead of moving each
  group's list through a separate accumulation step.
- Share the native CLI measurement boundary across compiler and lexical fixtures.
  Include the new `tests/support` directory in self measurement: no new code is
  hidden from the source metrics.
- Qualify the two cross-module `impl super::Inventory` targets. This removes an
  unnecessary imported-name ambiguity for readers and the lexical type inventory;
  it does not teach the inventory to guess imported owners.
- The detached resolver now uses actual library target names, not package-derived
  guesses, and no longer links executables as dependencies. Cache version 17
  invalidates the previous dependency model. The retained live resolver journey
  uses a deliberately different library name. It **failed at runtime** before the
  fix (one unresolved dependency; exit 101), then passed after it (exit 0).
- Reject calibration project names that collide after output-path normalization,
  before measuring or overwriting any project. The new native calibration journey
  covers single-crate and workspace layouts, real published distributions,
  malformed input, and collision rejection without replacing the previous report.

Test retirement is based on ownership, not a count target:

| Removed test | Retained primary protection / distinct local risk |
| --- | --- |
| `calibration_project_specs_are_validated_and_normalized` | Native calibration CLI journey; keep hand-counted distribution/threshold tests for numerical edge cases |
| `required_mode_rejects_an_unavailable_analyzer` | Native trust journey now requires failure and preservation of published output; keep live resolver/cache and protocol tests |
| `run_context_reuses_inventory_graph_and_artifact_fingerprint` | Snapshot mid-run-edit test and CLI publication-integrity journey; the artifact contract test owns both producer and consumer rejection of contradictory confidence; remove pointer identity / schema implementation assertions |

The overlapping source/correctness inventory test retains its distinct scope
checks, but no longer constrains its implementation to specific `Arc` identities.
No compatibility mode or important compiler/macro protection was retired.

Workspace tests, strict Clippy, formatting and bundled-helper parity pass at this
checkpoint. Numerical comparisons and final gate results follow below. Do not
interpret a test-count reduction as evidence of adequacy; the new native journey
executes previously uncovered calibration production paths.

A byte-identical-file coverage comparison caught one lost producer error-path line
(`src/contracts.rs:41`). The confidence-rejection assertion was retained at the
artifact contract boundary rather than restoring the mixed snapshot/cache/schema
test. This is a distinct producer obligation, not covered merely by testing a
consumer's rejection of malformed reports.
