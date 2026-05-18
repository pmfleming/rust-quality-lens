# rust-quality-lens

Reusable quality measurement tools for Rust projects.

This repository was extracted from Scratchpad's local measurement system. The
first goal is to make the quality measurements portable across Rust projects
without requiring each project to vendor a pile of scripts.

## What It Measures

- complexity hotspots
- token and AST clone groups
- Rust escape hatches such as unsafe, FFI, raw memory, globals, and lint suppressions
- type health for broad structs, large enums, and wide impl surfaces
- code locality and hidden coupling
- architecture leverage and style pressure

## Quick Start

Create a config in the Rust project you want to inspect:

```toml
project_name = "my-rust-project"
project_root = "."
source_roots = ["src"]
output_dir = "target/analysis"
```

Run all quality producers:

```powershell
python -m rust_quality_lens.cli measure --config rqlens.toml
```

Run one producer:

```powershell
python -m rust_quality_lens.cli measure escape-hatches --config rqlens.toml
python -m rust_quality_lens.cli measure type-health --config rqlens.toml
```

Print the task catalog:

```powershell
python -m rust_quality_lens.cli catalog --config rqlens.toml
```

Serve the bundled dashboard assets:

```powershell
python -m rust_quality_lens.cli serve --config rqlens.toml --port 8765
```

## External Tools

`hotspots` requires `rust-code-analysis-cli` on `PATH`.

The AST clone and leverage analyses use helper binaries from
`rust_helpers/Cargo.toml`. They run through Cargo automatically when `cargo` is
available.

## Current Extraction Status

This is a copy-first extraction. Scratchpad can keep its existing script names as
compatibility wrappers while the reusable package stabilizes.
