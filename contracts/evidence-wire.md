# Cross-lens evidence vocabulary v1

Native envelopes may carry an additive `evidence_contract` array. All consumers validate the array before using its enclosing artifact. Omission preserves legacy native envelopes; null/malformed extensions are rejected. This does not replace native freshness checks or assert that all producers already emit the common vocabulary.

Each entry has `schema_version: 1` and a discriminant `kind`:
- `run`: id; analyzer `{name, version}`; ruleset; config/input fingerprints; declared language profile; producer `{name, version}` array. Strings must be nonempty.
- `finding`: id; namespaced rule_id; semantic subject; run_id; locations `{file, line}`; evidence_kind (diagnostic/test/metric/heuristic/semantic/tool-rule); confidence (high/medium/low); severity (error/warning/note); disposition (block/warn/review/info); message; explicit null or `{reason}` suppression. Lines are positive safe integers.
- `measurement`: name; finite value or explicit null with limitations; unit; scope; model; run_id; explicit null or ordered finite `{lower,upper}` uncertainty; limitations array. Missing observation cannot silently become zero.
- `comparison`: compatible/incompatible/unknown status; current_run; baseline_run or explicit null; reasons. Compatible requires both runs and no incomparability reasons. Other states require reasons. Structural validation does not establish compatibility: native comparison must verify versions, scope and environment first.

Capability records remain specified by `capability-cases.json`. `wire-cases.json` supplies positive cases, missing-field mutations, malformed scopes, absent provenance, invalid intervals and suppressed findings. Native TypeScript discriminated types and Rust serde variants validate the same corpus; the native artifact readers consume their validators.

`reporting-cases.json` checks that impact and disposition remain distinct and waived findings remain visible in SARIF. TS now exports audit SARIF through `audit --format sarif`; QML exports accepted suppressions; Rust policy and export share disposition/waivability interpretation. Operational failures are not ordinary suppressible diagnostics.

Still open: exhaustive rule-family contracts and complete end-to-end reporter equivalence, producer-wide projection to these wire records, and cross-artifact reference integrity. Do not infer these from a passing structural corpus.
