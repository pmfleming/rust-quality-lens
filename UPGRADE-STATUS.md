# Cross-lens upgrade execution

The review's phases 0–4 are executed as phases 1–5. Each phase has its own commit.

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
