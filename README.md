# rust-quality-lens

Reusable measurement JSON producers for Rust projects.

This repository was extracted from Scratchpad's former local measurement
system. The goal is to make quality, correctness, and architecture-map
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

`rust-quality-lens` only writes JSON artifacts. It does not own a dashboard,
local web server, or project UI. The current dashboard and local runner live in
the sibling `project-management-board` repository, which consumes this lens'
catalog and JSON outputs.

For Scratchpad specifically, Scratchpad is now only the Rust editor project
being measured. The Scratchpad checkout provides the source tree, Cargo targets,
Rust probe binaries, benches, and `target/analysis` output directory. Dashboard
orchestration belongs to `project-management-board`.

## External Tools

`hotspots` requires `rust-code-analysis-cli` on `PATH`.

The AST clone and leverage analyses use helper binaries from
`rust_helpers/Cargo.toml`. They run through Cargo automatically when `cargo` is
available.

## Repository Boundaries

- `rust-quality-lens`: reusable Rust quality, correctness, and map JSON
  producers.
- `scratchpad-performance-lens`: Scratchpad-specific performance, telemetry,
  and overview JSON producers.
- `project-management-board`: React/TypeScript dashboard, task catalog, and
  local run API for invoking the lenses.
- `scratchpad`: the Rust editor application under measurement, not the owner of
  measurement scripts or dashboard UI.
