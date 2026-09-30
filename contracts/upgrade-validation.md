# Upgrade validation and remaining release work

The five implementation iterations correspond to review phases 0–4. They are not a claim that every broad acceptance criterion is satisfied.

## Verified
- TS: 60 tests; full CI schema/rule/format checks, performance budget and package smoke.
- QML: 109 tests; clean-package smoke without optional parsers, including conservative unsupported-evidence behavior.
- Rust: 166 normal workspace tests, plus the normally ignored live rust-analyzer macro test; strict Clippy, formatting, docs, bundled-helper parity and external-consumer package smoke.
- Shared capability and calibration conformance files are byte-identical across repositories.

## Profiling observations (Linux; single samples, not calibration)

| Workload | First/cold | Repeat/warm | Cumulative process peak RSS |
|---|---:|---:|---:|
| TS 100 generated files, all measurements except project tests, same context | 4681 ms | 295 ms | 441192 KiB |
| QML two 8000-line clone candidates, no context cache | 61 ms | 68 ms | 130528 KiB |
| Rust 100 generated files, source-only hotspots, separate CLI processes | 499 ms | 505 ms | 54176 KiB (children) |

These are different workloads, **not a language/lens speed ranking**. Binaries/helpers were prebuilt; OS/JIT caches may be warm. RSS peaks are cumulative and cannot attribute all memory to the last sample. Reproduce with TS `npm run bench`, QML `npm run benchmark:clones`, and Rust `python3 scripts/benchmark-measure.py` after building. QML reported complete clone coverage and zero omitted groups/windows in these samples. Existing bounded-work regression tests remain authoritative.

## Known unmet checks
- Live Qt oracle/CMake integration could not start: required Nix Qt/CMake dependency builds were unavailable offline. The clean Node-only QML package campaign passed.
- Rust's deliberately adopted self-metric budgets still fail: maximum function hotspot 98.95 versus 70, cognitive complexity 12 versus 11, and source lines 21192 versus 17289. Cyclomatic maximum is 20 (budget 20). The hotspot maximum is a SARIF integration test; production policy/review coordinators also exceed the hotspot budget. Thresholds were **not raised to hide this**. The obsolete positive-isolation leverage gate was replaced by the observed-reuse equation invariant; other gates are unchanged. This means the full Rust CI workflow is not certified green.
- TS dependency installation reported three high-severity advisories; dependency upgrades were not mixed into this work.

## Remaining plan acceptance
- Complete every blocking rule's applicability/negative corpus and align all reporter/policy semantics; the shared contract currently standardizes capabilities, not every language-native finding payload.
- Independent held-out ranking validation and human-reviewed behavior-preserving refactor pairs; regression partitions alone do not establish useful ranking.
- Fine-grained TS/Rust rule applicability, TS source-map/branch execution coverage, complete QML JS/C++ boundary resolution, and automatic Rust Git-base analysis.
- QML clone/rule computation is lazy; source discovery, type resolution and configured qmllint remain eager. Full optional-work laziness, cancellation and comprehensive resource budgets are not certified.
- Broader typed producer/finding migration remains incremental. No new editor/agent server was added before those contracts stabilize.
- Multi-file publication remains staged and sequential, not a universally atomic transaction.

## Compatibility notes
- TS measurement producers return the same enriched artifact they publish. Internal `writeArtifact` now returns enriched data rather than a path. Required compiler/test failures remain failures even if their findings are baselined or suppressed; the verification-policy identity is versioned.
- QML focused artifacts explicitly list unrequested clone/rule analysis; `confidence(context)` still requests full static confidence, while measurement reporting uses requested scope. Removed forwarding files are not shipped from stale build output.
- Rust type-health wire fields remain flat and unchanged while native sorting/scoring uses typed records. Cohesion declaration counts are explicitly syntactic proxies, not inferred responsibilities. Leverage v2 is observed reuse; old architecture pressure is separately labeled and advisory.
