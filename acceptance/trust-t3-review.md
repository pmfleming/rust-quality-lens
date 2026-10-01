# T3 follow-up and T4 disposition

## Result

T3's fail-closed syntax/type-identity and requested architecture-identity checks pass. T2 repeatability still passes. **Overall acceptance does not pass:** T4's local native policy gate fails, and the declared platform matrix remains unexecuted.

Implementation and evidence are committed together on parent `c355400`. The frozen executable is SHA-256 `2bdcef6d8ab0a20e6445aa5febe520b93ef92c49ba6a969c0a862667e7f37d93`. Subsequent report/README edits do not change the analyzed implementation.

## Repairs

- Package/target/module-qualified impl joins no longer guess by suffix. Unresolved aliases and ambiguous definitions are explicit limitations.
- Opaque item, impl, expression and attribute macros, cfg, unsupported owners and incomplete provenance cannot produce a complete type inventory. A compiling fixture proves that statement macros can introduce non-local impls; another proves attribute-generated type omission.
- Unknown authoritative method/impl totals and structural scores are null. Separately named source observations remain available; they are not compiled totals or guaranteed compiled lower bounds.
- Row, envelope, capability, policy and review confidence preserve abstention. Missing helper limitation metadata is rejected rather than interpreted as no limitations.
- Requested but disabled/unavailable architecture resolution makes outer and custom payload confidence partial. Empty reference scope remains distinguishable from unresolved references.

See `../contracts/semantic-abstention.md` for the exact scope and migration contract. This is conservative abstention, not newly implemented compiler expansion.

## Daemon validation

The final `target/trust-t3-confirmed/` campaign ran all 13 focused static producers twice on six daemons and daemon-framework: **182 successful commands and 91 matching artifact comparisons**. Only generation/measurement timestamps were removed; array order, counts, findings, confidence and fingerprints were retained.

| Project | Type rows explicitly partial |
|---|---:|
| app-daemon | 104 |
| bar-daemon | 228 |
| bt-daemon | 118 |
| clip-daemon | 120 |
| nm-daemon | 321 |
| update-daemon | 18 |
| daemon-framework | 69 |

App's five `usage_fields!` types remain unexpanded, but the omission is now explicit and neither the envelope nor capability claims completion. The artifact audit checks null unknown totals/scores, retained observations, nested semantic confidence, capability status and repeatability on both runs.

All seven source inventories match the T2 baseline, including the existing nm-daemon changes. Nix used `--no-write-lock-file`; update-daemon used the available Rust environment. No daemon service, daemon test suite or daemon coverage campaign was run. Partial support is not a daemon defect label. Framework's incomplete dependency provenance remains a limitation.

Durable results: `trust-t3-results.json`. Replay:

```sh
cargo build --locked --bin rqlens
python3 acceptance/trust_campaign.py --output target/trust-new
python3 acceptance/check_trust_t3.py target/trust-new
```

The earlier `trust-t3-final` campaign lacked the final attribute/expression-macro and missing-helper-metadata cases. `trust-t3-verified` was intentionally stopped before further helper edits. Neither is substituted for the final confirmation.

## Native checks and remaining blocker

- 184 normal workspace tests, the live macro test, and eight Python acceptance tests pass.
- The two-package CLI fixture remains complete with method counts 1 and 2. Macro-free controls retain known zero and correctly joined qualified owners.
- 16/17 local Linux Rust 1.95 workflow commands pass, including audit, deny, external package smoke, coverage-backed self-measurement, SARIF and repeated resource checks.
- Self limits remain unchanged: hotspot 69.55, cyclomatic 20, cognitive 11; declared-scope source lines 21,941 <= 22,500.
- The strict native policy exits **1**: required evidence is partial because RQLens itself uses unexpanded visitor macros and other unsupported type scopes. Observed test/practice failure arrays are empty; the policy does not mistake missing certainty for successful verification. Advisory findings/thresholds are not claimed absent.

`trust-t4-results.json` records every native command and the failed policy categories. This was a local working-tree check, not clean CI certification. macOS, Windows and latest-stable profiles were not executed. No gate, budget, source scope or calibration weight was weakened to obtain a pass.

T4 needs genuinely supported cfg/macro/identity evidence (or an explicitly agreed scope decision), then clean native and platform validation. The broader rule/reporter, semantic-profile, calibration and operational criteria remain open. **No push or release is authorized.**
