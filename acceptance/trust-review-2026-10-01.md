# Trust acceptance review — 2026-10-01

**Decision: trust acceptance does not pass. T1 has passing scoped evidence; T2 and T3 have reproduced failures; T4 remains blocked.** This is an assessment of RQLens, not a quality verdict on the daemon projects. No production code or acceptance-ledger statuses were changed.

## History and criteria

Reviewed the ten latest commits, through `77bb7b1b4a631cd155bc38c89820c1916fe9511d`:

- `d483b4d` fixes package/target-local type joins and validates SARIF inputs.
- `933db70`–`a222415` add capability/wire evidence, comparison provenance, calibration regressions, typed boundaries and packaging checks.
- `ab213f1` establishes the fail-closed ledger; `9c3ac81` aligns evidence validation/reporting; `f5ed52` addresses self-metric regressions; `e5969c6` freezes refactor candidates.
- `77bb7b1` serializes measurement writers and records scoped native validation. It does not claim full acceptance.

The authoritative criteria are `acceptance/criteria.json`, T1–T4, derived from phase 0 of `../quality-lenses-review-and-upgrade-plan.md` (relative to the repository root).

| Criterion | This review |
| --- | --- |
| T1: reproduced trust defects have positive/negative regressions | **Pass in its declared scope.** Existing regressions pass; a real two-package CLI fixture reports one and two methods respectively, not three for both. All seven projects reject corrupt and deliberately stale SARIF input. |
| T2: repeated analysis is deterministic except timestamps/run identity | **Fail.** Warm map runs reorder graph edges on every daemon and the framework. |
| T3: unresolved semantics cannot report fully evaluated evidence | **Fail.** Macro-generated types disappear without a limitation; unresolved aliased impls become complete zero-method measurements. |
| T4: complete native CI and declared integration profiles pass | **Blocked / not established.** The local checks below pass, but this campaign does not run the entire workflow or the macOS, Windows and latest-stable matrix. |

`python3 acceptance/check.py --release` returned **1**, consistent with the ledger's existing 4/25 passed and 21 unresolved criteria. A green structural gate alone would not prove these substantive claims.

## Campaign

Built the clean reviewed checkout with `cargo build --locked --bin rqlens` and froze the executable. SHA-256:

`b2935adeda735a2270d0c5e7a39df750d0110454f742cd19210b49d6e85dde36`

For each project, retained its existing configuration, changing only project/output paths and resolving an explicitly configured relative helper manifest against its original project. Output was isolated under `target/trust-review/<project>/`; existing daemon analysis directories were not replaced.

Executed twice: `measure type-health`, `measure map`, `measure reliability`, `measure architecture-rules`, `measure hotspots`, `measure clones`, then `sarif`. Afterward, executed two more `measure map` runs with unchanged prerequisite artifacts and warm caches where available. Comparisons remove only `generated_at` and `measured_at`, preserving array order, fingerprints, findings, confidence and other metadata. SHA-256 inventories and Git state were stable during each project's main and warm campaigns.

| Project | Source revision | Type rows | Warm map edges | Different JSON leaves in warm map pair | Normal SARIF |
| --- | --- | ---: | ---: | ---: | --- |
| app-daemon | `5ca88f0` | 104 | 175 | 255 | Exported |
| bar-daemon | `61b8151` | 228 | 325 | 583 | Exported |
| bt-daemon | `9628f76` | 118 | 166 | 291 | Exported |
| clip-daemon | `3e2f319` | 120 | 55 | 90 | Exported |
| nm-daemon | `622ebc7` plus pre-existing working changes | 321 | 443 | 1,063 | Exported |
| update-daemon | `918127d` | 18 | 55 | 88 | Exported |
| daemon-framework | `c13f27d` | 69 | 55 | 126 | Rejected: incomplete fingerprint |

All **84 focused measurement commands** and **14 additional map commands** exited 0. On the six daemons, the five non-map artifacts and SARIF were identical across the main pair after timestamp normalization. Exit 0 is not interpreted as complete evidence or acceptance.

All **14 negative SARIF probes** exited 1: malformed JSON and a deliberately changed fingerprint digest, using isolated evidence copies restored afterward. The framework's normal export also failed closed because Cargo metadata could not resolve its complete project/helper path-dependency graph; this is not counted as a successful normal-export test. Its artifacts explicitly report the missing fingerprint input.

### Harness/environment qualifications

- Used each project's Nix development shell, except `update-daemon`: its flake provides no devShell. Its initial Nix invocation failed; the complete campaign and warm pair then succeeded in the already available Rust 1.95 environment. Both failed setup logs and successful fallback logs are retained.
- `nix develop --offline` unexpectedly added `daemonFramework` entries to five daemon `flake.lock` files before measurement. These exact additions were removed after all runs, and all five locks were verified restored. The measurement inventories therefore describe the temporarily resolved shell state, not untouched clean Git checkouts. Replay scripts now use `--no-write-lock-file`; the originally executed scripts are retained separately.
- All six daemon source trees were left as found: five clean; `nm-daemon` retains its original 19 modified source/test files. The framework is clean. No daemon service was launched or restarted.
- Shared helper builds and unrelated host workloads affected elapsed times. This is not a resource-budget benchmark.
- This is focused trust validation, **not** `measure all`, daemon verification/test execution, coverage certification, or full release CI.

## Reproduced failures

### 1. T2: nondeterministic graph-edge serialization

`src/facts/graph.rs:38–40` stores dependency/containment edges in `HashSet`s. `src/producers/map.rs:174–204` iterates those sets directly into the output array without sorting.

The warm pairs have identical edge multisets, but different edge order. Sorting only the edge arrays makes the entire timestamp-normalized map documents identical on all six daemons. The framework additionally changes three copies of semantic `duration_ms`, because incomplete fingerprints prevent cache reuse. The daemon failures do not depend on treating timing metadata as nondeterminism.

The first pair also changes cache state and available prerequisite artifacts, so those first-pair map differences were **not** used alone to establish this defect. The controlled warm pairs isolate edge ordering.

**Needed:** deterministic edge ordering plus an end-to-end repeated-run regression that preserves array order. If execution telemetry is intentionally variable, declare that separately rather than silently normalizing findings or edges.

### 2. T3: unresolved type/impl semantics are presented as complete

**Real daemon:** `app-daemon/src/model.rs:3–9` defines `usage_fields!`, invoked for `ComputeUsage` (line 11), `StorageUsage` (35), `NetworkUsage` (62), `EnergyUsage` (69), and `ResourceAvailability` (204). None of these five generated types appears in `type_health.json`. Nevertheless, the artifact reports:

```json
{"complete": true, "partial": false, "unsupported_pattern": []}
```

Its `project-inputs` capability is `completed`, with `skipped: 0` and no reasons. The confidence is labelled `syntax_facts`, but no type-health-specific evidence explains that generated types and their method/field measurements were not evaluated. `rust_helpers/src/bin/rust_facts.rs:1021–1059` warns for certain module/test/cfg macros, not this type-generating case.

**Compiling isolation fixture:** `target/trust-review/alias-fixture/src/lib.rs` contains:

```rust
pub mod a { pub struct Widget; }
pub mod b { pub struct Widget; }
pub mod implementation {
    use crate::a::Widget as LocalWidget;
    impl LocalWidget { pub fn answer(&self) -> u8 { 42 } }
}
#[test]
fn behavior() { assert_eq!(a::Widget.answer(), 42); }
```

`cargo test --offline --manifest-path target/trust-review/alias-fixture/Cargo.toml` passes. RQLens reports **zero methods and zero impl blocks for both types**, with complete confidence on the artifact and both rows. `a::Widget` actually has one method.

`src/producers/type_health.rs:79–80` silently discards unresolved impl identities; lines 172–173 convert the absent summary into zero counts. Line 24 takes general syntax confidence, not confidence in the impl join. Package qualification fixes the original cross-package collision, but does not close this semantic-completeness gap. The existing ambiguous-name unit test checks only that no incorrect join occurs, not that the result abstains or becomes partial.

**Needed:** track unresolved/ambiguous/generated type scope through measurement confidence and applicability; distinguish unknown from zero. Resolve aliases correctly or abstain. Add CLI-level negative tests covering confidence as well as counts.

Correct partial reporting also exists: `nm-daemon` reports its `include!` wiring as unsupported and 339 unresolved semantic references; `clip-daemon` reports 258 unresolved semantic references. These positives do not discharge the macro/alias counterexamples.

## Native regression checks

Passed on Linux with Rust/Cargo 1.95:

- `cargo test --workspace --locked`: **171 passed**, one normally ignored.
- `cargo test --locked semantic::macro_tests -- --ignored`: **1 passed**.
- `cargo fmt --all -- --check`.
- `bash scripts/check-bundled-helpers.sh`.
- `cargo check --workspace --all-targets --locked`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked`.
- `python3 -m unittest discover -s acceptance -p 'test_*.py'`: **3 passed**.

The two-package fixture passes Cargo tests and the CLI count check: `trust-a::lib::Widget` has one method/one impl file; `trust-b::lib::Widget` has two methods/one impl file. Its first measurement attempt hit the outer 60-second harness timeout; the recorded retry completed successfully. No timeout was counted as a pass.

## Evidence and replay

Local raw evidence (ignored build output, not committed): `target/trust-review/`.

- `provenance.json`: analyzed revision and frozen binary hash.
- `campaign.executed.py`, `warm_maps.executed.py`: original executed harnesses.
- `campaign.py`, `warm_maps.py`: replay harnesses with non-writing Nix setup and update-daemon fallback.
- `<project>/summary.json`: exact commands, statuses, timings, source inventories and initial comparisons.
- `<project>/warm-map-summary.json`: controlled repeated-map comparisons and edge-multiset checks.
- `<project>/run-{1,2}/`, `warm-{3,4}-map.json`: raw outputs.
- `<project>/{corrupt,stale}.log`: negative SARIF results.
- `alias-fixture/`, `package-fixture/`: source, Cargo test logs and CLI evidence.
- `tests.log`, `macro-tests.log`, `native-status.txt`, other named native logs and `acceptance.log`.

For a fresh local replay, build the same revision, copy the two replay scripts into a new `target/<campaign-name>/` directory, run `campaign.py`, then `warm_maps.py`. Use the original project revisions/configurations for exact comparison; the saved `nm-daemon` source hash identifies its dirty snapshot. No independent labels or human review are asserted by this campaign.
