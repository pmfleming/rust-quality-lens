# Testing policy implementation record

## Stage 1 — interpretation contract

Published the policy and plan, qualified coverage/test-count interpretation, and
updated catalog advice. Formatting and diff checks passed. No metric/gate change.

## Stage 2 — ownership MVP

Added optional strict TOML declarations, exact Cargo-aware selectors, fingerprinted
policy inputs, versioned test-quality review and schema, and unavailable/unknown
semantics. `measure all` now executes correctness before ownership review. No new
runner and no test deletion. The real CLI workflow owns manifest→Cargo execution→
artifact freshness behavior in `tests/testing_policy.rs`.

Validation: workspace tests, strict workspace/all-target Clippy, formatting, diff
checks, and `rqlens measure test-quality` against this checkout passed. The self
run is unconfigured at this stage; it does not certify behavior ownership.

Limits: legacy execution cannot attribute duplicate qualified names across targets;
ownership explicitly abstains. Manifest scope remains a declaration, not automatic
cross-platform certification. Full self-pilot and fault sensitivity follow later.

## Stage 3 — reviewed consolidation

Added explicit value/matrix/consolidation/retirement dispositions, conservative
protection of other declared owners, support-decision inputs, contextual clone/
coverage references and deterministic estimate-based ordering within the review
queue. Non-Rust manifest changes now retain measurements in changed-code review.
No semantic duplicate detector or automatic deletion was introduced.

The primary CLI journey now exercises replacement prerequisites, low-value cases,
protected owners, explicit support withdrawal and exact policy/SARIF finding
agreement. Focused CLI tests, strict Clippy, formatting, diff checks and a self
`measure test-quality` run passed. Broad validation is repeated in the self-pilot.

## Stage 4 — sensitivity and explicit requirements

Added opt-in, fingerprint-bound imported fault experiments with baseline/injected/
restored outcomes and explicit imported authority. Compile failures are not defect
detection. The CLI never executes imported commands. Existing mutation evidence
remains aggregate context. Added `rust.testing.required-owner` with documented
prerequisites, positive/negative/unavailable evaluation and fail-closed report
validation; value and sensitivity recommendations remain non-blocking.

The primary CLI fixture actually changes a library result, observes its owner's
assertion failure, restores and reruns, then distinguishes a real compile failure
and stale source evidence. It also verifies required-owner evaluations, SARIF
agreement and rejection of contradictory missing findings. Commands now run from
outside the fixture root, catching and fixing relative policy-input fingerprinting.
Workspace tests and strict workspace/all-target Clippy passed, as did formatting
and diff checks. No score formula, baseline rule or default gate changed.

## Stage 5 — self-pilot and refinement

Enabled the optional policy in `rqlens.toml`, with four bounded behavior owners in
`rqlens-testing.toml`. This is not an exhaustive requirement inventory. The new
feature's primary owner is a cohesive three-journey CLI suite: ownership/value
review, required-owner enforcement, and scoped fault evidence. Separating the
journeys improves independent execution and diagnosis; it is **not** test reduction.
A complete external Cargo fixture now demonstrates a passing configured gate as
well as failed and unavailable evaluations through the real producers/reporters.

The lens's first self-run identified oversized new workflow helpers. Extracted
meaningful setup, publication, reporter agreement and failure phases rather than
adding overlapping unit tests. Moved circular-input validation into the testing
policy config boundary, reducing `LensConfig::load` hotspot 80.75→69.10. Preserved
the existing raw test `path` API and added `relative_path` for exact manifest
selectors. Execution context now reports the configured command/scope separately
from availability. Static assertion/call/ignored hints explicitly caution against
judging value or adding tests merely to satisfy counts.

### Deliberate test-value changes

- Removed `tool::tests::tool_scope_and_backend_requirements_are_explicit`: one
  registration containing **11 private routing assertions**. The existing CLI
  measurement bundle and compiler integration suites already own the important
  public behavior. No new higher-level test was built just to remove this family;
  private routing details are not independent requirements.
- Removed two schema-title literal assertions without replacement: presentation
  wording is not a compatibility requirement.
- Kept required artifact envelope fields and schema-version checks, but removed
  the unsupported requirement that envelopes contain *exactly* those keys. Public
  schemas allow additive fields. Kept numerical, type-attribution, trust,
  publication, compiler, ignored-live and regression protections.
- The lens classified the declared private-routing family as a
  `value_review_candidate` before removal. Removed its disposition from the active
  manifest afterward; historical rationale lives here, not in dead selectors.

This is a small justified deletion, not a claim that a large overlapping suite has
been eliminated. The feature workflow split adds two registrations and the routing
family removal removes one: the pilot's net registration count **increases by one**.

### Comparable pilot observations

Same configured self source roots and model, immediately before/after stage-5
refinement; **not** a pristine pre-feature comparison:

| Observation | Before | After |
| --- | ---: | ---: |
| Maximum hotspot in new testing-policy code/tests | 116.48 | 59.98 |
| Maximum function hotspot, entire measured scope | 116.48 | 99.62 |
| Maximum cyclomatic complexity | 26 | 19 |
| Maximum cognitive complexity | 14 | 14 |
| Source nonblank lines | 31,767 | 31,781 |
| Discovered test statuses | 186 passed / 8 skipped | 187 passed / 8 skipped |
| Aggregate line coverage | 88.90% | 88.89% |

Source size increased by 14 lines while local complexity improved. Coverage
includes changed tests and different denominators; this is **not** an unchanged-
production coverage-retention or assertion-quality proof. No branch coverage was
available. No score formula, threshold, source exclusion or baseline was weakened.

### Real lens fault sensitivity

In a disposable source copy, changed the real
`src/testing_policy/requirements.rs` unknown-execution check from
`row.status == "unknown"` to `row.status == "never_unknown"`.
The retained `required_ownership_gate_is_fail_closed` CLI journey:

1. Passed with the baseline implementation (one selected test, exit 0).
2. Failed with the defect (one selected test, exit 101): the actual policy output
   incorrectly said `passed` instead of `incomplete`.
3. Passed after restoration (one selected test, exit 0). All tracked Rust sources
   in the copy matched the active checkout byte-for-byte afterward.

This exercises the real lens implementation, not a mocked response or merely a
compile failure. It is one bounded representative defect, not mutation adequacy
certification. Commands/log hashes and historical source fingerprint are in
[`acceptance/testing-policy-results.json`](../acceptance/testing-policy-results.json).
Raw evidence is under `target/testing-policy-pilot/`, including the disposable
source copy and `fault-{baseline,injected,restored}.log`.

### Validation and remaining failures

Passed: workspace tests, strict workspace/all-target Clippy, formatting, workspace
all-target check, bundled-helper parity, warning-free rustdoc, 14 Python acceptance
tests, six Python script tests, package/external-consumer smoke, self `verify`,
self `measure all` execution and SARIF export. Eight ignored tests remain skipped;
no live-fixture/platform certification is claimed. An independent JSON Schema
validator was unavailable; machine-readable schema CLI and typed consumer tests ran.

**Strict self policy and self-metric gates still exit 1.** The observed test and
practice failure lists are empty, but evidence is partial. Existing unresolved
lexical `Inventory` impl ownership in `src/compiler_inventory/architecture_policy.rs`
and `src/compiler_inventory/projection.rs` makes correctness/test-quality partial.
All six self owner selectors resolve, but execution attribution deliberately remains
`unknown`; `rust.testing.required-owner` is `incomplete`, not silently passed.
The source/type completeness boundary was not relaxed to make the pilot green.

Global budgets also remain exceeded: hotspot 99.62 vs 70, cognitive 14 vs 11,
31,781 source lines vs 22,500, and four AST clone records vs zero. Cyclomatic 19
is within 20; token clones 110 are within 216. No gate is waived and no release
acceptance criterion is closed by these results.

Reproduce the main run with:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked --bin rqlens -- verify
cargo run --locked --bin rqlens -- measure all
cargo run --locked --bin rqlens -- sarif
bash scripts/check-self-metrics.sh
cargo run --locked --bin rqlens -- check --fail-on partial --fail-on test-failure \
  --fail-on practice-failure --fail-on reliability-finding
```

Commits change the Git component of input fingerprints; rerun measurements after
committing before interpreting freshness. The checked-in result describes the
pre-commit source snapshot, not an automatically refreshed certification.

### Follow-up boundary

The external consumer here is a real disposable Cargo fixture, not an independent
production project. Independent real-project calibration, semantic duplicate/value
inference, external harness execution and any new risk-model version remain
unimplemented/deferred. The delivered heuristics rely on reviewed declarations;
they are not calibrated defect probabilities. A future separation of test-execution
attribution confidence from unrelated type-inventory completeness needs its own
contract and regression evidence, not a waiver of today's partial results.
