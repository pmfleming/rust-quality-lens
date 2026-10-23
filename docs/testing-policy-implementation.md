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
