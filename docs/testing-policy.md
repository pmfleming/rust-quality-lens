# Testing policy

Protect important behavior, not test counts or line-coverage targets. This is the
interpretation contract for the optional testing-policy review.

## Ownership

Give each important behavior one primary test or cohesive suite at the highest
practical level: end-to-end for critical journeys, integration for real component
interactions, contract for public interfaces and compatibility. These are roles,
not a compulsory hierarchy. Keep focused local owners for algorithms, meaningful
boundaries, deterministic concurrency, cryptography/numerics, transformations and
cases that cannot reliably or efficiently be exercised higher up.

Add representative real vertical journeys where needed, then replace overlapping
lower-level tests rather than permanently stacking coverage. Supporting tests
must protect distinct important risks. Test shared behavior once and exceptions
separately. Doubles can test orchestration or recovery but never establish coverage
of the implementation they replace.

## Value and removal

Existing tests are evidence, not requirements. Unique assertions need not survive
when they protect no important requirement. Review duplicate protection, obscure
permutations, incidental implementation details, architecture constraints and
obsolete behavior for removal. Layouts, caches, allocations, retry schedules,
request counts, literals and ordering require an explicit public, correctness,
safety, security, compatibility or performance requirement to justify retention.

Reduce matrices to representative mechanisms, retaining materially important
security, destructive-operation, recovery, integrity, compatibility and regression
combinations. Prioritize risk first and substantial removable families second.
No deletion quota, test-count reward or mandatory replacement for low-value cases.

## Retirement and verification

Remove compatibility tests, and separately authorize production removal, only
after explicit withdrawal of support. Deprecation or an old test is insufficient.

Preserve critical journeys, public contracts, meaningful regressions, integration
boundaries, security, destructive operations, data integrity, recovery, significant
failure modes and explicit correctness/performance guarantees. Where practical,
verify retained owners using isolated controlled failures: successful baseline,
intended behavioral failure, successful restored run. Compile failures or unrelated
timeouts do not demonstrate sensitivity.

## Interpretation limits

Declarations, execution observations and heuristic judgments remain separate.
Aggregate coverage, syntax, names, clones and assertion counts cannot prove value,
semantic duplication or test-specific execution. Missing evidence stays unknown.
Heuristic advice is non-blocking and always requires review. The lens never deletes
tests, withdraws support, changes production code, or weakens a coverage gate.
Current risk formulas and baseline compatibility remain unchanged.

## Opt-in ownership review

```toml
# rqlens.toml
[testing_policy]
manifest = "rqlens-testing.toml"
evidence_inputs = ["docs/requirements.md"] # optional local documents/fixtures
```

```toml
# rqlens-testing.toml
version = 1
[[behaviors]]
id = "checkout"
description = "A customer can complete an order"
risks = ["critical_journey", "data_integrity"]
requirement = "docs/requirements.md#checkout"
scope = "default features, host integration tests; not live provider certification"
expectations = ["order is persisted", "failed payment leaves no completed order"]
real_boundaries = ["application", "database"]
substituted_boundaries = ["payment provider"]
reviewer = "checkout-maintainers"
review_revision = "requirements-v1"
required = true
[behaviors.owner]
role = "end_to_end"
rationale = "Representative journey through the real application and database"
tests = [{package = "shop", target = "checkout", path = "tests/checkout.rs", name = "complete_order"}]
```

Use exact `package_name`, `target_name`, `relative_path`, and `qualified_name` values
from `test_quality.json` as selector `package`, `target`, `path`, and `name`.
The existing raw `path` field is preserved for prior consumers. An owner
suite is an explicit list, not a glob. Empty suites request an ownership review.
Roles are `end_to_end`, `integration`, `contract`, or `local`; all require rationale.
Optional `[[behaviors.supporting]]` records contain `test = { ... }` and
`distinct_risk = "..."`; an empty rationale requests review, never deletion.
Scope is maintainer-declared: only the actual configured Cargo context is observed,
not arbitrary platforms or all environments named in prose.

```sh
rqlens measure correctness-run
rqlens measure test-quality
# Or: rqlens measure all (execution precedes ownership review)
```

`test_quality.json` retains syntax hints and adds versioned `data.testing_policy`:
declared behaviors, exact owner resolution, bounded execution observations,
limitations and advisory findings. Test-quality never executes tests itself.
Missing/stale/partial execution cannot become a pass. Duplicate qualified names
across targets abstain because the legacy runner cannot reliably attribute them.
Unsupported external harnesses remain unresolved. Invalid configured manifests
produce an unavailable/partial review; absent configuration preserves old behavior.
The manifest and `evidence_inputs` are automatically fingerprinted. Paths must be
project-relative, without `..`; references in prose are not automatically fetched.

Schemas are exposed by `config-schema` (including the `testing_policy` property's
`x-manifest-schema`) and `artifact-schema`.

## Consolidation and compatibility review

Add explicit reviewed families to the manifest; syntax alone cannot nominate safe
deletions. Each disposition names a behavior and exact tests, a reason, reviewer,
review revision, preserved risks and estimated effort:

```toml
[[dispositions]]
id = "checkout-overlap"
behavior = "checkout"
action = "consolidate"
tests = [{package = "shop", target = "shop", path = "src/cart.rs", name = "tests::duplicate_checkout"}]
reason = "The retained journey protects this same public behavior"
reviewer = "checkout-maintainers"
review_revision = "requirements-v1"
preserved_risks = ["Focused transaction rollback test remains"]
estimated_effort_hours = 2.0
```

Actions: `consolidate`, `low_value`, `reduce_matrix`, `retire_compatibility`.
Low-value cases need no replacement; consolidation/matrix candidates require
observed replacement execution. A declared primary owner or distinct-risk support
cannot be recommended for removal without further risk review. Unknown selectors
remain unresolved. Candidates are review-required even after prerequisites pass.

Retirement also requires `[dispositions.support_decision]` with `withdrawn = true`,
`document`, `effective` (support version/date) and `approved_by`. The document must
exist in configured `evidence_inputs`. This is a maintainer declaration, not an
independent interpretation of the decision or its effective version. Shared tests
protecting another retained behavior still require distinct-risk review. No code
or test is automatically changed.

`data.testing_policy.candidates` lists replacement owners, prerequisites, declared
removals and estimates. Within the consolidation queue, estimated registrations
removed per hour prioritizes substantial families; protection gaps come first.
Estimates are not observed deletions, a quality score or a quota. Fresh clone and
coverage artifact references are contextual only, never proof of equivalence.
Findings share existing review, policy and SARIF projection. Review-only rule IDs
are not supported as blocking deletion rules.

## Explicit owner gate

```toml
[policy.rules."rust.testing.required-owner"]
level = "error"
max = 0
```

Only `required = true` behaviors participate. Unresolved owners or observed owner
failures exceed the limit. Unknown, skipped, stale, ambiguous execution, no required
behaviors, an unconfigured policy or incomplete inputs make evaluation unavailable,
not passed. The gate checks resolution/execution in the actual Cargo scope, not the
truth of prose requirements. Configured-rule prerequisites follow the explicit
rule-to-family contract: this rule requires fresh, complete test-quality evidence,
including its current execution attribution, not unrelated practices, reliability
or architecture artifacts. Unknown rule IDs still abstain. Use `--fail-on partial`
to additionally require global evidence completeness. Waivers and exclusions
remain visible under existing policy semantics. No deletion, count,
coverage or sensitivity score is enforced.

## Optional fault evidence

Set `testing_policy.fault_evidence = "target/testing-faults.json"` to import bounded
experiments performed by a trusted maintainer/harness. The lens does **not** execute
commands in this file, inject faults or modify code. Perform experiments only in
disposable worktrees/fixtures with explicit resource/time limits, no live destructive
operations or unintended external effects. Restore sources and rerun after failure.

First run `measure test-quality` and copy its
`data.testing_policy.sensitivity.subject_fingerprint` into the report. A missing
configured report is initially unavailable. This subject includes source, manifest,
configuration, toolchain, helper and Git context **excluding the fault report itself**;
the ordinary artifact fingerprint includes the report too. Do not also put the
report or its parent in `evidence_inputs` or `fingerprint_inputs` (circular evidence).
After writing the report, rerun correctness and test-quality for fresh owner status.

The version-1 JSON report contains `version`, `input_fingerprint`, `producer`,
`producer_version`, and `records`. Each record has:

- `id`, `behavior`, exact primary `owner` selector;
- `fault`, project-relative `fault_path`, `expected_failure`;
- `baseline`, `injected`, `restored`: each with a nonempty command array, output
  text, and status (`passed`, `assertion_failed`, `compile_failed`, `timed_out`,
  `not_run`, `inconclusive`).

See `config-schema` → `properties.testing_policy.x-fault-evidence-schema` for the
wire shape. Claims are labeled `imported_observations_not_independently_verified`.
Only passing baseline/restoration plus an intended assertion failure yields
`reported_detected`; compilation failures/timeouts are inconclusive, an injected
pass is `survived`, and an unexecuted injection is `not_run`. Stale/malformed input
makes the configured measurement partial. This does not certify that the imported
commands/logs are truthful or that all important defects are detected.

Required behaviors without bounded detection evidence receive non-blocking advice
to try a representative controlled failure where practical. Existing cargo-mutants
results are linked through `rust_practices.json` / `rust.tests.mutation`; aggregate
results are never automatically credited to a particular owner.

See [the implementation plan](testing-policy-plan.md) for acceptance criteria.
