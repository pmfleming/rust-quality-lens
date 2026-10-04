# Test reduction: 231 → 155 registrations

This follows the **uncommitted self-refactoring-2 working tree**, not pristine
`13889de`. Its existing production changes are preserved. This pass changes tests,
mirrored helper test sources, and acceptance tooling/documentation—not production
rules, scoring, configuration, scope or budgets. Nothing was committed or pushed.

## Count and scope

| Discoverable suite | Before | After |
|---|---:|---:|
| Rust workspace, including two ignored live tests | 196 | 135 |
| Python acceptance | 25 | 14 |
| Python scripts | 10 | 6 |
| **Total** | **231** | **155** |

`round(231 × 0.67) = 155`: **76 fewer registrations, a 32.90% reduction**.
Counts use the pinned Linux toolchain, `cargo test --workspace --locked -- --list`,
and unittest discovery. Fixture source inputs and native campaign scenarios are
not additional registered tests. Neither live test was disabled or removed. This
is a one-time target, **not a permanent quota constraining future essential tests**.

Most of the reduction is **consolidation, not deletion of behavioral cases**.
Tables and shared assertion helpers preserve distinct failures inside focused
contract tests; they no longer need a separate registration and repeated setup.
The machine result retains both complete inventories, not just totals:
`acceptance/test-reduction-results.json`.

## Selection and consolidation

Removed outright:

- The duplicate `computes_full_score_when_inputs_are_known` risk-model test;
  explained complete-score assertions remain in the retained confidence test.
- Two repository/environment-pinned tests: current workspace metadata and current
  toolchain compatibility. Fixture-driven direct/inherited metadata checks remain;
  explicit passed/failed/skipped/unparseable toolchain cases replace incidental
  dependence on this checkout's declared version.
- The Python check of the source-reporter fixture generator's literal output.
  Native source/imported agreement and rejection campaigns still exercise that
  evidence through real consumers.

Representative consolidations:

- **CLI 24→15:** six mini-project checks share one `measure all` result. Golden
  attribution/snapshots, missing/complete architecture evidence, and outcomes/
  performance scenarios share their relevant fixture setup and keep assertions.
- **Command runner 9→5:** status cases share setup; output limits and checked-output
  failures are grouped. Timeout and parent-exit descendant cleanup remain separate
  table rows, not discarded failure modes.
- **Configuration 8→4:** defaults/arguments, waiver exclusions, TOML/unknown fields,
  and relative-root/metadata discovery pairs retain their respective cases.
- **Semantic unit tests 14→9:** generated origins, telemetry omission, URI/location
  coordinates and loading behavior remain exercised. Both live fixtures remain.
- **Helper tests 19→11:** metric shapes and native syn/tree-sitter comparisons are
  table-driven; test scope, safety, opaque macros, attributes and hash/operator
  discrimination still have assertions. Bundled copies remain identical.
- Related validation, fingerprint, publication, coverage/status, performance,
  expansion and diff cases share setup or assertion helpers. Publication rejection
  still preserves prior artifacts; incomplete evidence still cannot become zero.
- Python confidence, inventory, projection, compiler-context and daemon-evidence
  cases use grouped scenarios/subtests. The self-refactor normalization regression
  now lives in `acceptance/test_trust_campaign.py`, not a separate test module.

The nine native trust integration tests and policy/SARIF regression cases remain.
No failure-mode campaign, threshold, feature scope or completeness gate was removed
to satisfy the count. Historical results keep their historical test counts.

## Coverage check and correction

The first post-change LLVM run exposed nine previously covered function spans:
one version-check closure and eight function-risk closures. The former depended
on the environment-pinned test; the latter depended on retained coverage artifacts
in a CLI fixture. Merely accepting the lower count would have hidden that gap.

The retained version test now exercises explicit compatibility outcomes. The CLI
bundle creates fresh, clearly synthetic, source-linked coverage and verifies the
native function-risk projection, full-coverage score and confidence. Mini/golden
analysis directories were archived, then the coverage suite rerun without those
cached outputs. No new registration was needed.

| Unchanged compared source areas | Before covered | After covered | Lost |
|---|---:|---:|---:|
| 1,613 function spans | 1,342 | 1,342 | **0** |
| 22,213 LLVM code regions | 18,695 | 18,696 | **0** |

The comparator groups generic instantiations by source span, verifies equal span
inventories and unchanged saved source prefixes, excludes inline test modules and
test-only files, and includes small test-only fixture helpers outside those modules.
This is **bounded coverage retention**, not exhaustive behavior or branch/MC/DC
certification. Unarchived production files are assumed unchanged during this
test-only pass; saved edited prefixes are checked. LLVM exported no branch/MC/DC
measurements.

Whole-workspace line coverage is 87.60%→87.66%; function coverage is
84.62%→84.33%. Those denominators include changed test code and therefore are not
like-for-like production coverage measures. The unchanged-region comparison above
is the retention check.

## Validation and trade-offs

All **133 normal Rust tests, two live Rust fixtures, 14 acceptance tests and six
script tests pass**. Also passing: strict Clippy, rustdoc, bundled-helper parity,
package smoke, native self verify, unchanged self-metric gates and acceptance
ledger structure. Native campaigns retain all 28 ingestion cases, 19 practice
projection cases, ten malformed diagnostic cases, six source/import agreement
cases and 30 rejection/abstention cases.

Measured source lines decrease **22,457→22,373**, below the unchanged 22,500 budget.
Token-clone groups decrease 72→68; duplicated lines decrease 1,046→1,001.
Cyclomatic sum decreases 3,209→3,177, but **cognitive sum increases 1,383→1,398**
with grouped scenarios/additional checks. Maximum hotspot stays 69.22. Fewer tests
is not a claim that every complexity or maintainability measure improved. Measured
Rust lines do not include the new acceptance documentation/tooling.

Strict self policy still exits 1 on `configured-rules`, `partial`,
`reliability-finding` and `test-failure`, with no stale artifacts and empty actual
test/practice failure lists. Release audit remains blocked. No E2/E3/E4/E6, T4 or
platform criterion is closed; no daemon project was touched.

Raw evidence, starting test-source snapshots, commands' logs and LLVM exports are
retained in `target/test-reduction/`. Recheck the recorded count/coverage comparison:

```sh
python3 acceptance/audit_test_reduction.py \
  --evidence target/test-reduction --output /tmp/test-reduction-results.json
```

Native validation uses `nix develop --offline --no-write-lock-file --command ...`.
Campaign reruns require fresh output directories. The audit does not execute the
native suites itself; their successful execution logs are retained separately.
