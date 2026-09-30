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
