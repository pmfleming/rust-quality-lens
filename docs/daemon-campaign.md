# Daemon and RQLens campaign checkpoints

These are intermediate checkpoints, **not completion of the complexity/LOC/architecture campaign**.
All changes are local Git commits; nothing was pushed remotely.

## Latest local review correction (2026-10-07)

This is a review of saved artifacts, not a new execution or a replacement for the
historical checkpoints below. Paths are relative to the sibling daemon checkout.

| Daemon | Earlier limitation | Latest saved evidence and comparison scope |
|---|---|---|
| app | `usage_fields!` and a block-local request type made source analysis partial. | `target/five-improvements/before/` and `pre-inventory/` retain that limitation; `after/` reports complete locality/leverage source evidence. Five explicit model structs and module-scoped `OperationStatusParams` improve visibility, not runtime type counts. Type-health remains partial. See `quality/five-improvements.md`, section 5. |
| nm | Architecture evidence is partial because of generated code. | `target/quality-next-after/{locality,leverage}_metrics.json` records 39 unresolved references out of 2,012 (98.06% resolved), generated configuration via `src/lib.rs:36`, and unexpanded `string_identity!`, `serialize_via_ref!`, and `print_api_data_fns!`. Resolution of collected references is not exhaustive extraction. |
| bar | Two environment-dependent fixtures were skipped. | `target/refactor-review/tests-before.log` records the private PipeWire and native idle fixtures as ignored. `tests-after.log` records both passing: 191 unit tests and two integration tests, none ignored. This changes executed coverage, not the number of authored tests. |

The app's five newly visible type declarations are **not** an architectural
regression or five newly introduced runtime types. Source-scope completeness does
not imply complete compiled type/impl totals, body coverage, or a passing global
partial-evidence gate. Compiler inventory is an opt-in type-count backend, not an
architecture or body-analysis replacement.

The bar tests are explicitly `#[ignore]`: a default Cargo run omits them even if
the dependencies are installed. Full-fixture runs require `--include-ignored`,
PipeWire, and the patched Hypridle executable (`HYPRIDLE_TEST_BIN`), supplied by
the existing Nix environment. Both fixtures use private mock services. A fair
before/after test comparison must select the same fixtures; no retrospective
baseline pass is inferred from the successful after-run.

For future comparisons, capture each side into a fresh directory with the same
frozen analyzer/helper, features, test selection, and declared generator inputs.
Do not assemble a campaign from `target/analysis` or `target/quality-current` by
filename alone: app's latter directory contained a September `architecture_rules`
artifact alongside October measurements. Check fingerprints and per-producer
scope/status, and retain unresolved references, ignored tests, and unsupported
patterns rather than interpreting any of them as zero findings.

### Implemented follow-up

- nm `2cc26e5` enables trusted rust-analyzer build/macro execution and fingerprints
  the Wi-Fi CSV plus the three configuration files read by `build.rs`. The opt-in
  does not authorize arbitrary untrusted projects or certify generated inventory.
- bar `1debec3` supplies `tools/quality-fixtures.sh` for local/CI test and coverage
  runs. It checks prerequisites, selects both ignored fixtures, rejects missing
  fixture passes/nonzero ignored counts, and refuses output-directory reuse.
  Six mocked runner regressions and live test/coverage invocations passed. Other
  agents' daemon edits were preserved; the executions are not a frozen
  before/after performance or coverage comparison.
- RQLens distinguishes authored inventory from collected-reference resolution,
  reports unresolved counts by status, and no longer calls unindexed project or
  Cargo build-output definitions external. A local nm probe in
  `target/generated-owner-probe/artifacts/locality_metrics.json` retained all 39
  generated definitions as unknown owners, zero external definitions, nine
  unsupported patterns, and a complete input fingerprint. This is a correction
  to analyzer classification, not a regression in daemon architecture.

Validation: 172 normal Rust tests, two explicit live semantic tests (including
relocated `OUT_DIR` navigation and cold/warm cache equivalence), strict Clippy,
formatting, and bundled-helper consistency passed. Two other opt-in integration
checks (compiler inventory and source expansion) were not run in this follow-up.
Raw RQLens logs are `target/daemon-scope-{tests,live-tests,clippy}.log`; bar runner
logs are in its fresh `target/quality-fixtures.*` directories.

Subsequent nm repeat attempts in `target/generated-owner-probe/final-*.log` were
rejected because source inputs changed during measurement while that daemon was
being edited concurrently. The publication guard was **not** bypassed. Those
attempts are not repeatability evidence; the isolated live regression supplies
cache-repeat evidence, and the saved successful daemon probe remains scoped to
its recorded fingerprint. Macro-body extraction and block-local type ownership
remain partial; no strict gate or scoring threshold was relaxed.

## Original daemon checkpoint (v4)

The original daemon captures are retained under `target/daemon-campaign/<daemon>/`:

- `v4-baseline/` and `v4-final/`, except clip's baseline is `v4-baseline-retry/`.
- `v4-comparison.json` contains comparisons, confidence warnings, and verification results.
- Captures retain the executable, generated TOML policy, raw analysis, command logs,
  Git provenance, and derived summaries. These large artifacts are Git-ignored.

All ten v4 captures used the same executable SHA-256:

```text
a3fbf4aadd55d92266c7e350fa51ca2aa3d760fdd48148f5c30ebe0ea2974b48
```

Every artifact in that checkpoint uses `fnv1a64-quality-inputs-v4`, including mutable
path dependencies in its fingerprint. Framework revision `47d5a65` was unchanged.
App's vendored framework is covered by its own repository revision.
Baselines ran in detached worktrees with sibling dependency paths preserved;
final captures ran in the original repositories. Each pair used the same daemon
Nix shell, configuration, and locked verification. Worktree relocation is permitted
only within the same Git repository; other policy changes are rejected.

**Earlier directories are superseded.** Two app captures leaked coverage profiles;
its isolated subprocess fixture now preserves `LLVM_PROFILE_FILE`. The original
campaign also used a stale executable with v3 fingerprints despite the v4 source
fix. Both sides of every comparison were subsequently recaptured with v4, not
compared across tool versions. The failed first clip v4 baseline lacked its
configured sibling helper-manifest path; the retry restored that layout.

These are historical v4 results, not comparisons against the newer helper-aware
v5 instrument described below. The current campaign checker requires v5 evidence;
remeasure both revisions before extending these comparisons.

## Original daemon results

Complexity below covers `src/` functions excluding `tests` and `*_tests` module/file
names. This is a naming heuristic, **not cfg-aware production classification**.
LOC is nonblank `src/` lines, including tests. Clone counts are candidates touching
`src/`, including tests; structurally similar but unrelated lookup tables were
not merged merely to reduce a score.

| Daemon | Baseline → final commit | Cognitive sum | Cognitive max | Cyclomatic sum | Cyclomatic max | LOC delta |
|---|---|---:|---:|---:|---:|---:|
| app | `05cdd47` → `2579f67` | 496 → 491 | 31 → 20 | 1047 → 1048 | 21 → 17 | +34 |
| bar | `e754372` → `8c54c4f` | 1159 → 1159 | 31 → 31 | 2322 → 2322 | 43 → 43 | +2 |
| bt | `826e9e2` → `793edd0` | 733 → 732 | 11 → 11 | 1900 → 1900 | 18 → 18 | +4 |
| clip | `8165150` → `0b7830a` | 414 → 399 | 29 → 16 | 1327 → 1325 | 20 → 18 | +54 |
| nm¹ | `51042d6` → `db22675` | 1324 → 1324 | 11 → 11 | 4419 → 4418 | 29 → 29 | +18 |

¹ nm's syntax/graph evidence remains partial because of `include!` module wiring.
Bar's static artifacts are complete, but its overall comparison is provisional
because a verification gate still fails.

- **App:** `28827da` shares cycle-safe, ownership-bounded process traversal and
  avoids repeatedly cloning an application ID for membership insertion.
  `2579f67` separately fixes a pre-existing accounting bug: unobserved helpers
  beneath another application's process could be double charged. The expanded
  regression failed before the fix (four processes charged instead of three).
  Ownership now propagates without overwriting explicitly owned nested subtrees,
  consistently fixing both resource totals and sharing classification.
  `05cdd47` is the preceding coverage-fixture repair, not a production refactor.
- **Bar:** `fdb5d6c` finishes hypridle listener blocks without an `expect` or a
  temporary joined string; malformed/nested listener tests were extended.
  Production panic findings fell **3 → 2**. `8c54c4f` separately upgrades rustls
  **0.23.43 → 0.23.45**, resolving **RUSTSEC-2026-0285**.
- **BT:** `793edd0` shares retained-adapter selection between timeout and explicit
  scan completion. Adapter keys are borrowed while the task lock is held; keys
  are cloned only for adapters actually selected to stop. Existing overlap,
  ownership, cancellation, and failed-stop tests pass.
- **Clip:** `0b7830a` shares KMP transitions between prefix construction and search,
  separating matching from stream validation. Short reads, interrupted reads,
  Unicode expansion, overlapping prefixes, invalid tails, and errors after a
  match are tested. It still validates the entire input before returning success.
- **NM:** `db22675` consolidates D-Bus string-array decoding and avoids cloning
  the variant container. Differential tests compare borrowed decoding with the
  original owned conversion, including wrong types and variant-wrapped elements.
  AST clone groups fell **6 → 5**; token groups **121 → 120** (partial evidence).

Module locality and leverage measurements were unchanged in all five comparisons.
Syntax escape-hatch counts were unchanged; panic findings are a separate metric.
No whole-program dead-code reduction or calibrated effort improvement is claimed.
LOC increased overall, largely for regression coverage and the app correctness
fix. This pass therefore does **not** meet the LOC-reduction goal.

## Original checkpoint validation

All five final repositories passed formatting, locked workspace tests, and
Clippy with warnings denied in their native development environments:

| Daemon | Top-level passing tests | Ignored tests |
|---|---:|---:|
| app | 68 | 3 |
| bar | 116 | 1 |
| bt | 117 | 0 |
| clip | 49 | 0 |
| nm | 110 | 3 |

Ignored system/integration tests were not treated as passing hardware validation.
RQLens verification is conformant for app, bt, clip, and nm, with existing warning
and skipped-check evidence retained. Bar's `cargo audit` now passes, but
`cargo deny check` still rejects:

- unversioned path dependencies in bar and the shared framework;
- `webpki-roots`' **CDLA-Permissive-2.0** license, absent from bar's allowlist.

No license policy, gate, or self-metric budget was weakened.

RQLens passed **145 Rust tests**, **10 campaign Python tests**, strict Clippy,
formatting, rustdoc, bundled-helper consistency, and isolated package smoke tests.
Its pre-existing self-metric gate still fails:
`self-metric regression: function hotspot exceeds 70.0`.

## RQLens improvements made during the campaign

- `45c9a2b`: fingerprint sibling path dependencies and inherited workspace inputs;
  unresolved local dependency graphs cannot produce complete fingerprints.
- `b203f7f`: add capture, summary, and comparison tooling.
- `7d1ba43`: isolate profile output and reject source-unstable comparisons.
- `288df25`: clarify test-name scope, count panic findings separately, preserve
  verification failures, and reject changed measurement policy.
- `363cdf3`: build the default executable using Cargo's reported artifact path,
  freeze it before execution, support baseline worktrees, and reject mixed input
  snapshots. Old/incomplete fingerprints are explicitly provisional.
- `00cade5`: isolate package smoke builds. Cargo's unpacked-package verification
  had been able to replace the checkout executable; the smoke test now verifies
  successfully without changing that executable's hash.

Example for subsequent checkpoints:

```sh
python scripts/daemon-campaign.py capture ../bt-daemon target/daemon-campaign/bt/next
python scripts/daemon-campaign.py compare \
  target/daemon-campaign/bt/same-tool-baseline target/daemon-campaign/bt/next
```

Create `same-tool-baseline` from the desired baseline worktree with the same tool
and helper inputs. If either changes, remeasure both source revisions. Do not
compare an old capture with the newly built tool by assumption.

## Follow-up: dependency policy and RQLens self-hotspots

The wildcard path-dependency failures are resolved separately from refactoring:

- Framework `4a10b4c` versions its internal core dependency.
- Bar `0745b36` versions its path dependencies.
- Framework and bar formatting, locked tests, strict Clippy, and rustdoc passed.
  Bar's dependency bans, sources, and advisories passed. The license allowlist
  was **not** expanded: CDLA-Permissive-2.0 remains the outstanding bar blocker.
- The earlier `version-policy` daemon captures predate v5. They are not evidence
  for comparisons with the newly rebuilt instrument.

RQLens refactors separate policy input loading and report stages, diff dispatch
and escape decoding, command waiting/cleanup/output finalization, configuration
ownership/defaults, and benchmark execution/evaluation. Test fixtures share setup
without dropping assertions; eleven test modules use explicit imports rather than
wildcards. Checkpoints include `e780740`, `a811298`, `3550c23`, `97d99d1`, `cbd921f`,
`f05763b`, `ce0dfb7`, `b7f29ee`, `2ae04a7`, and `855cff1`.

### Instrument corrections and comparable self-evidence

`617fd12` fixes an AST normalization inconsistency: statement macros discarded their
names while expression macros retained them. A regression test distinguishes
`assert!` from `assert_eq!` while preserving identifier normalization and node
counts. The previous self AST clone was a false positive, **not duplicated domain
logic removed by refactoring**.

Further reruns exposed shared Cargo executable-path contamination. `8ae5815`
isolates campaign tool builds; `c9cc4c3` isolates helper builds by canonical manifest
under the helper cache, uses locked helper builds, and fingerprints helper sources,
workspace/dependency inputs, and lockfiles. Helper dependencies are resolved before
measurement snapshots. V5 also records a separate `helper_digest`; campaign
comparisons reject changed helper inputs even if the frozen main executable matches.
Tests cover same-named helpers, preserved-timestamp helper edits, missing helper
metadata, and comparison rejection.

The authoritative self comparison is **`6eb86d9` → `c9cc4c3`**:

- `target/self-campaign/v5-baseline/`, `v5-final/`, and `v5-comparison.json`.
- Same executable SHA-256:
  `9ee102534f8958c46aefac05a17374b1b1140bcc0c8575c0a6c9d587f429921d`.
- Complete `fnv1a64-quality-inputs-v5` fingerprints; identical helper digest
  `7eca41baf7275cbe`; stable source inputs and no comparison warnings.
- Both revisions were remeasured after the corrections. Earlier self captures,
  including `corrected-*` and `validated-*`, are superseded; hotspot previews are
  diagnostic only. An abandoned baseline capture detected changing inputs and
  did not publish its measurement batch.

Unlike the daemon table, this table covers **all configured self source roots and
functions**, including tests: `src`, `rust_helpers/src`, and `tests/cli.rs`.

| Metric | Baseline → final | Unchanged budget |
|---|---:|---:|
| Maximum function hotspot | 226.24 → **69.61** | ≤70 |
| Maximum cognitive complexity | 34 → **11** | ≤11 |
| Maximum cyclomatic complexity | 26 → **20** | ≤20 |
| Syntax escape records | 11 → **0** | 0 |
| AST clone records, corrected detector | 0 → **0** | 0 |
| Token clone records | 131 → **112** | ≤216 |
| Duplicated-line percentage | 9.59 → **8.35%** | ≤16.14% |
| Map locality / leverage | 97 / 50 → **97 / 50** | ≥97 / ≥50 |
| Nonblank source lines | 19,915 → **20,003** | **≤17,289: failing** |

Cognitive complexity sum fell 1,266 → 1,212; cyclomatic sum increased
2,795 → 2,829. Source LOC increased by 88, including new provenance regression
coverage. Lower maxima and fewer token clones therefore do **not** establish net
LOC, total cyclomatic, locality, leverage, or calibrated effort improvement.

Validation passed: **152 Rust tests**, **10 campaign Python tests**, formatting,
strict Clippy, rustdoc, bundled-helper consistency, and isolated package smoke tests.
Both self captures are verification-conformant (17 passed, 11 skipped); skipped
checks are not passes. Running the unchanged self-metric script against the final
capture stops at `source nonblank line count exceeds 17289`, not the former hotspot
failure. No gate, license policy, or baseline threshold was relaxed.

## Next pass

1. Resolve bar's remaining license-policy decision explicitly.
2. Add cfg-aware function scope and address nm's `include!` evidence gaps.
3. Reduce RQLens source LOC while retaining the now-passing complexity limits and
   regression coverage; its existing LOC budget is still exceeded by 2,714 lines.
4. Target genuine repeated domain workflows and measured module coupling across
   the daemons, seeking net production LOC and locality/leverage improvements.
   Do not merge unrelated AST lookalikes or treat partial evidence as complete.
