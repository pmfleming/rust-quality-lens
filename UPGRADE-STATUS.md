# Cross-lens upgrade execution

## Latest test consolidation: 231→155 registrations

The rounded 67% target is met: 135 Rust registrations (including both live fixtures), 14 Python acceptance tests and six script tests. Related scenarios share setup/assertions; redundant and repository-version-pinned checks are removed without dropping native failure-mode campaigns. All retained tests pass, with no previously covered function spans or code regions lost in the unchanged comparison scope. Self gates pass at 22,373 measured source lines; cognitive sum rises to 1,398, an explicit consolidation trade-off. See `docs/test-reduction.md` and `acceptance/test-reduction-results.json`. This is not a permanent test-count cap or a release/trust certification.

## Prior self-review: unchanged frozen evaluator

From `13889de`, measured source lines fall 22,499→22,457, token-clone groups 83→72 and duplication 5.82%→4.66%. Cyclomatic/cognitive sums fall 3,211→3,209 and 1,396→1,383. Correctness locality improves 91→94; current-artifact reuse leverage improves 20→40, while average leverage declines slightly as direct dependencies disappear. Effort is uncalibrated; no exhaustive dead-code claim is made, and detected escape hatches remain zero. See `docs/self-refactoring-2.md` and `acceptance/self-refactoring-2-results.json` for explicit regressions, frozen/candidate parity and 194 Rust/two live/25 Python tests. All unchanged self gates pass. Trust/release status is unchanged.

## Current reporter agreement: source and imported findings

Source and imported projections now share identity, policy-context and waiver interpretation. Original evidence is retained, reliability selection remains unchanged and out-of-scope observations stay visible. Six native agreement fixtures, 30 rejection/abstention cases and twelve exact reporter replays pass; actual self-analysis agrees on 127 source findings, including 125 outside reliability policy selection. Unsafe adapter names and malformed imports cannot replace prior valid reports. See `contracts/source-projection.md` and `acceptance/source-reporters-results.json`. Review/change snapshots and broader E2/E3/E4/E6 scope remain open. Self gates pass at 22,499 source lines; strict policy and release remain blocked.

## Prior reporter agreement: shared practice projection

Policy and SARIF now share nested-diagnostic projection and retain all contributing observations and invocation context. Nineteen native cases verify identity, evidence, disposition, waivers/exclusions and exact input reconstruction; 38 reporter replays match byte-for-byte. Ten malformed-input cases reject without replacing the prior SARIF. The original 28 ingestion cases still pass. See `contracts/practice-projection.md` and `acceptance/reporter-agreement-results.json`. Unknown negative coverage and compatible-engine scope remain explicit; **E2/E3/E4/E6 remain open** outside these bounded cases. Self gates pass at 22,492 source lines with no scope/budget/weight changes.

## Initial evidence contracts: E2/E3 boundary

The catalog now exposes 39 native rule IDs and draft family/gate contracts, explicitly retaining missing per-rule cases. Configured rules abstain on unsupported, disabled or unusable evaluation inputs rather than reporting verified-zero counts. All 28 native ingestion cases and 28 policy replays pass; CLI stdout matches published JSON. The initial probe recorded the E4/E6 nested-diagnostic projection/corroboration gap subsequently repaired for practices above. See `contracts/blocking-rule-coverage.md` and `acceptance/evidence-contracts-results.json`. **E2, E3, E4 and E6 remain open**; no release or T4 closure is claimed.

## Prior refactoring: self-measured quality

Frozen-evaluator measurements reduce token-clone groups 111→88, duplicated lines 1,641→1,382, and measured source lines 22,492→22,462. Cyclomatic/cognitive sums fall 3,223→3,214 and 1,387→1,383; maxima remain 20/11. Shared fixture and current-artifact boundaries improve reuse and selected locality scores, with explicit per-module trade-offs. All 19 artifact schemas and catalog output are unchanged. See `docs/self-refactoring.md` and `acceptance/self-refactoring-results.json` for measurements, repeatability limits and validation. No calibrated effort estimate, T4 closure or release permission is claimed.

## Current research: compiler cfg/target contexts

`acceptance/self_compiler_profile.py` instruments a fresh self-only Cargo check and records actual primary-unit rustc arguments and cfg probes. All 11 expected normal/test contexts compile and probe successfully without source changes. Four new Python regressions cover context coverage, mismatched/failed evidence, probe selection and unchanged delegated compiler arguments. This is a POSIX acceptance experiment, not production cfg evaluation, ownership resolution or T4 certification. See `docs/self-compiler-profile.md` and `acceptance/self-compiler-profile-results.json`.

## Current implementation: bounded generated syntax

Expansion replies now yield context-checked declaration observations with UTF-8 ranges in the returned text. All nine self visitor fragments parse; native validation rejects altered observations and synthetic impl-wrapper escapes. Larger/deeper text is explicitly declined under a conservative parser budget, not counted as empty. No generated observations enter authoritative type-health totals or scores yet.

Validation: 193 normal tests, two live fixtures, ten Python tests, package smoke and unchanged self-metric gates pass (22,492/22,500 source lines). The seven-project campaign passes 196 commands and 98 comparisons; all 978 T3 partial type rows retain abstention and all project sources remain unchanged. Helper serialization refactoring preserves 16 fixture outputs and both read-error/recovery cases. See `acceptance/generated-syntax-results.json`.

**T4 remains failed.** Compiler cfg/target binding, hygiene, owner joins and metric integration are still missing. Strict policy rejects partial evidence; declared clean/platform CI is not certified. This milestone grants no release permission.

## Previous implementation: source-expansion observations

The opt-in `measure source-expansion` producer captures rust-analyzer item/impl macro text with invocation provenance and explicit unavailable/disabled states. All nine self-analysis visitor sites return expansions; 14 daemon/framework runs cover 33 sites with matching repeated artifacts and unchanged project sources. Native validation passes 192 normal tests plus both live macro fixtures; unchanged self-metric gates pass.

This is **foundation work, not T4 closure**: effective cfg, compiler target/owner binding, generated origins and fact integration remain unimplemented. Expansion replies never clear measurement abstention or supply authoritative scores. Strict self-policy still fails on partial evidence. See `docs/source-expansion.md` and `acceptance/source-expansion-results.json`. No acceptance status or release permission changes.

## Trust acceptance T1–T3; T4 remains unmet

T1's scoped package/target and SARIF regressions are reconfirmed. T2 has deterministic edges/semantic evidence and a virtual-workspace correctness regression. T3 now abstains on unresolved type owners, opaque macros/attributes, cfg and unavailable semantics; unknown totals/scores are null rather than zero. Positive controls retain known zero and package/target isolation.

The final T3 campaign passes all 182 commands and 91 artifact comparisons across 13 static producers on six local daemons plus daemon-framework. All project sources, including existing nm-daemon edits, are preserved. Native tests: 184 plus the live macro test; eight Python acceptance tests pass. See `acceptance/trust-t3-review.md`, `acceptance/trust-t3-results.json` and `contracts/semantic-abstention.md`.

Acceptance is **not complete**: 6/25 criteria are evidenced; 19 remain unresolved. T4 fails the strict local policy gate because self-evidence is honestly partial (16/17 workflow commands pass); clean/platform/latest-stable certification is still outstanding. No gate, source scope or budget was weakened. No push is permitted yet.

## Previous: acceptance closure steps 1–5

All 17 Linux Rust 1.95 workflow commands passed from a clean source snapshot, including 171 normal workspace tests plus the live macro test, audit/deny, installed-package smoke, coverage-backed measurement, self metrics, SARIF, policy and repeated resource profiles. Source-size rebaselining is explicit in `acceptance/source-budget-review.md`; other self budgets were not raised. Batches now reject competing writers and escaping filenames.

At that checkpoint, 4/25 required criteria were evidenced and 21 remained unresolved. No push is permitted yet. See the repository ledger `acceptance/criteria.json`, `acceptance/step-5.md`, and packaged operational limits in `contracts/operational-scope.md`. Independent calibration/intent review and broader semantic, operational and platform obligations remain open.

## Historical implementation record

The review's phases 0–4 are executed as phases 1–5. Each phase has its own commit. These are implementation iterations; full plan acceptance remains open (see `contracts/upgrade-validation.md`).

## Phase 1 — Trust repairs
- Type/implementation aggregation is qualified by package and target; ambiguous names are not joined.
- SARIF validates artifact schemas and input freshness before exporting evidence.
- Workspace tests pass (158 passed, one ignored); new package/target collision and stale-export regressions are covered.
- Subsequent phases add normalized finding identity, comparative evidence and calibration. Sequential artifact publication is not claimed to be a fully atomic transaction.

## Phase 2 — Evidence boundaries
- Added native capability-v1 types and the shared conformance corpus; updated the additive artifact-envelope snapshot.
- Artifacts and SARIF share source-anchor/occurrence identities. SARIF preserves advisory findings and records accepted waivers rather than silently dropping them; unavailable/timed-out checks cannot be waived in the exporter.
- Workspace suite: 160 passed, one ignored.
- The initial capability ledger is project-input scoped. Full per-rule applicability and symbol-semantic identity (beyond source anchors) remain outstanding.

## Phase 3 — Measurement parity
- Review v3 includes a full-project finding snapshot and `--baseline-review` comparison, separate from changed-file/hunk presentation. This consumes an explicitly saved base review, not an automatic Git checkout.
- Criterion comparison preserves uncertainty/sample evidence and matching environment. Policy cannot reinterpret unknown/noisy comparisons as point-estimate regressions.
- Source-linked external findings now enter configured policy rules and SARIF with adapter namespaces; unverified inputs remain unusable for gating.
- Workspace suite: 163 passed, one ignored. Usage/limitations: `contracts/comparison-evidence.md`.

## Phase 4 — Calibration and anti-gaming
- Added native labeled-evaluation conformance plus actual Rust idiom cases and movement/rename mutations through the syntax fact backend.
- Isolated modules no longer claim observed reuse; public surface and implementation size are explicit. Legacy architecture pressure remains separate and advisory.
- Workspace suite: 166 passed, one ignored.
- Independent real-world calibration and behavior-reviewed refactor pairs are still required before claiming calibrated ranking; `contracts/calibration-design.md` specifies this boundary.

## Phase 5 — Simplification and delivery validation
- Type-health production uses borrowed typed records and typed score inputs, serialized once at the boundary. Removed JSON-key sorting, summary/file cloning and duplicate pressure calculation while retaining the flat wire format.
- External report validation/normalization has explicit boundaries and no redundant whole-row-vector clone. Cohesion exposes its syntactic/advisory basis.
- 166 workspace tests passed, plus the normally ignored live rust-analyzer macro test; strict Clippy, formatting, docs, bundled-helper parity and package smoke passed.
- Source-only first/repeat CLI/memory benchmark: 499/505 ms for 100 generated files, with prebuilt helpers and no project-code execution.
- **Self-metric gate still fails:** hotspot 98.95 > 70, cognitive complexity 12 > 11, source lines 21192 > 17289. Existing thresholds were not raised to hide growth. The old isolated-module leverage floor was replaced by an observed-reuse invariant, not a new arbitrary score floor.
- Full Rust CI and broad plan acceptance are not certified; details and remaining work are in `contracts/upgrade-validation.md`.
