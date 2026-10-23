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

Use exact `package_name`, `target_name`, `path`, and `qualified_name` values from
`test_quality.json` as selector `package`, `target`, `path`, and `name`. An owner
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

Schemas are exposed by `config-schema` and `artifact-schema`. See
[the implementation plan](testing-policy-plan.md) for delivery and acceptance criteria.
