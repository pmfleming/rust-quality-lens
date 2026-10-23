# Plan: risk-owned, value-based testing

Status: proposed; no runtime, scoring, gate, or test-deletion changes in this document.

## Recommendation and policy review

Adopt this policy as a **behavioral protection and test-value review**, not another
coverage target or an automatic test-pruning rule. The lens should help answer:

1. Which important behavior or risk does this test protect?
2. Where is its primary owner, and what real implementation does that owner exercise?
3. Does another layer protect a distinct risk, or merely duplicate protection?
4. What can be removed, and what evidence supports that decision?

The proposed policy is a strong improvement over “add tests / increase coverage.”
Preserve its four principles, with these interpretation safeguards:

- **Highest practical, not highest possible.** Reliability, runtime, diagnostic
  quality and controllability matter. Contract tests are an ownership category,
  not necessarily a higher execution layer than integration tests. One primary
  owner may be a cohesive suite with representative scenarios, not one giant test.
- **Local tests remain first-class owners** for algorithms, meaningful boundaries,
  deterministic concurrency, crypto/numerics, complex transformations, and behavior
  that cannot practically be exercised higher up. Do not require an E2E substitute.
- **A double limits the claim, not necessarily the test's value.** A stubbed payment
  provider can support a checkout orchestration claim, but not a claim about the
  real provider implementation or live protocol compatibility. A simulated fault
  can exercise real recovery logic without proving the dependency produces it.
- **Uniqueness is neither a retention requirement nor a deletion justification.**
  Review the underlying requirement and consequence of failure. Low-value unique
  assertions may go without replacement; distinct important risks must stay.
- **Risk first, numerical leverage second.** Prefer removing whole overlapping
  families, but do not invent extra work or withhold an essential protection
  because it only replaces one test. No required reduction percentage.
- **Explicit withdrawal is mandatory for obsolete compatibility.** A deprecated
  interface, old test, or apparently unused platform is not evidence that support
  has ended. Production removal is a separate, authorized change.
- **Demonstrated sensitivity is bounded evidence.** A passing suite, unchanged
  coverage, or a killed mutant alone does not prove adequate protection.

Retain critical journeys, public contracts, meaningful regressions, integration
boundaries, security, destructive operations, data integrity, recovery, significant
failure modes, compatibility, and explicit correctness/performance guarantees.
Retain materially important combinations in these areas even when reducing large
feature × failure-mode matrices elsewhere.

## Current implementation and gaps

| Existing surface | What it establishes / what must change |
| --- | --- |
| `src/producers/correctness.rs`, `src/facts/test_runner.rs` | Test inventory, execution status, syntax attribution and aggregate coverage observations. Neither test names nor module attribution establish behavioral ownership. |
| `src/measurement/paths.rs` | `kind` is path-based; `layer` is an architecture classification. Neither should be relabeled as an authoritative E2E/integration/contract/unit classification. |
| `src/producers/test_quality.rs` | Advisory assertion/call/ignored-test hints. These do not measure value; helper assertions, `Result`-returning smoke tests and compile-time checks require care. |
| `src/producers/coverage.rs`, `src/producers/function_risk.rs`, `src/review.rs` | Execution reach and CRAP/changed-line evidence, not assertion sensitivity or per-test ownership. Keep the measurements, qualify recommendations. |
| `src/risk_model.rs`, `src/measurement/scoring.rs` | Missing-test evidence and below-70% coverage contribute risk; failed/unknown/skipped test counts also contribute. These incentives need a separately versioned review, not silent changes. |
| `src/policy.rs` | Coverage decrease can trigger an explicitly requested regression gate. Intentional deletion of low-value coverage needs explicit review, not automatic suppression. |
| `src/producers/practices/mutation.rs` | Existing fresh cargo-mutants outcomes can support bounded defect-sensitivity evidence; they do not currently establish a behavior's primary owner. |
| `docs/test-reduction.md` | Historical 231→155 registrations mostly consolidated cases. Preserve that historical record; fewer registrations with the same assertions are not this policy's main objective. |

There is no need for a separate universal testing score. Extend the existing
`test-quality` review and reuse correctness, coverage, execution and mutation
artifacts. Do not build a parallel discovery or execution system.

## Proposed design

### 1. A small, optional behavior manifest

Add an opt-in `[testing_policy]` configuration section pointing to a versioned,
project-relative manifest (proposed filename: `rqlens-testing.toml`). Final field
names are an implementation decision, not an existing CLI contract.

Record only important behaviors and reviewed test families; do not require an
annotation on every assertion. Each behavior record contains:

- Stable behavior ID, description, risk categories, and requirement/incident link.
- Primary owner: exact discovered test IDs or an explicitly scoped suite; owner
  role (`end_to_end`, `integration`, `contract`, `local`) and rationale.
- Observable success/failure expectations; materially important scenarios.
- Real components/boundaries exercised, substituted components, and environments,
  features, platforms or protocol versions to which the claim applies.
- Supporting tests and the distinct risk each protects; local-owner rationale
  where applicable. Shared behavior has one owner; component exceptions are
  separate claims, not copies of the shared suite.
- Maintainer/reviewer and review revision. Compatibility records additionally
  reference an explicit support decision and its effective version/date.

Ownership means accountable protection, not exclusive execution of a code path.
One test can own several behaviors; one behavior has one primary test/suite owner
within a declared scope. Scope exceptions must be explicit, not conflicting owners.

Resolve selectors using Cargo package/target context and qualified test identity;
never match bare names across a workspace. Report unresolved, empty, conflicting
or overly broad selectors. Support external/non-libtest owners through bounded,
source-linked imported evidence later; until then mark them unsupported, not absent.

Use a separate reviewed disposition record for consolidation/retirement decisions:
retained owner, tests/families to remove, distinct-risk review, reason, expected
savings, evidence and approval. A low-value deletion can explicitly have no
replacement. Do not make a permanent ledger of every incidental assertion.

### 2. Keep declarations, observations and judgments separate

Extend `test_quality.json` with versioned behavior records, evidence references,
review candidates, and actionable protection gaps. Keep existing raw test facts.

Every result distinguishes:

- **Declared:** requirement, ownership, boundary realism and support policy supplied
  by maintainers; declarations alone are not verification.
- **Observed:** exact selected tests and results, command/context, fresh source and
  input fingerprint, covered scope, or targeted fault/mutation outcome.
- **Inferred:** possible duplication, implementation coupling or matrix redundancy;
  always review-required, with the heuristic and limitations visible.
- **Unknown/unsupported:** missing, stale, ambiguous or unavailable evidence; never
  interpreted as “unprotected,” “worthless,” “safe to delete” or a verified pass.

Fresh owner execution supports only the stated scope. Aggregate LLVM coverage
cannot identify which test exercised an implementation, which assertion checked
it, or whether two tests protect the same risk. Static clones, mock-like names,
call counts and assertion counts can nominate candidates but cannot establish
semantic equivalence, test value or real boundary coverage.

Hash the manifest and relevant fixtures/support decisions into freshness inputs.
Reuse artifact envelopes, typed validation, staged publication and confidence
semantics. An absent optional manifest means “not configured”; a configured but
missing/invalid manifest is an explicit incomplete/error result. Do not make all
existing projects partial merely because they have not adopted this feature.

### 3. Actionable advice, not generic coverage pressure

Initial advisory rule families should cover:

| Review condition | Recommended action |
| --- | --- |
| Declared important behavior has no resolved owner | Choose the highest practical owner; add a representative real vertical journey if appropriate. |
| Owner is failed, skipped, stale or unexecuted | Repair/run the owner or explain environmental constraints; do not recommend duplicate local tests by default. |
| An ownership claim includes a substituted implementation | Narrow the claim or add real integration/contract evidence for that boundary. |
| Multiple tests claim the same behavior without distinct-risk rationale | Review consolidation; identify retained owner and a concrete removable family. |
| Assertions constrain incidental internals | Confirm an explicit requirement or propose removal; a unique assertion is insufficient justification. |
| Large parameter/cross-product family | Group by underlying mechanism and keep representative cases plus materially important combinations. |
| Compatibility tests refer to withdrawn support | Propose scoped retirement with the support decision attached; otherwise ask for a support review. |
| Important risk lacks demonstrated fault sensitivity | Suggest a bounded controlled-failure experiment, not an arbitrary mutation-score target. |

Implementation-coupling candidates include internal layouts, incidental caches,
allocation patterns, exact retry schedules, nonessential call/request counts,
private representations, presentation literals and unsupported ordering. Any of
these may be essential when backed by a public, correctness, safety, security,
compatibility or performance requirement. Do not blanket-ban snapshots or mocks.

Each recommendation reports behavior/risk, evidence and uncertainty, proposed
owner, proposed removals (if any), prerequisites and verification steps. Prioritize
critical protection gaps first; rank consolidation work by expected reduction of
maintenance/runtime burden and substantial removable families relative to effort.
Show estimates as estimates. Test counts and test LOC are supporting observations,
not rewards; merging many cases into one registration is not behavioral deletion.

### 4. Safe consolidation and retirement workflow

1. Inventory a high-leverage family and identify requirements and distinct risks.
2. Separate low-value unique cases, overlapping cases, and essential exceptions.
3. If needed, add a small number of representative vertical journeys through real
   implementations. Do not mock the boundary whose implementation is being claimed.
4. Run retained owners in the relevant scope. Where practical, inject a representative
   fault and verify the expected behavioral failure; restore and rerun successfully.
5. Review deletion: replace overlapping lower-level tests rather than permanently
   stacking them. Temporary overlap during migration is fine. Low-value cases need
   no artificial replacement; important exceptions retain focused protection.
6. Retire compatibility tests/code only after explicit support withdrawal, including
   rollout, migration or recovery obligations that still apply.
7. Run the relevant suites and inspect behavior/risk evidence, failures, runtime,
   flakiness and coverage deltas. Explain lost coverage; neither zero loss nor a
   lower test count is an acceptance requirement.

Never auto-delete tests, remove production compatibility code, accept a new
baseline, weaken a gate or declare a support requirement withdrawn.

### 5. Fault sensitivity and policy integration

Reuse cargo-mutants observations and add optional targeted fault-evidence links.
Record baseline success, injected defect, affected behavior/scope, executed owner,
expected assertion/observable failure, actual result, and restored-run success.
A build failure or unrelated timeout is not successful defect detection. Preserve
survived, inconclusive, unsupported and not-run outcomes distinctly. Aggregate
mutation results cannot identify a detecting owner without scoped execution.

Run experiments only with explicit opt-in in isolated disposable fixtures/worktrees,
with time/resource limits and no live destructive operation or external side effect.
Do not add mutation campaigns to ordinary measurement by default.

Start all value/overlap inferences as advisory. Later opt-in blocking rules may
validate explicit obligations (e.g. a declared critical owner must resolve and run
successfully in its required scope). Missing evidence must fail as unavailable,
not masquerade as a proven defect. Do not offer a “low-value tests must be deleted”
gate. Register per-rule applicability, prerequisites, limitations and cases before
allowing policy enforcement; preserve CLI/JSON/SARIF agreement and waiver semantics.

Keep coverage and CRAP as diagnostic evidence. First update interpretation and
advice without changing their formulas. Subsequently evaluate a new risk-model
version that stops rewarding test quantity and treats risk ownership/protection
separately from execution reach. Do not assume manifest declarations justify a
lower risk score. Calibrate before changing defaults; retain version-compatible
baseline rules. Any intentional coverage-gate waiver or rebaseline remains a
reviewed, visible decision under the existing policy, not a side effect of the
manifest.

## Delivery sequence

1. **Policy contract and advice (small first delivery).** Publish the normative
   policy and interpretation limits; update `docs/quality-model.md`, catalog wording
   and review guidance. Specify representative rule cases and manifest semantics.
   Leave current metrics/gates intact.
2. **MVP: declared ownership review.** Add optional config/schema and manifest
   validation; join existing test IDs/statuses into `test-quality`; report unresolved
   owners, missing execution and distinct-risk review candidates. Update
   `src/config.rs`, `src/producers/test_quality.rs`, `src/contracts*`, `src/catalog.rs`
   and freshness handling. No automatic semantic classification or deletion.
3. **Evidence-backed consolidation.** Add scoped reviewed dispositions, candidate
   ranking and links to existing clone/coverage evidence. Integrate with
   `src/review.rs`, shared finding projection, `src/policy*`, `src/sarif.rs` and
   `contracts/blocking-rules.json` as applicable. Preserve incomplete scope.
4. **Sensitivity and optional enforcement.** Connect fresh targeted mutation/fault
   evidence and required owner scopes. Reuse `src/producers/practices/mutation.rs`;
   introduce blockers only for explicit, evaluable requirements after calibration.
5. **Self-pilot and model decision.** Apply to one substantial overlapping RQLens
   test family, preserve essential trust/regression protections, then trial an
   external Rust project. Record real behavioral deletions separately from grouping
   registrations. Review false positives and maintainer effort before expanding
   heuristics or proposing a versioned risk-model change.

## Acceptance and validation

Use this policy to test the feature itself: one primary CLI-to-artifact/reporter
journey for the public workflow, focused contract cases for manifest/evidence
validation, and local tests only for distinct resolver/ranking/boundary risks.
Avoid duplicating the same assertions at every layer.

Representative acceptance scenarios:

- A real vertical owner protects a critical journey; its overlapping local family
  is a review candidate, while a deterministic concurrency or algorithm test stays.
- A double-backed journey is credited for orchestration, not the replaced storage
  or service implementation; a simulated failure may still test real recovery.
- A unique incidental assertion may be removed without replacement; a unique
  security/data-integrity regression and a material matrix combination remain.
- An old compatibility test is retained pending a support decision; explicit
  withdrawal enables a retirement proposal, never automatic production deletion.
- Missing/stale/ignored/ambiguous owner evidence and unsupported external harnesses
  remain unknown or incomplete. Equal names in different targets never cross-match.
- A helper-based assertion is not declared valueless from syntax counts. Cloned
  tests with distinct risks are not declared redundant from text similarity.
- A controlled fault produces the intended failure and recovery run; unrelated
  command failure and compile-invalid mutants do not count as sensitivity evidence.
- CLI, JSON, SARIF and policy preserve the same finding meaning and evidence scope;
  existing projects without a manifest retain their current behavior.
- Intentional low-value deletion can lower coverage without a false “quality
  improved” claim or silently bypassing the existing coverage-regression gate.

Success means clearer important-risk ownership, demonstrated bounded protection,
and justified removal of unnecessary cases/families with acceptable runtime,
flakiness and maintenance cost—not a larger suite, higher coverage, a fixed
reduction quota, or a new universal score.
