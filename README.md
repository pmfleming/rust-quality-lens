# rust-quality-lens

Reusable measurement JSON producers for Rust projects.

This repository was extracted from Scratchpad's local measurement system. The
first goal is to make quality, correctness, and architecture-map measurements
portable across Rust projects without requiring each project to vendor a pile of
scripts.

## What It Measures

- complexity hotspots
- token and AST clone groups
- Rust escape hatches such as unsafe, FFI, raw memory, globals, and lint suppressions
- type health for broad structs, large enums, and wide impl surfaces
- code locality and hidden coupling
- architecture leverage and style pressure
- correctness catalog coverage
- architecture risk map data

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
python -m rust_quality_lens.cli measure --config rqlens.toml
```

Run one producer:

```powershell
python -m rust_quality_lens.cli measure escape-hatches --config rqlens.toml
python -m rust_quality_lens.cli measure type-health --config rqlens.toml
python -m rust_quality_lens.cli measure correctness --config rqlens.toml
python -m rust_quality_lens.cli measure map --config rqlens.toml
```

Print the task catalog:

```powershell
python -m rust_quality_lens.cli catalog --config rqlens.toml
```

`rust-quality-lens` only writes JSON artifacts. Dashboards, hosted reports, and
other interfaces should live in separate tools that consume those artifacts.

## External Tools

`hotspots` requires `rust-code-analysis-cli` on `PATH`.

The AST clone and leverage analyses use helper binaries from
`rust_helpers/Cargo.toml`. They run through Cargo automatically when `cargo` is
available.

## Current Extraction Status

This is a copy-first extraction. Scratchpad keeps its existing dashboard and uses
`scripts/rqlens.py` as a compatibility wrapper while the reusable JSON producers
stabilize.
