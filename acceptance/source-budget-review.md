# Source-size budget decision — closure step 3

The old 17,289-line ceiling is not a valid baseline for the checkout at the start of this work. To distinguish pre-existing drift from this upgrade, the current analyzer was run against an untouched `git archive 64a587d` in a temporary directory, and against the current source tree, using each checkout's `rqlens.toml` and the same Rust 1.95 environment.

| Scope | Source nonblank lines | Maximum function hotspot |
|---|---:|---:|
| Pre-upgrade commit 64a587d | 20,315 | 69.61 |
| Closure step 3 working tree | 21,470 | 69.55 |

3,026 lines of the apparent overrun predate these upgrades. The measured upgrade delta is 1,155 lines (5.69%), not the full 4,181-line gap against the stale ceiling. The largest additions implement wire/capability validation, source-stable identities, external-report validation, review comparison, uncertainty-aware performance evidence and their tests. Existing producer payloads have been made typed, redundant JSON/vector cloning removed, and static SARIF test data moved out of executable setup code. No behavior or tests were deleted to reduce the metric.

Decision: set the **project scope-growth budget to 22,500 source nonblank lines** for this acceptance closure. This allows the measured 21,470 plus 1,030 lines for the remaining operational/calibration regressions. It is a deliberately adopted scope ceiling, not a correctness gate or a calibrated definition of good code. Exceeding it requires a new documented review; it is not automatically updated from measurements.

All other self budgets remain unchanged: hotspot 70, cyclomatic 20, cognitive 11, AST clones 0, token clones 216, duplication 16.14%, map locality 97. Current maximums are 69.55, 20 and 11 respectively. Refactoring separated evidence acquisition from diff presentation, isolated measurement uncertainty validation, removed redundant validation/serialization, and simplified test setup rather than merely moving arbitrary lines into helpers.

Reproduction: archive 64a587d outside the checkout; run `target/debug/rqlens measure hotspots --config <archive>/rqlens.toml` inside the native Nix shell. Sum `sloc` only for module records in the resulting hotspots artifact. Run all current self producers followed by `bash scripts/check-self-metrics.sh` for the current gate. These observations are scoped to this backend/toolchain, not a cross-language size comparison.
