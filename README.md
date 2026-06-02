# rust-quality-lens

Reusable measurement JSON producers for Rust projects.

The goal is to make quality, correctness, and architecture-map
measurements portable across Rust projects without requiring each project to
vendor dashboard code or project-specific scripts.

## What It Measures

- complexity hotspots
- token and AST clone groups
- Rust escape hatches such as unsafe, FFI, raw memory, globals, and lint suppressions
- type health for broad structs, large enums, and wide impl surfaces
- code locality and hidden coupling
- architecture leverage and style pressure
- correctness catalog coverage
- architecture risk map data

The architecture map uses a versioned risk model so raw facts and derived risk
scores remain separate. See `docs/risk-model-v1.md` for the current weights.
Recent extractor and scoring upgrades are summarized in
`docs/measurement-upgrades.md`.

## Quick Start

Create a config in the Rust project you want to inspect:

```toml
project_name = "my-rust-project"
project_root = "."
source_roots = ["src"]
output_dir = "target/analysis"
```

Run all JSON producers:

```powershell
cargo run --bin rqlens -- measure all --config rqlens.toml
```

Run one producer:

```powershell
cargo run --bin rqlens -- measure escape-hatches --config rqlens.toml
cargo run --bin rqlens -- measure type-health --config rqlens.toml
cargo run --bin rqlens -- measure correctness --config rqlens.toml
cargo run --bin rqlens -- measure correctness-run --config rqlens.toml
cargo run --bin rqlens -- measure map --config rqlens.toml
```

Print the task catalog:

```powershell
cargo run --bin rqlens -- catalog --config rqlens.toml
```

`rust-quality-lens` only writes JSON artifacts. It does not own a dashboard,
local web server, or project UI. The current dashboard and local runner live in
the sibling `project-management-board` repository, which consumes this lens'
catalog and JSON outputs.

For Scratchpad specifically, Scratchpad is now only the Rust editor project
being measured. The Scratchpad checkout provides the source tree, Cargo targets,
Rust probe binaries, benches, and `target/analysis` output directory. Dashboard
orchestration belongs to `project-management-board`.

## External Tools

The CLI is Rust. Syntax-aware facts come from helper binaries in
`rust_helpers/Cargo.toml`, and they run through Cargo automatically. The helper
crate exposes shared path and module-key logic in `rust_helpers/src/lib.rs` so
helper binaries do not duplicate project path normalization.

## Validation

For changes inside this repository, run the same checks the lens uses on
itself:

```powershell
cargo fmt
cargo check --all-targets
cargo test
cargo test --manifest-path rust_helpers/Cargo.toml
cargo run --bin rqlens -- measure all
```

The final command refreshes `target/analysis/*.json`. Check
`correctness_review.json.summary` for failed or unknown tests and
`map.json.meta.summary.artifact_status` for missing or stale inputs. Optional
`slowspots.json` may be absent; in that case performance facts stay unknown
while the rest of the map remains usable.
