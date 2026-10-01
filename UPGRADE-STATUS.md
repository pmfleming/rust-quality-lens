# Cross-lens upgrade execution

## Latest: trust acceptance T1–T2

T1's scoped package/target and SARIF regressions are reconfirmed. T2 is now evidenced: deterministic graph-edge publication, semantic execution diagnostics kept out of measurement JSON, and a virtual-workspace correctness panic fixed. All 13 static producers match across two runs on each of six local daemons plus daemon-framework (182 successful commands; 91 comparisons). Native tests: 174 plus the live macro test. See `acceptance/trust-t2-results.json` and `contracts/determinism.md`.

Acceptance is **not complete**: 5/25 criteria are evidenced; 20 remain unresolved. T3 still has unresolved-semantics counterexamples and T4's full platform matrix is not certified. No push is permitted yet.

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
