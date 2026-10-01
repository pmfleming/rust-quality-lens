# Closure step 5 — Scoped operational repairs and clean validation

## Implemented

- TS: coordinated native publication locks, old-content backup/rollback, fail-closed recovery markers, baseline publication ordering and fault-injection tests. Clean builds remove obsolete emitted modules; packages no longer ship compiled development tests/scripts. Package smoke now installs and exercises a real external consumer and its project-test subprocess.
- QML: configured qmllint execution/version probes are lazy and memoized, like clones/rules. Explicit unrequested confidence does not force them. Earlier source-only artifacts retain valid version scope after later Qt observation; observed Qt-version mismatches still fail validation.
- Rust: exclusive cooperative batch locks and validated JSON basenames, with competing-writer and escaping-path regressions. Sequential publication is not represented as a complete transaction.
- All: repeated, bounded synthetic resource campaigns and recorded native observations; acceptance gate requires exactly the 25 original ledger IDs and validates even under Python optimization. Candidate freezes remain intact. Recovery and unsupported guarantees are in `../contracts/operational-scope.md`.

## Clean source snapshots

Temporary local clones were committed solely for clean validation; **nothing was pushed**. Native implementation, tests, package manifests and build scripts were compared with the final working trees. Later acceptance notes/results do not alter the analyzed implementation. These temporary commit IDs are evidence anchors, not commits on the delivery branches.

| Lens | Clean source revision | Result |
|---|---|---|
| TS | `26a6708f4807ad0fcbedb2c51f4c8463835b77f2` | Node 24.18 Linux: 66 tests; full `npm run ci`; actual installed-package/project-test smoke; zero `npm audit` vulnerabilities |
| QML | `d8ed3b743a572299d4bb9ce27d5acfc327dadde2` | Pinned Node 24.15 / Qt 6.11 Linux: 112 tests; live qmllint oracle; Qt/CMake integration including intentional failures; installed CMake package smoke |
| Rust | `c4a27d87ae541401b03a10ad69e5eade6fc90fd5` | Rust 1.95 Linux: 171 normal workspace tests plus the normally ignored live macro test; all 17 workflow commands below passed |

Rust commands: formatting, bundled-helper parity, workspace/all-target check, strict Clippy, workspace tests, live macro test, warning-free docs, RustSec audit, cargo-deny, native verify, external-consumer package smoke, catalog, full coverage-backed measurement, self-metric gate, SARIF export, configured policy gate, and repeated resource profile. Policy invocation: `cargo run --locked --bin rqlens -- check --fail-on partial --fail-on test-failure --fail-on practice-failure --fail-on reliability-finding`. Informational/advisory findings are not claimed absent. Self metrics retain hotspot 69.55 <= 70, cyclomatic 20 <= 20, cognitive 11 <= 11; source lines 21,553 <= the explicitly reviewed 22,500 scope ceiling. Other budgets are unchanged.

## Repeated resource evidence

Run `python3 acceptance/profile.py` after building/prewarming the native tools. Ceilings in `resource-budgets.json` were fixed before this campaign. Raw repeated observations are in `resource-results.json`.

| Lens / scoped fixture | Samples | Maximum sampled ms | Peak RSS KiB | Ceiling ms / KiB |
|---|---:|---:|---:|---:|
| TS synthetic 10/50/100 files, fresh/reused context | 18 | 5,028 | 440,372 | 30,000 / 786,432 |
| QML clone fixtures 1k/2k/4k/8k lines, first/repeat | 24 | 48 | 135,608 | 2,000 / 262,144 |
| Rust prebuilt source-only 10/50/100 files, first/repeat processes | 18 | 1,001 | 54,396 | 5,000 / 131,072 |

These are different workloads, **not cross-lens rankings**. RSS peaks are cumulative within the native process/child accounting scope; OS caches may be warm. No universal semantic workload or hard analyzer memory bound is certified.

## Failures investigated, not hidden

The first QML run found a test expecting eager execution; it now explicitly requests qmllint. A diagnostic-before-timeout assertion also failed under oversubscribed parallel validation; no timeout threshold was raised, and the isolated and clean full QML campaigns passed. The new TS benchmark launcher initially referenced an uninstalled runner; it now uses the existing compiled benchmark. The installed-package test initially used an incorrect task name; the registered native task is now exercised. Clean/working package comparison exposed nine obsolete emitted files, prompting clean builds and runtime-only packaging. Final campaigns above were rerun after the relevant fixes.

## Acceptance decision

Four criteria (T1, E1, E5, P1) have scoped implementation/test evidence. **21 of 25 required criteria remain unresolved per repository.** The release gate remains nonzero. Important remaining work: exhaustive rule applicability/negative/unavailable coverage; whole-run determinism and reporter/reference consistency; bounded semantic boundary matrices; independent calibration and intent review; broader typed/permission/cancellation/generation guarantees; and unexecuted platform/minimum-version profiles. Existing partial evidence does not certify these whole criteria.

No push is authorized by these results. Re-run the release gate only after every criterion is substantively closed (or an explicitly agreed scope decision is recorded), then complete the declared platform matrix. Authentication alone does not satisfy acceptance.

Full local logs: `/tmp/{ts,qml,rust}-closure-clean.log`; Rust command statuses: `/tmp/rust-closure-clean-status.tsv`. Clean validation clones: `/tmp/lens-acceptance-clean-Ga47cu/`. See each repository's `criteria.json` and `validation.md` for durable reproduction commands and regression paths.
