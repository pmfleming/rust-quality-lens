# rust-quality-lens

Reusable measurement JSON producers for Rust projects.

The goal is to make quality, correctness, and architecture-map
measurements portable across Rust projects without requiring each project to
vendor dashboard code or project-specific scripts.

## What It Measures

- complexity hotspots
- token, AST, module-responsibility, and test-body duplication groups
- Rust escape hatches such as unsafe, FFI, raw memory, globals, and lint suppressions
- type health for broad structs, large enums, and wide impl surfaces
- code locality and hidden coupling
- architecture leverage and style pressure
- Cargo target and tool entrypoint awareness
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

Or scaffold one:

```powershell
cargo run --bin rqlens -- init
```

Run all JSON producers:

```powershell
cargo run --bin rqlens -- measure all --config rqlens.toml
```

Run one producer:

```powershell
cargo run --bin rqlens -- measure escape-hatches --config rqlens.toml
cargo run --bin rqlens -- measure clones --config rqlens.toml
cargo run --bin rqlens -- measure type-health --config rqlens.toml
cargo run --bin rqlens -- measure correctness --config rqlens.toml
cargo run --bin rqlens -- measure correctness-run --config rqlens.toml
cargo run --bin rqlens -- measure locality --config rqlens.toml
cargo run --bin rqlens -- measure leverage --config rqlens.toml
cargo run --bin rqlens -- measure map --config rqlens.toml
```

Print the task catalog:

```powershell
cargo run --bin rqlens -- catalog --config rqlens.toml
```

Print the config schema:

```powershell
cargo run --bin rqlens -- config-schema
```

Print artifact schemas:

```powershell
cargo run --bin rqlens -- artifact-schema
cargo run --bin rqlens -- artifact-schema map
```

Review changed Rust files:

```powershell
cargo run --bin rqlens -- review --changed-since main --config rqlens.toml
cargo run --bin rqlens -- review --diff-file pr.diff --config rqlens.toml
```

The review command writes `target/analysis/review.json`. It runs the standard
project extraction and filters hotspot, clone, escape-hatch, and type-health
rows to changed Rust files so module keys remain comparable to full-project
measurements. Review scope also reports changed tool entrypoints in
`scope.entrypoints`.

## Reading Outputs

`clones.json` reports multiple clone and duplication engines:

- `token`: normalized token-window repeats
- `ast`: repeated function or method structure
- `module-responsibility`: modules with similar API, type, impl, and dependency footprints
- `test-ast`: repeated non-trivial test body structures discovered from Cargo test targets

Syntax-backed outputs include target metadata when available:

- `target_kind`: `lib`, `bin`, `test`, `bench`, `example`, or `module`
- `entrypoint_kind`: runnable target kind for tool/test/example/bench entrypoints
- `is_entrypoint`: whether the module is a runnable tool-style entrypoint

Map, locality, leverage, clone responsibility rows, and review output preserve
this metadata. Entrypoints remain visible in the architecture map, but outbound
orchestration and a small amount of layer-boundary crossing receive a scoring
allowance so `main.rs`, `src/bin/*.rs`, and explicit Cargo `[[bin]]` tools are
not judged like ordinary domain modules.

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
cargo clippy --workspace --all-targets
cargo test
cargo test --manifest-path rust_helpers/Cargo.toml
cargo run --bin rqlens -- measure all
```

The final command refreshes `target/analysis/*.json`. Check
`correctness_review.json.summary` for failed or unknown tests and
`map.json.meta.summary.artifact_status` for missing or stale inputs.
