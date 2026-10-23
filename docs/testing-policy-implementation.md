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
