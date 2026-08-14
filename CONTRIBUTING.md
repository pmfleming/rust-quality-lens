# Contributing

Thank you for improving rust-quality-lens.

## Before opening a pull request

Discuss substantial rule, artifact-contract, or risk-model changes in an issue first. Keep pull requests focused and include regression tests for changed extraction behavior.

Run the repository gates from the workspace root:

```bash
cargo fmt --all -- --check
bash scripts/check-bundled-helpers.sh
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo run --locked --bin rqlens -- verify
cargo run --locked --bin rqlens -- measure all
bash scripts/check-self-metrics.sh
cargo run --locked --bin rqlens -- check \
  --fail-on partial \
  --fail-on test-failure \
  --fail-on practice-failure \
  --fail-on reliability-finding
bash scripts/check-package.sh
```

Coverage measurement requires `cargo-llvm-cov` and `llvm-tools-preview`. The CI
workflow is the authoritative required gate list.

When Scratchpad behavior is affected, also run:

```bash
cargo run --bin rqlens -- measure all --config examples/scratchpad.toml
cargo run --bin rqlens -- verify --config examples/scratchpad.toml
```

Generated findings must distinguish verified gates, static findings, and
heuristic signals. Missing inputs must not be reported as passing. Changes to
JSON contracts require schema and conformance-snapshot updates. Changes to score
formulas require a risk-model version, documentation, and calibration evidence.
Update the [documentation index](docs/README.md), README examples, and changelog
when a public command, configuration key, artifact, or interpretation changes.

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report vulnerabilities
according to the [Security Policy](SECURITY.md), not in public issues.
