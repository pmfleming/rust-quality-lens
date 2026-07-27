# Contributing

Thank you for improving rust-quality-lens.

## Before opening a pull request

Discuss substantial rule, artifact-contract, or risk-model changes in an issue first. Keep pull requests focused and include regression tests for changed extraction behavior.

Run the repository gates:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run --bin rqlens -- verify
cargo run --bin rqlens -- measure all
cargo run --bin rqlens -- check --fail-on partial --fail-on test-failure
```

When Scratchpad behavior is affected, also run:

```bash
cargo run --bin rqlens -- measure all --config examples/scratchpad.toml
cargo run --bin rqlens -- verify --config examples/scratchpad.toml
```

Generated findings must distinguish verified gates, static findings, and heuristic signals. Missing inputs must not be reported as passing. Changes to JSON contracts require schema and conformance-snapshot updates. Changes to score formulas require a risk-model version and calibration evidence.

Please follow `CODE_OF_CONDUCT.md` and do not disclose security issues in public issues.
