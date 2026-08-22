# Contributing

Thank you for improving rust-quality-lens.

## Change expectations

Discuss substantial rule, artifact-contract, or risk-model changes in an issue
first. Keep pull requests focused and add regression tests for extraction or
scoring changes.

Generated evidence must keep verified gates, static findings, heuristic signals,
and contextual evidence distinct. Missing inputs must not be reported as
passing.

When changing a public interface:

- update schemas and conformance snapshots for JSON contract changes;
- version, document, and calibrate scoring-formula changes;
- update the README, documentation index, and changelog for commands,
  configuration, artifacts, or interpretation changes.

## Required checks

Run these commands from the workspace root:

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

Coverage requires `cargo-llvm-cov` and `llvm-tools-preview`. The CI workflow is
the authoritative required-gate list.

If a change affects Scratchpad integration, also run:

```bash
cargo run --bin rqlens -- measure all --config examples/scratchpad.toml
cargo run --bin rqlens -- verify --config examples/scratchpad.toml
```

## Conduct and security

Follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report vulnerabilities through
the private process in the [Security Policy](SECURITY.md), not in public issues.
