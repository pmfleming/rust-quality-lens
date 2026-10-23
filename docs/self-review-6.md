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
  cache version 18 invalidates results made with the old candidate selection.
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
  guesses, and no longer links executables as dependencies. Cache version 18
  invalidates the previous dependency model. The retained live resolver journey
  uses a deliberately different library name. It **failed at runtime** before the
  fix (one unresolved dependency; exit 101), then passed after it (exit 0).
  Refining the same journey to give the binary and library the **same target
  name** exposed a second missed-candidate case (one resolved definition instead
  of two). Candidate selection now also distinguishes target kinds; the refined
  journey fails before that correction and passes afterward, without another test
  registration.
- Reject calibration project names that collide after output-path normalization,
  before measuring or overwriting any project. The new native calibration journey
  covers single-crate and workspace layouts, real published distributions,
  malformed input, unnamed projects, and collision rejection without replacing
  the previous report.

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

## Results and limits

Measured source revision: `6d0c5e3`; baseline: `5be2262`. The original source was
also measured in a detached worktree using evaluator `6d0c5e3` (same production
measurement code as the final source). This control left complexity, SLOC and
clone counts unchanged, but recovered previously missed reuse. The temporary
worktree was removed after collecting `target/self-review-6/corrected-before/`.

| Measure | Original | After |
| --- | ---: | ---: |
| Maximum function cognitive complexity | 14 | 11 |
| Maximum function cyclomatic complexity | 19 | 19 |
| Sum function cognitive complexity | 2,049 | 2,035 |
| Maximum function Halstead effort | 254,915 | 201,196 |
| Maximum function hotspot | 99.62 | 95.65 |
| AST clone groups | 4 | 3 |
| Token clone groups | 110 | 109 |
| Duplicated nonblank lines | 1,300 | 1,276 |
| Escape-hatch findings | 1 | 0 |
| Line coverage | 88.89% | 90.20% |
| Covered / executable lines | 19,155 / 21,548 | 19,455 / 21,569 |
| Calibration module line coverage | 31.50% | 97.56% |
| Passed / skipped test registrations | 187 / 8 | 185 / 8 |
| Measured source nonblank lines | 31,781 | 31,847 |
| Total tracked Rust physical lines | 34,153 | 34,234 |
| Mean locality | 96.8732 | 96.8467 |
| Mean observed leverage | 24.4118 | 25.0365 |

**LOC reduction and average locality improvement were not achieved.** The added
regression protection and explicit setup cost more lines than were retired. We
retained that protection rather than chasing a numerical deletion quota. No
unused production feature was identified for safe retirement; the cleanup removes
redundant setup/initialization and low-value implementation assertions, not public
compatibility. Remaining small compiler visitor clones have distinct typed walk
operations; they were not replaced with obscuring macros to satisfy the clone gate.

The corrected-baseline mean leverage is **24.7794**, so the entire observed
24.4118→25.0365 change is not a refactoring gain. The shared native measurement
boundary has five observed module consumers. Module count changes from 136 to 137;
means are descriptive, not evidence that every module improved. The four targeted
function hotspots changed as follows (extracted helpers are included in all totals):

- `FactVisitor::record_impl_fact`: 99.62→1.40.
- `Scopes::enter`: 79.18→8.34.
- `RunContext::new`: 91.62→61.88.
- `policy::run_check`: 68.99→52.08.

The final byte-identical-file comparison covers 97 files under `src`,
`rust_helpers/src` and `compiler_helpers/src`: 11,614→11,617 covered lines, with
**no previously covered lines lost**. This includes inline tests and is not a
per-test protection proof. Aggregate coverage denominators changed. The live
renamed-library regression's red/green result supplies a separate runtime
sensitivity check; it was not a compilation-error "detection".

### Gate status

- Workspace tests, strict Clippy, formatting, bundled-helper parity, rustdoc,
  Python tests, package/external-consumer smoke, self verification, `measure all`,
  and SARIF export pass.
- All declared self-owner behaviors report observed passes;
  `rust.testing.required-owner` now passes with complete execution evidence.
- Strict policy still exits **1** for `configured-rules` and `partial`. There are
  no observed test/practice/reliability failures and no stale artifacts in this run.
- The two lexical impl-owner limitations are gone (94 resolved impl observations,
  zero unresolved). Type evidence still abstains for unexpanded macros. Four
  semantic references remain unresolved in the separately configured
  `compiler_helpers` project: its three root imports and `architecture::capture`.
  Architecture/topology evidence remains partial; this was not relabeled complete.
- The self-metric script still exits **1**. Remaining exceeded budgets are hotspot
  95.65 versus 70, SLOC 31,847 versus 22,500, and three AST clone groups versus zero.
  The cognitive-complexity and escape-hatch gates now pass. No budget was raised.
- Eight optional tests remain skipped. A new live pinned-compiler campaign,
  independent production-project calibration, and independent JSON Schema
  validation were not performed (no independent schema validator is installed).

Machine-readable results: `acceptance/self-review-6-results.json`. Raw local
artifacts and red/green logs: `target/self-review-6/`. Delivery artifacts are rebuilt
under `target/analysis/` after the final documentation commit so Git-bound
fingerprints describe that checkout.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
bash scripts/check-bundled-helpers.sh
cargo run --locked --bin rqlens -- verify
cargo run --locked --bin rqlens -- measure all
cargo run --locked --bin rqlens -- check --fail-on partial --fail-on test-failure \
  --fail-on practice-failure --fail-on reliability-finding
bash scripts/check-self-metrics.sh
cargo run --locked --bin rqlens -- sarif
```
