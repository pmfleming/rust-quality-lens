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

See [the implementation plan](testing-policy-plan.md) for delivery and acceptance
criteria. Configuration and evidence examples will accompany the implementation.
