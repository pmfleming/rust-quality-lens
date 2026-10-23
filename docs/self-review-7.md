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

## 3. Compiler capture validation

**Necessity:** `Capture::validate` was a production hotspot at **80.86**, with
cyclomatic complexity 12, cognitive complexity 8 and function effort 106,010.
Its single body mixed capture identity, module-tree integrity, declaration
ownership, reference targets and traversal coverage.

Separated module-tree, declaration and reference validation while preserving
validation order and error diagnostics. Origin validation is now shared directly:
references no longer allocate a one-element identity set for the dummy name
`reference` merely to validate their source origin.

Validation:
- Expanded the existing local ownership-contract test before changing production
  code. **22 invalid capture cases have identical decisions and diagnostics** on
  old and new implementations: identity, exclusions, roots, cycles, dangling
  parents, duplicates, declaration ownership, origins and reference targets.
- Valid one-module, multi-level and reverse-ordered trees pass both implementations.
  Existing unknown-target/incomplete-category abstention coverage remains.
  Logs: `step3-before-contract.log`, `step3-after-contract.log`.
- `Capture::validate` hotspot **80.86→20.78**, cyclomatic **12→5**, cognitive **8→1**.
  The largest extracted validation method is **30.80** (`validate_module_tree`),
  not an unreported relocated hotspot. Helpers remain in the measured scope.
- This is a behavior-preserving refactor, not a claim that the old validator
  accepted an invalid capture. The bounded differential cases are not a proof
  over all possible inputs. Per-function complexity drops are not a claim that
  summing cyclomatic complexity across more functions must drop.
- Workspace tests and strict Clippy pass. Self `measure all` succeeds; line
  coverage changes **90.24%→90.32%**, with changed denominators. Stage artifacts:
  `step3/`. Regression cases extend an existing test rather than stack new suites.

## 4. Independent production-workspace pilot

Project: a disposable clone of `daemon-framework` at
`7e4ec1d8b70f0adb6d2458eb89795c5c2903fdd2`. The original checkout remains clean.
The declarations in `acceptance/production-pilot-testing.toml` are an assistant's
bounded review, **not upstream maintainer approval**. The companion
`acceptance/production-pilot.toml` retains required-owner enforcement.

Two existing owners were reviewed: the local decimal compatibility/input-bounds
contract and the output actor's routing/state/serialization journey. The latter
uses an in-memory writer; it does not certify the replaced transport or frontend.
No upstream test was removed, added permanently, or promoted to end-to-end status.

### Actual observations, not an exit-zero adequacy claim

- An initial offline run could not complete Cargo metadata because `hermit-abi
  v0.5.3` was not cached. A bounded `cargo fetch --locked` succeeded; subsequent
  offline measurements have complete fingerprints. This is environment
  preparation, not a completeness exception.
- `verify` and `measure all` exit 0. Native listing and discovery each count 57
  tests; execution records 56 passes and one ignored test. Equal counts alone do
  not prove one-to-one identity or inventory completeness.
- Semantic resolution recovers **399/400 references (99.75%)**. The remaining
  `cli::Cli::parse` reference comes from a derived parser, with macro execution
  disabled. No edge was guessed to reach 100%.
- Coverage is **3,893/4,891 lines (79.60%)**. A separate native LLVM export agrees
  with the total and the selected file projections: `decimal.rs` **43/43**, and
  `output_actor.rs` **324/344**. Their module IDs identify the correct Cargo
  packages/targets. This checks projection, not an independent coverage engine,
  and establishes no per-test coverage attribution.
- The initial declarations mistakenly used full native libtest names. They were
  corrected to the documented exact **catalog** selector values. Both owners now
  resolve, but execution attribution remains **unknown**, not an observed pass.
  `declare_methods!` in `contract.rs` leaves authored-source inventory partial.
  The actual native tests pass, but the correctness artifact remains partial.
- Required-owner `check` still exits **1**, with evaluation **incomplete**. The
  stricter partial/test-failure check also exits 1; partial required evidence is
  not an observed runtime test failure. No gates or exclusions were changed.

### Representative sensitivity and concrete test value

All experiments ran in the clone. All 50 original Rust files were checked against
saved SHA-256 digests and restored byte-for-byte; `git diff` is empty.

1. Invert `if tie` to `if !tie` in `fixed_decimal`. The real existing owner runs
   exactly one test: baseline **0**, injected **101**, restored **0**. This is a
   runtime assertion failure (`Some("1.2")` versus `Some("1.3")`), not a compile
   failure. Logs: `pilot/fault-{baseline,injected,restored}.log`.
2. Relax `decimals > 9` to `decimals > 10`. The same existing owner still passes:
   this fault **survives despite 100% reported file line coverage**. Adding just
   `assert_eq!(fixed_decimal(1.0, 10), None)` temporarily to that same owner detects
   it at runtime; restoring the guard makes the augmented owner pass. Both the
   fault and temporary assertion were then removed, and the original test passes.
   Logs: `pilot/precision-{survived,probe-red,probe-green,restored}.log`.

This supports a specific upstream review recommendation: protect the existing
precision boundary with one assertion in its existing owner, not another test
layer. It does not establish a mutation score or comprehensive adequacy. These
are raw experimental observations, not imported/certified fault-report authority.
No fault was tried against the routing owner.

### Pilot-driven correction and remaining defects

The old owner-review advice told users to rerun tests even when the suite had
passed and inventory was the obstacle. Partial execution evidence now directs
users to `correctness_review.json` limitations and explicitly distinguishes this
from an observed owner failure. The existing native ownership journey reproduces
this with a real item macro and passing test execution: **red 101 → green 0**
(`step4-advice-{red,green}.log`). Statuses and blocking requirements are unchanged.
The live pilot produces the new advice after fresh measurement.

The pilot also exposed follow-up work, **not silently corrected or accepted as
successful behavior**:

- A source-local catalog name is inserted into a suggested native `--exact`
  command. For the decimal owner this exits 0 with **zero tests**, while the full
  native module path runs one. The selector contract is documented; the generated
  command is defective. Propagate authoritative target/module identity and verify
  nonzero scoped execution rather than weakening matching or trusting exit 0.
  Reproducer: `pilot/catalog-command.log` versus `pilot/fault-baseline.log`.
- Workspace `crates/shelllist-local-build/tests/cli.rs` tests are labeled `inline`
  rather than integration tests in the catalog summary. Native Cargo identifies
  their integration target. Do not use this classification to recommend deletion.
- Common item macros prevent owner certification through the current conservative
  source-inventory prerequisite. Complete/appropriately scoped inventory and
  attribution need investigation; rerunning passing tests cannot solve this.

No scoring formula, threshold, deletion heuristic, or gate was calibrated from
this single project. The necessary correction was diagnostic; the precision
experiment demonstrates why broad numeric test-reduction advice would be unsafe.

### Reproduce the bounded pilot

Set `FRAMEWORK` to a local checkout containing the pinned revision. Use a new
clone, not the original checkout. Fetching is only needed for missing cache inputs.

```sh
cargo build --locked --bin rqlens
pilot="$PWD/target/self-review-7/reproduction/project"
git clone --no-hardlinks "$FRAMEWORK" "$pilot"
git -C "$pilot" checkout --detach 7e4ec1d8b70f0adb6d2458eb89795c5c2903fdd2
cp acceptance/production-pilot.toml "$pilot/.rqlens-pilot.toml"
cp acceptance/production-pilot-testing.toml "$pilot/.rqlens-testing-pilot.toml"
(cd "$pilot" && cargo fetch --locked)
CARGO_NET_OFFLINE=true target/debug/rqlens verify --config "$pilot/.rqlens-pilot.toml"
CARGO_NET_OFFLINE=true target/debug/rqlens measure all --config "$pilot/.rqlens-pilot.toml"
# Expected incomplete gate, exit 1; do not treat measure's exit 0 as certification.
CARGO_NET_OFFLINE=true target/debug/rqlens check --config "$pilot/.rqlens-pilot.toml"
(cd "$pilot" && cargo test --locked -p shelllist-daemon-core --lib \
  decimal::tests::binary_rounding_matches_existing_decimal_labels -- --exact)
```

## Net self-measurement and limits

| Observation | Start | Final source |
| --- | ---: | ---: |
| Unresolved collected semantic references | 4 | 0 |
| Maximum cognitive / cyclomatic complexity | 11 / 19 | 11 / 19 |
| Maximum hotspot | 95.65 | 95.65 |
| Measured nonblank source lines | 31,847 | 32,143 |
| Covered / executable lines | 19,455 / 21,569 | 19,612 / 21,715 |
| Line coverage | 90.20% | 90.32% |

Recovered dependencies are evaluator corrections, not architectural improvement.
The validator improvement is a local refactor, not a reduction of the global
maximum hotspot. LOC increased; no LOC or test-count reduction is claimed.
The native independent-Cargo regression adds one explicitly ignored registration;
it was also run explicitly. Other new protections extend existing primary tests.

Self required-owner enforcement passes completely. Strict self policy now fails
only `partial`, not `configured-rules`; remaining macro/type limitations are not
waived. Workspace tests, strict Clippy, formatting and native regression runs pass.
No independent JSON Schema validation or live pinned rustc-private driver campaign
is claimed. Durable measurements are in `acceptance/self-review-7-results.json`;
raw evidence is local under `target/self-review-7/`. Git-bound artifacts must be
rebuilt after the fourth commit.
