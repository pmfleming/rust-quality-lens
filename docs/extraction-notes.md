# Extraction Notes

`rust-quality-lens` carries reusable JSON-producing quality, correctness, and
architecture-map measurements for Rust projects.

Included:

- `hotspots`
- `clones`
- `escape-hatches`
- `type-health`
- `locality`
- `leverage`
- `correctness`
- `map`
- shared catalog and artifact-writing support
- Rust helper binaries for syntax facts and AST hashing
- shared helper-side path and module-key logic in `rust_helpers/src/lib.rs`
- shared CLI-side modules for config, catalog, facts, producers, artifacts,
  risk scoring, and utilities

Not included:

- dashboard/server/UI assets
- Scratchpad-specific app package diagnostics

Left in Scratchpad for now:

- performance probes
- capacity/resource probes
- Scratchpad-specific profile binaries

The CLI is now split into smaller modules while preserving the command and
artifact contracts consumed by dashboards and local runners. Performance inputs
remain project-specific unless a measured project provides compatible
`slowspots.json` data.
