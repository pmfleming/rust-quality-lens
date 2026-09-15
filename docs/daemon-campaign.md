# Daemon campaign: first validated checkpoint

This is a first pass, **not completion of the complexity/LOC/architecture campaign**.
All changes are local Git commits; nothing was pushed remotely.

## Reproducible evidence

Authoritative captures are under `target/daemon-campaign/<daemon>/`:

- `v4-baseline/` and `v4-final/`, except clip's baseline is `v4-baseline-retry/`.
- `v4-comparison.json` contains comparisons, confidence warnings, and verification results.
- Captures retain the executable, generated TOML policy, raw analysis, command logs,
  Git provenance, and derived summaries. These large artifacts are Git-ignored.

All ten authoritative captures used the same executable SHA-256:

```text
a3fbf4aadd55d92266c7e350fa51ca2aa3d760fdd48148f5c30ebe0ea2974b48
```

Every authoritative artifact uses `fnv1a64-quality-inputs-v4`, including mutable
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

## Results

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

## Validation and remaining gates

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
  target/daemon-campaign/bt/v4-final target/daemon-campaign/bt/next
```

If the lens executable changes, remeasure both source revisions with that same
executable. Do not compare an old capture with the newly built tool by assumption.

## Next pass

1. Resolve bar's dependency-policy issues explicitly, including a license decision;
   coordinate any shared-framework manifest changes across dependent daemons.
2. Add cfg-aware function-scope evidence to RQLens instead of relying on names.
3. Address RQLens's existing self-hotspots without relaxing budgets.
4. Target genuine repeated domain workflows and measured module coupling, seeking
   net production LOC reductions and locality/leverage improvements while keeping
   the new regression tests. Do not merge unrelated AST lookalikes.
