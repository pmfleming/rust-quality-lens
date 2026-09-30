# Cross-lens upgrade execution

The review's phases 0–4 are executed as phases 1–5. Each phase has its own commit.

## Phase 1 — Trust repairs
- Type/implementation aggregation is qualified by package and target; ambiguous names are not joined.
- SARIF validates artifact schemas and input freshness before exporting evidence.
- Workspace tests pass (158 passed, one ignored); new package/target collision and stale-export regressions are covered.
- Subsequent phases add normalized finding identity, comparative evidence and calibration. Sequential artifact publication is not claimed to be a fully atomic transaction.
