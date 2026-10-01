# Rust validation map

Latest clean Linux Rust 1.95 workflow: all 17 recorded commands passed, including 171 normal tests plus the live macro test, audit/deny, coverage-backed measurement, self metrics, installed-package smoke, SARIF and policy. See `step-5.md` and `criteria.json`; macOS/Windows and forward-version validation remain outstanding.

Baseline revision: a222415. Run commands inside `nix develop --command bash` when Rust tools are not on PATH.

| Criteria | Implementation / regression starting points | Reproduce |
|---|---|---|
| T1–T3, P1–P4 | src/producers/type_health_tests.rs, src/performance.rs, src/review/comparison.rs, src/external.rs; tests/cli.rs | cargo test --workspace --locked |
| E1–E6 | src/contracts/, src/policy/, src/sarif.rs, src/capability_evidence.rs | cargo test --workspace --locked |
| C1–C4 | src/labeled_evaluation_tests.rs, contracts/quality-corpus.json | cargo test --locked labeled_evaluation -- --nocapture; independent review still required |
| O1–O7 | src/measurement/, src/command_runner.rs, src/semantic.rs; src/semantic_macro_tests.rs, tests/cli.rs | cargo test --workspace --locked; python3 scripts/benchmark-measure.py |
| T4 | .github/workflows/ci.yml; scripts/check-self-metrics.sh, scripts/check-package.sh | cargo fmt --all -- --check; cargo clippy --workspace --all-targets --locked -- -D warnings; cargo test --locked semantic::macro_tests -- --ignored; cargo doc --workspace --no-deps --locked; bash scripts/check-bundled-helpers.sh; bash scripts/check-package.sh; complete native CI workflow |

Self budgets now pass. The source-size ceiling alone was explicitly rebaselined after a same-analyzer historical comparison (`source-budget-review.md`); hotspot, complexity, duplication and other ceilings were not raised. This is a documented policy decision, not evidence of calibrated code quality. Saved review snapshots support comparison; automatic base checkout is not implied. Full per-rule applicability and negative evidence remain open.
