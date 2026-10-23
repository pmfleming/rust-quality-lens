# Self-review 7: four independently committed steps

Starting revision: `163465a`. Evidence is retained locally in
`target/self-review-7/`. No thresholds, scoring weights or exclusions are relaxed.

## 1. Independent Cargo projects in semantic analysis

**Necessity:** the baseline self scan left four compiler-helper references
unresolved. A new native CLI fixture with two independent Cargo workspaces failed
before the change (exit 101: one unresolved reference). Merely linking the second
project exposed another defect: its files were incorrectly attributed to the
parent package. The fixture checks package identity as well as resolved reuse.

Changes:
- Discover actual nearest manifests for measured sources and link their real Cargo
  workspaces into rust-analyzer. Deduplicate workspace members. Do not invent
  dependency edges or override project toolchains with a synthetic shared crate.
- Collect independent-project metadata before capturing measurement inputs; keep
  it separate from the primary workspace's verification scope. Independent
  projects do not inherit the primary project's feature names. Offline and locked
  behavior remain respected, and mutable path dependencies join fingerprints.
- Treat the nearest manifest as an ownership boundary, including when metadata
  cannot be loaded. Do not mislabel an excluded/nested crate as its ancestor.
- Invalidate semantic caches (version 19). Existing detached-source fallback is
  retained when the primary Cargo project cannot be loaded.

Validation:
- The native fixture now passes: distinct package identities and dependencies,
  stable cold/warm results, and rejection of a subsequently unloadable project
  without replacing published evidence. Logs: `step1-red.log`, `step1-green.log`.
- This external-tool test is explicitly ignored by default, not silently passed
  when rust-analyzer is absent. It was executed here with:
  `cargo test --locked --test trust independent_cargo_projects_resolve_without_cross_project_guesses -- --ignored --nocapture`.
- Workspace tests and strict Clippy pass. `measure all` succeeds.
- Self semantic resolution: **4 unresolved → 0**; **99.71% → 100%** of collected
  references. Compiler-helper files now belong to `rqlens-compiler-inventory`, not
  `rust-quality-lens`. Maximum cognitive complexity remains 11.

This is authored-source dependency evidence, not proof that the pinned compiler
adapter builds or that macro/type inventory is complete. No rustc-private build
was substituted with the host toolchain. Compiler inventory still requires its
separately documented pinned driver. The native fixture validates separate Cargo
contexts; it does not certify every possible cross-toolchain installation.

## 2. Rule-specific policy prerequisites

**Necessity:** the retained required-owner CLI journey was strengthened to request
only correctness execution and test-quality evidence. Its rule evaluation passed,
but `check` still failed because three unrelated producers had not run. This new
assertion failed before the correction (`step2-red.log`, exit 101).

Removed the blanket four-producer requirement. The existing explicit contract
mapping in `rule_unavailability` is now the sole configured-rule prerequisite
resolver; this is not prefix guessing or a new permissive fallback. The published
blocking contract and testing-policy documentation now state this obligation.
Architecture error rules retain their independent prerequisite and unknown IDs
still evaluate incomplete. `--fail-on partial` retains the global completeness gate.

Validation:
- The same native journey passes without unrelated artifacts and with an actually
  partial architecture artifact (semantic analysis deliberately disabled).
- The explicit global partial check still fails, without falsely attributing that
  failure to configured rules. Missing, stale and contradictory required reports,
  unknown rules, unknown execution and observed owner failures still fail closed.
- No new test registration: these are phases of the existing primary owner-gate
  test. `step2-green.log` records its passing run.
- All **28 existing adversarial policy-ingestion cases** pass, including disabled,
  warning, unsupported, unavailable, collision, stale and malformed cases. These
  synthetic ingestion cases do not certify detector implementations.
- Workspace tests, strict Clippy and self `measure all` pass; stage artifacts are
  under `step2/`. This correction enables bounded consumer workflows; it does not
  claim that the self project's remaining macro/type limitations disappeared.
